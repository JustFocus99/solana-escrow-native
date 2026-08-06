# Architecture

This document lays out the shape the escrow program is being built toward,
before any of its business logic exists. Writing this now — while only
[validation.rs](../programs/escrow/src/validation.rs) is implemented — is
deliberate: it's easier to get the boundaries between these pieces right on
paper than to untangle them after the fact. Each section below names the
module that will hold that piece of the program, its job, and which of the
study docs its design decisions come from.

## Module map

| Component | Planned file | Status |
|---|---|---|
| Program entrypoint | `programs/escrow/src/entrypoint.rs` | not yet written |
| Instruction decoder | `programs/escrow/src/instruction.rs` | not yet written |
| Processor router | `programs/escrow/src/processor.rs` | not yet written |
| Escrow state account | `programs/escrow/src/state.rs` | not yet written |
| Escrow PDA | `programs/escrow/src/validation.rs` | **`derive_escrow_pda` implemented** |
| Vault token account | (validated in `processor.rs`, not a module of its own) | not yet written |
| Token Program CPI | `programs/escrow/src/cpi.rs` | not yet written |
| Native Rust integration tests | `programs/escrow/tests/` | `pda_derivation.rs` implemented; others pending |
| Local validator client | not yet created (likely `client/` or `tests/` using `solana-test-validator`) | not yet written |

## Program entrypoint

The single, well-known function the Solana runtime calls to hand control to
this program — the landing point referenced in
[transaction-model.md](transaction-model.md#the-execution-path) as "Runtime
invokes the program." Declared once, via `entrypoint!`, and gated behind a
`no-entrypoint` feature so the crate can also be depended on as an ordinary
library (by tests, by client code, or by another program that wants this
program's types) without pulling in a second copy of the entrypoint symbol.
The entrypoint itself does no validation — it exists only to hand
`(program_id, accounts, instruction_data)` to the processor in the shape the
runtime provides them.

## Instruction decoder

Turns the opaque `instruction_data: &[u8]` byte slice — the "instruction
bytes" from
[transaction-model.md](transaction-model.md#instructions) — into a typed
Rust enum (`EscrowInstruction`), one variant per operation
(`Initialize`, `Deposit`, `Exchange`, `Cancel`). This is the only place in
the program that interprets raw bytes; everywhere else works with the typed
enum. Decoding failures (unknown discriminant, truncated payload) are
rejected here, before any account is touched — the same "fail fast, before
paying for validation or CPIs" reasoning from
[compute-notes.md](compute-notes.md#plan-to-measure-later).

## Processor router

Takes the decoded instruction and the raw `accounts` slice and dispatches to
one handler function per instruction variant. The router itself holds no
business logic — each handler is where the checks from
[account-model.md](account-model.md) and
[cpi-model.md](cpi-model.md#required-checks-before-calling-into-the-token-program)
actually get applied to the specific accounts that instruction expects, in
the specific order the client supplied them (see
[threat-model.md](threat-model.md) for why that order can never be assumed
rather than checked).

## Escrow state account

The program-owned account holding an escrow's data: maker pubkey, vault
pubkey, expected/deposited amount, mint(s), and the stored canonical PDA
bump (see
[pda-design.md](pda-design.md#determinism-and-the-canonical-bump)). This
account only means what its struct says once
`escrow_account.owner == program_id` has been confirmed — the worked example
in
[account-model.md](account-model.md#escrow-state-account) covers exactly
this trust boundary. It's created (and funded to rent-exemption) during
`Initialize` and closed during `Exchange` or `Cancel`.

## Escrow PDA

The escrow state account's address is itself a PDA, derived from
`derive_escrow_pda(maker, escrow_id, program_id)` — the one piece of this
architecture that's already implemented and tested (see
[pda-design.md](pda-design.md) for the seed design, and
[programs/escrow/tests/pda_derivation.rs](../programs/escrow/tests/pda_derivation.rs)
for the six properties it's tested against: determinism, sensitivity to
each seed component, bump reproduction, and canonical-bump reuse). Every
instruction after `Initialize` re-derives this address with
`create_program_address` and the stored bump to confirm the account the
caller supplied really is this escrow's PDA, rather than trusting whichever
account arrived in that slot.

## Vault token account

An SPL Token account, owned by the Token Program (not the escrow program —
see the ownership-vs-authority worked example in
[account-model.md](account-model.md#why-the-escrow-program-owns-the-state-account-but-not-the-vault)),
whose token-account *authority* is set to the escrow PDA. It holds the
maker's deposited tokens for the life of the escrow. The escrow program
never writes its bytes directly; it only ever directs the Token Program to
move or close it, authorized via `invoke_signed` with the PDA's seeds (see
[pda-design.md](pda-design.md#signer-seeds-for-invoke_signed)). There's no
dedicated module for it in the file layout above because it isn't a Rust
type the program defines — it's an SPL Token account the program validates
and directs, the same way it validates any other account handed to it.

## Token Program CPI

The `cpi.rs` module will hold the escrow program's calls into the SPL Token
Program — `Transfer` and `CloseAccount`, primarily — following the flow
documented in
[cpi-model.md](cpi-model.md#the-cpi-flow): validate the Token Program
account itself, validate every token account being passed (owner, mint,
authority, writability), then call `invoke` (when the authority is a
transaction signer, as in `Deposit`) or `invoke_signed` (when the authority
is the escrow PDA, as in `Exchange`/`Cancel`). Centralizing these calls in
one module keeps the seven required checks from
[cpi-model.md](cpi-model.md#required-checks-before-calling-into-the-token-program)
in one place rather than re-implemented per instruction handler.

## Native Rust integration tests

Tests that exercise the program's actual `process_instruction` entry point
in-process — via `solana-program-test`'s native (non-BPF) processor
injection — rather than through a running validator. These are the fast
inner-loop tests: no local validator process, no RPC round-trips, and (per
[compute-notes.md](compute-notes.md)) a natural place to start measuring
per-instruction compute usage once handlers exist.
[pda_derivation.rs](../programs/escrow/tests/pda_derivation.rs) is the first
of these, though it currently only exercises `validation.rs` directly rather
than the processor (there is no processor yet to exercise).

## Local validator client

A thin client-side layer (not yet created) that submits transactions to a
real `solana-test-validator` instance rather than the in-process test
runtime. This is where the program gets exercised the way a real client
eventually will — full RPC round-trip, real blockhash expiry (see
[transaction-model.md](transaction-model.md#recent-blockhashes)), and real
parallel-transaction scheduling (see
[parallel-execution.md](parallel-execution.md)) — as a check that behavior
seen in the fast in-process tests actually holds under the real runtime.
