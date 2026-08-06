# Threat Model

[architecture.md](architecture.md) lays out what the program is built from.
This document is about the adversary: anyone can build and submit any
transaction that names this program's ID, with any accounts and any bytes
they like. The runtime enforces a fixed set of guarantees before the
program ever runs; everything else is the program's problem. This is the
same split [transaction-model.md](transaction-model.md) draws between
signature verification and authorization, generalized to every input the
program receives — the point of writing this down before any handler exists
is to make sure every validation checklist in this codebase (starting with
[cpi-model.md](cpi-model.md#required-checks-before-calling-into-the-token-program))
is answering an actual threat, not an imagined one.

## Trust boundary

The attacker is anyone who can submit a transaction — no special
permission, stake, or relationship to the escrow is required to construct
one that invokes this program. "The attacker" therefore includes: a
dishonest taker, a dishonest maker, an uninvolved third party, or a client
bug that isn't malicious but behaves identically to an attack from the
program's point of view. The program cannot distinguish intent — it can
only distinguish valid account states and correctly-authorized signers from
invalid ones. Every check in this codebase exists to make that distinction
regardless of which of those four the caller actually is.

## What the attacker controls

Everything a client assembles before submitting a transaction is
attacker-controlled, in the sense that the program must treat it as
adversarial input rather than as a well-formed request from a cooperating
party.

- **Instruction bytes.** The raw payload after the discriminant — amounts,
  escrow IDs, any other arguments — can be any bytes at all: truncated,
  padded, wildly out-of-range numbers, or a discriminant that doesn't match
  any real variant. The instruction decoder
  ([architecture.md](architecture.md#instruction-decoder)) must reject
  malformed data rather than assume a well-typed enum arrived.
- **Account order.** The client chooses which account goes in which slot.
  Nothing about a slot's *position* implies what's actually there — see
  [account-model.md](account-model.md#key-pubkey) — so "the account in slot
  2" is never assumed to be the vault just because that's the convention;
  its key must be checked.
- **Supplied account public keys.** Every pubkey in the instruction —
  claimed vault, claimed state account, claimed mint — can point at any
  existing account, including ones with no relationship to this escrow at
  all. A pubkey is an identity, not a guarantee (see
  [cpi-model.md](cpi-model.md#why-validating-only-a-token-accounts-public-key-is-not-sufficient)).
- **Duplicate accounts.** The same account can be passed into more than one
  slot in a single instruction — e.g. supplying the maker's own token
  account as both the "vault" and the "destination" in `Exchange`. Handlers
  that assume two account slots refer to two distinct accounts can be
  tricked into self-referential transfers or double-counted balance checks
  unless each slot is validated independently and, where distinctness
  matters, explicitly compared against the others.
- **Token accounts.** Any token account the attacker owns or controls can
  be substituted in — including one for the wrong mint, one with a frozen
  state, or one whose authority isn't who the program expects. Being a
  genuine, Token-Program-owned account proves nothing beyond that (checks 3
  through 5 in [cpi-model.md](cpi-model.md#required-checks-before-calling-into-the-token-program)).
- **Mint accounts.** An attacker can supply a real, valid SPL mint that
  simply isn't the mint this escrow was created for — a "valid mint" and
  "the correct mint" are different claims, and only the second one is safe
  to trust without an explicit comparison against the mint recorded in
  `EscrowState`.
- **CPI program accounts.** The account passed in the "Token Program"
  slot can be any account at all, including a malicious program with the
  same instruction-call shape as the real Token Program. Nothing prevents a
  client from substituting it unless the program checks the key against
  `spl_token::ID` and confirms it's executable, per
  [cpi-model.md](cpi-model.md#required-checks-before-calling-into-the-token-program).
- **Transaction construction.** Which instructions are bundled together,
  in what order, alongside what other (possibly unrelated, possibly
  adversarial) instructions — all attacker-chosen. Atomicity
  ([transaction-model.md](transaction-model.md#transaction-atomicity))
  means a bundled instruction can't corrupt state on its own, but it also
  means the program can't assume its instruction is the only thing
  happening in that transaction.
- **Which optional signers sign.** Any account can be included as a signer
  or not, so long as the transaction as a whole is validly signed for
  whichever accounts *are* marked as signers. An attacker can simply choose
  not to include a signature for an account whose authorization would
  normally be required, and the runtime raises no objection on its own —
  the *specific* accounts that must sign for a given instruction (maker for
  `Cancel`, taker for `Exchange`) is a rule this program has to enforce
  itself; see the worked `Cancel` example in
  [transaction-model.md](transaction-model.md#signature-verification-vs-authorization).

## What the attacker does not control

These are runtime-enforced, before the program's code ever runs. They are
not weaker guarantees than the program's own checks — they're guarantees
the program is entitled to *build on top of*, precisely because nothing a
client does can forge them.

- **Runtime signature verification.** `is_signer == true` on an account is
  never fabricated by a client — the runtime validates a real ed25519
  signature against that account's real private key before the transaction
  is allowed to execute at all (see
  [transaction-model.md](transaction-model.md#transaction-signatures)).
  What the attacker *does* control is which accounts they choose to sign
  with (see above) — the signature itself, once present, can't be spoofed.
- **Account owner fields.** `AccountInfo.owner` cannot be set by an
  arbitrary transaction — only the account's *current* owning program can
  reassign it (see [account-model.md](account-model.md#owner-pubkey)). An
  attacker cannot make a lookalike account pass an `owner == spl_token::ID`
  or `owner == program_id` check by simply asserting that it should.
- **Program executable code.** The bytecode actually running at a given
  program ID is fixed by deployment, not by anything in the attacker's
  transaction. An attacker can point the "Token Program" slot at a
  different account (see above), but they cannot make the *real*
  `spl_token::ID` execute code other than the real Token Program's.
- **PDA derivation rules.** `find_program_address` /
  `create_program_address` are deterministic, runtime-implemented math over
  seeds and a program ID (see
  [pda-design.md](pda-design.md#determinism-and-the-canonical-bump)). An
  attacker cannot produce an account that re-derives to the escrow PDA
  without controlling the actual seed inputs (maker pubkey, escrow ID,
  program ID) that legitimately produce it — and even if the resulting
  address happens to be off-curve for some other seed combination, it still
  won't equal the specific PDA this escrow's seeds produce.
- **Atomic transaction rollback.** If any instruction in the transaction
  fails, the runtime guarantees every account mutation in that transaction
  is discarded (see
  [transaction-model.md](transaction-model.md#transaction-atomicity)). An
  attacker cannot construct a transaction that "partially succeeds" from
  the program's perspective — either every check the program performs
  passes and the whole transaction's effects commit, or one fails and none
  of them do.

## Why this split matters

Every validation checklist elsewhere in this codebase — the seven CPI
checks in [cpi-model.md](cpi-model.md), the per-account trust boundaries in
[account-model.md](account-model.md) — exists specifically to cover the
"what the attacker controls" list above. None of those checks are
compensating for anything in the "does not control" list; re-verifying a
signature the runtime already verified, or re-checking that PDA math is
deterministic, would be validating something that was never actually in
question. The program's validation logic should be exactly as large as the
attacker-controlled surface, no larger and no smaller.
