# Compute Notes

[transaction-model.md](transaction-model.md) covered *whether* a transaction
is allowed to run. This doc covers a separate budget that applies once it
does: compute units (CU). Every transaction has a compute budget (a default
per-instruction limit, or an explicit higher one requested via the Compute
Budget Program), and every unit of work a program does during invocation
draws down that budget. Run out before the instruction finishes and the
transaction fails with a compute-exhaustion error — same category of failure
as returning `ProgramError`, in that state still rolls back per
[Transaction atomicity](transaction-model.md#transaction-atomicity), except
the fee is still paid.

This is a study pass, not a tuning pass — the goal here is to know *what*
spends compute and *why*, not to change any code. Nothing below should turn
into a premature optimization in the escrow program before it's even
measured.

## What consumes compute

- **Every program instruction consumes compute units**, just by executing —
  there's a baseline cost to invocation itself before any of the program's
  own logic runs.
- **Every CPI consumes additional compute units**, on top of whatever the
  instruction being called costs internally. A CPI is not free scaffolding;
  invoking the SPL Token Program to do a `Transfer` or `CloseAccount` costs
  compute both for the call itself and for the callee's own work. An
  instruction that makes two or three CPIs (e.g. `Exchange` closing the
  vault *and* transferring tokens *and* closing the state account) is
  paying that overhead multiple times.
- **Logging consumes compute units.** `msg!()` and similar aren't free
  diagnostics — formatting and emitting a log line costs CU, and the cost
  scales with the message. Debug-style logging left in a hot path is a
  real, measurable cost, not just noise in the transaction log.
- **Repeated deserialization consumes compute units.** Unpacking the same
  account's bytes into a struct more than once per instruction (e.g.
  `EscrowState::try_from_slice` called separately in a validation helper and
  again in the handler) pays the deserialization cost twice for the same
  data. Deserialize once, then pass the typed value around.
- **Repeated PDA derivation consumes compute units.** `find_program_address`
  is not free — it iterates bump seeds internally and hashes each candidate.
  Calling it more than once per instruction for the same seeds (once to
  validate the caller-supplied vault PDA, then again later to sign a CPI
  with it) pays that iteration cost twice. Deriving once and reusing the
  `(pubkey, bump)` is cheaper than re-deriving; better still, accept the
  bump as an instruction argument and confirm it with `create_program_address`
  (no iteration) rather than re-searching for it.
- **Allocations and unnecessary copies may consume extra compute.** Cloning
  account data into a new `Vec`, collecting into intermediate containers, or
  copying an `AccountInfo`'s data where a borrow would do all cost more than
  the zero-copy equivalent. Native programs run inside a constrained BPF VM
  where allocation isn't the free operation it feels like off-chain.

None of these are reasons to avoid CPIs, logging, or deserialization
outright — the escrow program needs all of them to function. They're reasons
to notice *when* something happens more than once per instruction for no
reason, once there's a number to look at.

## Plan to measure later

Nothing below has been measured yet — there's no implementation to measure.
This is the checklist for once the program exists and instructions can be
exercised against a local validator or via `solana-test-validator` logs /
`sol_log_compute_units()`.

| Instruction | What to measure | Why it's the one to watch |
|---|---|---|
| `Initialize` | CU for creating the state account + creating/funding the vault | Two CPIs to the System/Token programs back to back; likely the most CPI-heavy of the "setup" instructions |
| `Deposit` | CU for the token transfer into the vault | Should be close to a bare SPL `Transfer` CPI plus one deserialize; a good baseline for "CPI + validation only" cost |
| `Execute` (Exchange) | CU for validating both sides, transferring out, and closing the vault + state accounts | Likely the most expensive instruction — multiple CPIs (transfer, close vault, close state) plus the most account validation of any instruction |
| `Cancel` | CU for returning the vault's tokens to the maker and closing both accounts | Structurally similar to `Execute` minus one party; useful comparison point for "how much does dropping one leg of validation save" |
| Rejected instruction | CU consumed by an instruction that fails validation (wrong signer, wrong owner, bad PDA) *before* returning an error | Confirms how expensive a rejected attempt is — validation checks run and cost CU even though the transaction ultimately reverts and no state changes stick |

The "rejected instruction" row matters for the same reason
[Transaction atomicity](transaction-model.md#transaction-atomicity) matters:
a failed transaction still pays fees and still burns whatever compute ran
before the failure. Cheap, early validation checks (signer/owner comparisons
before any deserialization or CPI) fail fast and burn less CU than a
rejection that happens after most of the instruction's work is already done
— worth confirming with real numbers once there's something to run.

Hour 4's compute half is complete once each of the six bullets under
[What consumes compute](#what-consumes-compute) can be explained from
memory, and the five rows above are understood as *planned* measurements —
not yet taken.
