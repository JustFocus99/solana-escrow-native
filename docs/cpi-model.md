# CPI Model

[pda-design.md](pda-design.md) ended with the signer-seeds array the escrow
program will hand to `invoke_signed`. This doc covers the other half of that
picture: what a Cross-Program Invocation actually is, the shape of the SPL
Token CPIs the escrow program depends on, and — the part that actually
matters for security — everything the escrow program must check about the
Token Program and the accounts it hands it *before* making that call. The
runtime enforces some of this automatically (same pattern as
[account-model.md](account-model.md) and
[transaction-model.md](transaction-model.md)); the rest is on the program.

## What a CPI is

A Cross-Program Invocation is one program calling another program's
instruction handler mid-execution, the same way the top-level transaction
called the first program. The escrow program never touches SPL token
account bytes directly — it isn't the owner of the vault or any other token
account (see
[account-model.md](account-model.md#why-the-escrow-program-owns-the-state-account-but-not-the-vault)),
so it has no write access to that data. Instead, it builds an SPL Token
instruction (`Transfer`, `CloseAccount`, etc.) and asks the Token Program —
the account's actual owner — to perform the mutation on its behalf. The
Token Program is the only thing that ever writes those bytes; the escrow
program only ever *directs* it to.

This is also why [compute-notes.md](compute-notes.md#what-consumes-compute)
flags every CPI as an added compute cost: the runtime is fully re-entering
another program's logic, not just returning a value.

## The CPI flow

```
Escrow program
    ↓
Build SPL Token transfer instruction
    ↓
Validate the Token Program account
    ↓
Supply token accounts and authority
    ↓
Call invoke or invoke_signed
    ↓
SPL Token Program validates token state
    ↓
SPL Token Program updates balances
```

Two things worth noticing about the ordering here:

- **"Validate the Token Program account" happens before the call, not
  after.** By the time `invoke`/`invoke_signed` runs, it's too late to
  matter *which* program actually ends up executing — the runtime will
  invoke whatever account the escrow program handed it as the target,
  trusting the *program* to have picked correctly. Skipping this check
  doesn't cause a crash; it causes the escrow program to hand token
  authority to whatever program the caller managed to substitute in that
  slot.
- **"SPL Token Program validates token state" is a second, independent
  validation pass**, done by different code the escrow program doesn't
  control. It will reject its own set of bad states (insufficient balance,
  frozen account, mismatched mint on the CPI's own terms). But the Token
  Program validating its *own* invariants is not a substitute for the
  escrow program validating that these are the *right* accounts for *this
  escrow* — same distinction as
  [signature verification vs. authorization](transaction-model.md#signature-verification-vs-authorization):
  the Token Program proves the transfer is *mechanically* valid, not that
  it's the transfer the escrow program meant to authorize.

## `invoke` vs. `invoke_signed`

Both functions do the same thing at the call boundary — re-enter another
program's handler with a chosen set of accounts and instruction bytes. They
differ only in how the *authority* behind the CPI proves it's allowed to
act:

- **`invoke(...)`** — use this when the transfer authority is a transaction
  signer. The authority's `is_signer == true` was already established by
  the runtime's transaction-level signature verification (see
  [transaction-model.md](transaction-model.md#transaction-signatures)), and
  that signer-ness carries through into the CPI unchanged. This is the case
  for a `Deposit`: the maker's wallet signed the outer transaction, so the
  maker can authorize moving tokens out of their own token account with a
  plain `invoke`.
- **`invoke_signed(...)`** — use this when the transfer authority is a PDA
  controlled by your program. A PDA has no private key and never appears as
  a transaction signer, so there is no transaction-level signature to carry
  through. Instead, the escrow program supplies the exact seeds (including
  the stored canonical bump — see
  [pda-design.md](pda-design.md#signer-seeds-for-invoke_signed)) that derive
  the PDA; the runtime recomputes the address from those seeds and, if it
  matches, treats the PDA as having signed *this specific CPI only*. This is
  the case for `Exchange` and `Cancel`: the vault's authority is the escrow
  PDA, so releasing its tokens requires `invoke_signed` with the escrow
  seeds.

A PDA's "signature" from `invoke_signed` is scoped narrowly — it authorizes
only the one CPI call it's passed to, made by the program that owns those
seeds, and proves nothing about any other account or instruction in the
transaction.

## Required checks before calling into the Token Program

None of these are optional, and none of them are enforced by the runtime on
the escrow program's behalf — they're payoffs of the account-model
reasoning from [account-model.md](account-model.md), applied specifically to
the accounts a Token CPI touches.

1. **Token Program account key equals the official SPL Token Program ID.**
   Compare the account's `key` against the known `spl_token::ID` constant.
   Nothing else in this list means anything if the "Token Program" being
   invoked isn't actually the Token Program.
2. **Token Program account is executable.** Confirms the account holds
   loaded program bytecode rather than being some ordinary data account that
   happens to share the right pubkey layout expectations — checking the key
   alone doesn't rule out a non-executable account being passed in that
   slot. See [account-model.md](account-model.md#executable-bool) — key and
   executable-ness are two separate facts and checking only one leaves a
   gap.
3. **Token accounts are owned by the Token Program.** Every token account
   involved (vault, maker's token account, taker's token account) must have
   `AccountInfo.owner == spl_token::ID` before its bytes are unpacked as
   `spl_token::state::Account`. Without this, a lookalike account with the
   same byte layout but a different owner could be substituted in.
4. **Token account mints match expected mints.** After unpacking, compare
   the account's `mint` field against the mint the escrow actually expects
   (e.g. the vault's mint must equal the mint recorded in `EscrowState` at
   `Initialize` time, and the taker's receiving account must match it too).
   Two token accounts can both be legitimate, Token-Program-owned accounts
   and still be for entirely different tokens.
5. **Token authorities match expected signer or PDA.** The unpacked
   `authority` field on the token account being debited must equal whichever
   party is supposed to be allowed to move its funds — the maker's wallet
   for their own token account, or the escrow PDA for the vault. This is an
   application-level field inside the Token Program's account data, distinct
   from `AccountInfo.owner`; see the ownership-vs-authority distinction in
   [account-model.md](account-model.md#why-the-escrow-program-owns-the-state-account-but-not-the-vault).
6. **Writable privileges are present.** Any token account being debited or
   credited must have `is_writable == true` in the accounts passed to the
   CPI, or the Token Program's own write will fail — see
   [account-model.md](account-model.md#is_writable-bool).
7. **Signer privileges are present.** For an `invoke`-based transfer, the
   authority account must have `is_signer == true`. For an
   `invoke_signed`-based transfer, the *seeds* supplied must actually
   re-derive the PDA the vault's authority is set to — an incorrect seed set
   simply fails to reproduce the right address and the CPI is rejected by
   the runtime before the Token Program ever runs.

## Why validating only a token account's public key is not sufficient

A token account's `key` is just an address — it carries no guarantee about
what's stored at that address today. Checking only that a client passed
"the pubkey the escrow expected" (e.g. comparing against a value cached
in `EscrowState`) skips every fact that actually determines whether the
account is safe to hand to a CPI:

- **Ownership**: the account at that key might no longer be owned by the
  Token Program at all — nothing stops an attacker from pointing the escrow
  at *some* account with a matching pubkey expectation if the program never
  independently checks `owner == spl_token::ID`. (In practice this means:
  never trust a cached pubkey without re-checking the live account's
  `owner` field — see check 3 above.)
- **Layout vs. meaning**: even a genuine Token-Program-owned account at the
  right key doesn't guarantee it's for the *right mint* — the key alone says
  nothing about the `mint` field inside the account's data (check 4).
- **Authority**: the key doesn't reveal who currently has authority over the
  account's balance. A token account can be reassigned to a new authority
  after creation (`SetAuthority`); a pubkey check performed once, long ago,
  says nothing about the authority as of *this instruction* (check 5).
- **Access flags**: a matching key says nothing about whether the account
  was actually passed into *this* instruction as writable and/or signed —
  those are properties of the current transaction's account metadata, not
  the account itself (checks 6 and 7).

In short: the pubkey identifies *which* account is being referenced, but
identity is not the same as current, verified state. Everything the escrow
program actually depends on — who owns it, what it holds, who can move it,
what permissions this transaction granted it — has to be read and checked
fresh from the account passed into the instruction, the same reasoning
[account-model.md](account-model.md) applies to every other account type
this program handles.

Hour 6 is complete once that last paragraph can be explained without
looking it up: a pubkey is an identity check, not a state check, and a CPI
into the Token Program is only as safe as the seven checks made *before*
`invoke`/`invoke_signed` is called.
