# Transaction Model

[account-model.md](account-model.md) covered what a program is allowed to
trust about a single account once it has one in hand. This doc covers the
layer above that: how an account ever gets "in hand" in the first place — how
a client's intent turns into instructions, how instructions are bundled into
a transaction, how the runtime decides a transaction is even allowed to run,
and what happens to state if it doesn't finish cleanly. The throughline is
the same as before: the runtime enforces some guarantees automatically, and
leaves the rest for the program to check explicitly.

## Programs

A program is executable code deployed at an address (an account with
`executable == true`, per [account-model.md](account-model.md#executable-bool)).
It is stateless in the sense that matters here: a program has no persistent
memory of its own outside of the accounts it's given on each call. Every
piece of data it needs — whose deal this is, how much was deposited, what
state the escrow is in — has to be either passed in as an account or
recomputed from scratch. This is why the escrow program can't "remember" the
maker between `Initialize` and `Cancel`; it has to re-read the state account
every single invocation and re-verify everything about it.

Programs don't call themselves. They're invoked either directly by a
transaction instruction, or indirectly by another program via a
Cross-Program Invocation (CPI) — which is how the escrow program moves tokens
by invoking the SPL Token Program rather than manipulating token account
bytes itself.

## Instructions

An instruction is a single request to run one program with one set of
accounts and one blob of data. It has exactly three parts:

- **program ID** — which program's code the runtime should invoke.
- **account metadata** — the ordered list of accounts the program is allowed
  to touch during this call, each tagged as signer/non-signer and
  writable/read-only (see [Account metadata](#account-metadata) below).
- **instruction bytes** — an opaque payload the program itself defines and
  parses; the runtime never interprets it. For the escrow program this is
  typically a discriminant byte (which variant: `Initialize` / `Exchange` /
  `Cancel`) followed by any arguments (e.g. the expected amount), serialized
  with something like Borsh.

The instruction is inert data — building one doesn't execute anything. It's
only a description of an action the client wants to happen, which becomes
real once it's placed in a transaction and that transaction is submitted.

## Transactions

A transaction is the unit of execution the runtime actually accepts. It
bundles:

- one or more instructions, executed **in order**,
- the set of signatures authorizing it,
- a recent blockhash bounding its lifetime.

Instructions inside one transaction share the same account-locking and
either all take effect or none do (see
[Transaction atomicity](#transaction-atomicity)). This is why a client
depositing into escrow can combine "create the vault token account" and
"transfer tokens into it" as two instructions in one transaction: if the
transfer fails, the vault creation is rolled back too, rather than leaving an
empty vault account behind.

## Account metadata

Every account referenced by an instruction is passed as an `AccountMeta`,
which is where `is_signer` and `is_writable`
(from [account-model.md](account-model.md#is_signer-bool)) originate — the
client sets these when building the instruction, and the runtime enforces
them:

```rust
AccountMeta::new(escrow_state_pubkey, false)       // writable, not a signer
AccountMeta::new(maker_pubkey, true)               // writable AND a signer
AccountMeta::new_readonly(spl_token::ID, false)    // read-only reference
```

Two things the runtime does with this metadata that are easy to
underappreciate:

- **It's positional.** The program reads accounts out of the passed-in slice
  by index/order, not by name — nothing stops a client from passing the
  wrong account into the "vault" slot. That's exactly why
  [account-model.md](account-model.md) insists the program re-derive and
  compare keys rather than trust position.
- **It drives parallelism.** Two transactions that don't share any writable
  accounts can execute concurrently; any overlap on a writable account
  serializes them. Marking an account `is_writable` when it doesn't need to
  be isn't just imprecise — it needlessly contends with other transactions
  touching that account.

## Transaction signatures

A transaction carries one ed25519 signature per account marked as a signer
in any of its instructions, each produced by that account's private key over
the transaction's message bytes. The runtime verifies all of them before the
transaction is allowed to execute at all — an invalid or missing signature
for a required signer fails the transaction outright, before any program
code runs.

What this buys the program is narrow and specific: for any account with
`is_signer == true`, the program can be certain the holder of that account's
private key authorized *this specific transaction*. It says nothing about
*why* they authorized it or whether they're the right party for *this
program's* rules — see
[Signature verification vs. authorization](#signature-verification-vs-authorization).

PDAs never sign as a client — they have no private key. A program instead
"signs" on a PDA's behalf via `invoke_signed`, supplying the seeds that
derive the PDA; the runtime accepts this as equivalent to a signature only
for CPIs made by the program that owns those seeds. This is how the escrow
program authorizes the vault's outgoing transfer without any human holding
the PDA's key.

## Recent blockhashes

Every transaction embeds a recent blockhash — the hash of a block from
shortly before the transaction was built. It serves two purposes:

- **Replay protection.** The runtime tracks recently-processed transaction
  signatures; combined with the blockhash expiring, this makes it
  impractical to silently resubmit an old, already-executed transaction
  later.
- **Bounded lifetime.** A blockhash is only valid for roughly 150 blocks
  (about a minute or two on mainnet) after it was produced. If a transaction
  isn't confirmed before its blockhash "ages out," validators reject it as
  expired and it must be rebuilt with a fresh blockhash and re-signed.

For the escrow program this mostly matters as a client-side concern (retry
logic must refetch a blockhash, not just resend the same bytes), but it's
worth knowing the failure mode: an expired-blockhash rejection happens
*before* the runtime even gets to invoking any program, so it will never
surface as one of the escrow program's own custom errors.

## Transaction atomicity

All instructions in a transaction succeed, or none of their state changes
take effect. If any instruction returns an error — including a program
returning a custom error via `ProgramError` — the runtime discards every
account mutation the transaction made, across all of its instructions, as if
none of it ran.

This is what makes it safe to compose, in one transaction, steps that would
be dangerous left partially applied: e.g. closing the escrow state account
and releasing the vault's tokens during `Exchange`. If the token transfer
CPI fails partway through, the state account is *not* left closed with the
vault still funded — the whole transaction reverts.

Atomicity is scoped to the transaction, not the instruction sequence in the
abstract — a **failed** transaction still consumes the blockhash slot for
replay-protection purposes and, notably, **still pays the base fee**. Only
the state changes are rolled back; the fee payment is not an "instruction"
in this sense and is settled regardless of success.

## The execution path

```
Client builds an instruction
        ↓
Instruction contains:
program ID
account metadata
instruction bytes
        ↓
Client places instruction in a transaction
        ↓
Required keypairs sign the transaction
        ↓
Transaction includes a recent blockhash
        ↓
Runtime verifies transaction signatures
        ↓
Runtime loads and locks accounts
        ↓
Runtime invokes the program
        ↓
Program validates accounts and bytes
        ↓
Program succeeds or returns an error
        ↓
Runtime commits or rolls back state
```

Two steps are worth calling out by who performs them, since it's the crux of
the next section:

- **"Runtime verifies transaction signatures"** happens *before* the program
  ever runs, and is entirely the runtime's job — the program has no say in
  it and doesn't need to re-verify that a signature is cryptographically
  valid.
- **"Program validates accounts and bytes"** happens *after* invocation, and
  is entirely the program's job — the runtime has no opinion on whether the
  signer it just verified is the *right* signer for this instruction.

## Signature verification vs. authorization

**The runtime proves that an account signed the transaction. The program
decides whether that signer is the correct authority.**

These are two separate checks, done by two separate parties, and conflating
them is a common source of bugs:

- **Signature verification** (runtime): "Is `is_signer == true` for this
  account, backed by a real, valid ed25519 signature over this transaction?"
  This is a cryptographic fact, checked once per transaction, uniform across
  every program that transaction happens to invoke.
- **Authorization** (program): "Given that this account did sign, are they
  *allowed* to do what they're asking this instruction to do?" This is a
  business-logic fact, specific to the escrow program's own rules, and
  nobody checks it unless the program's code explicitly does.

Concretely: on `Cancel`, the runtime will happily confirm that *some*
account with `is_signer == true` really did sign the transaction with its
own private key. But the runtime has no idea that `Cancel` is supposed to be
maker-only — that rule exists only inside the escrow program. If the
program's `Cancel` handler checks `is_signer` on an account but never
compares that account's key against the maker pubkey stored in the escrow
state, then *any* keypair holder can sign a validly-formed transaction,
sail through the runtime's signature check, and cancel someone else's
escrow. The transaction would be perfectly legitimate from the runtime's
point of view — every signature it required really was valid — while being
completely wrong from the escrow program's point of view.

The correct check is therefore always two-part:

```rust
if !maker_account.is_signer {
    return Err(ProgramError::MissingRequiredSignature); // runtime-checkable fact
}
if maker_account.key != &escrow_state.maker {
    return Err(EscrowError::WrongMaker.into());          // program-only fact
}
```

The first line leans on something the runtime already guaranteed by the time
the program is even running. The second line is the one doing the actual
authorization work, and it exists only because the program wrote it.

## Key facts

- Multiple instructions may exist in one transaction.
- Instructions run in transaction order.
- All instructions must succeed for state changes to commit.
- A failed transaction may still pay fees.
- A recent blockhash limits transaction lifetime.
- A valid signature does not automatically mean a signer is authorized for
  your escrow.

Hour 3 is complete once the distinction in
[Signature verification vs. authorization](#signature-verification-vs-authorization)
can be explained without looking it up: the runtime answers "did this
account really sign?"; the program alone answers "should this account's
signature be allowed to do this?"
