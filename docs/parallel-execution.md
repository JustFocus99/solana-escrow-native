# Parallel Execution and Account Locking

[transaction-model.md](transaction-model.md#account-metadata) already
established the raw mechanic: `is_writable` isn't just an access-control
flag, it's the scheduling input the runtime uses to decide which
transactions can run at the same time. This doc works through what that
means concretely for the escrow program's instruction set, once it exists.

## How account locking works

Before running a transaction, the runtime locks every account it references
— read locks for read-only accounts, write locks for writable ones (signer
status doesn't affect locking; only writability does). Any number of
transactions can hold a read lock on the same account simultaneously and run
in parallel. A write lock is exclusive: while one transaction holds it, no
other transaction — read or write — can touch that account until the first
one finishes.

So parallelism between two transactions comes down to one question: **do
their writable-account sets intersect?**

- No overlap in writable accounts → the runtime can schedule them
  concurrently, regardless of what read-only accounts they share.
- Any overlap in writable accounts → the runtime serializes them; one waits
  for the other to finish, even if the actual state they'd touch wouldn't
  have conflicted logically. The lock is per-account, not per-field — the
  runtime has no way to know two writes to the same account "wouldn't have
  collided."

This is also why over-marking an account as writable (see
[transaction-model.md](transaction-model.md#account-metadata)) is a real
cost and not just imprecision: every unnecessary write lock is a
serialization point against every other transaction that also happens to
touch that account, even read-only.

## Per-instruction account lists

These are the accounts each escrow instruction is expected to need. Nothing
here is implemented yet — this is the planning pass account-model.md and
transaction-model.md set up for, applied instruction by instruction.

### `Initialize`

| Account | Signer | Writable | Notes |
|---|---|---|---|
| Maker wallet | yes | yes | pays for account creation, funds rent |
| Escrow state (PDA) | no | yes | created and written this instruction |
| Vault token account | no | yes | created and assigned to the escrow PDA |
| Token mint | no | no | read to confirm decimals / mint identity |
| SPL Token Program | no | no | executable, invoked via CPI |
| System Program | no | no | executable, invoked via CPI |

**Conflicts with**: any other transaction writing to this same escrow state
PDA or this same vault — which, since both are freshly derived/created here,
means only another `Initialize` racing for the *same* seeds (same maker +
same deal parameters), or a later instruction on this exact escrow.
**Does not conflict with**: `Initialize`, `Deposit`, `Execute`, or `Cancel`
calls belonging to a *different* escrow (different maker or different PDA
seeds) — those write entirely different accounts.

### `Deposit`

| Account | Signer | Writable | Notes |
|---|---|---|---|
| Maker wallet | yes | yes | source of the deposited tokens (or its token account) |
| Maker's token account | no | yes | debited |
| Vault token account | no | yes | credited |
| Escrow state (PDA) | no | maybe | writable only if deposit status is tracked in state; read-only if the vault balance alone is the source of truth |
| SPL Token Program | no | no | executable, invoked via CPI |

**Conflicts with**: `Initialize`/`Execute`/`Cancel` on the *same* escrow
(shared vault and/or state account), and any other `Deposit` attempt against
the same vault. **Does not conflict with**: a `Deposit` into a different
escrow's vault, even by the same maker wallet, *if* the maker's token
account isn't also shared — in practice the maker's own token account being
writable here means two deposits by the same maker into two different
escrows still serialize on that shared account, even though the vaults
differ.

### `Execute` (Exchange)

| Account | Signer | Writable | Notes |
|---|---|---|---|
| Taker wallet | yes | yes | authorizes taking the deal, receives the vault's tokens |
| Maker wallet | no | yes | receives the taker's payment (lamports or tokens) |
| Escrow state (PDA) | no | yes | closed at the end of this instruction |
| Vault token account | no | yes | emptied and closed |
| Taker's token account | no | yes | credited from the vault |
| Maker's receiving account | no | yes | credited from the taker |
| SPL Token Program | no | no | executable, invoked via CPI (transfer + close) |

**Conflicts with**: `Deposit`, `Cancel`, or another `Execute` attempt on the
same escrow (shared state + vault) — this is the instruction with the
largest writable set, so it's also the one most likely to collide with
something. **Does not conflict with**: activity on any other escrow, and
notably does *not* conflict with a third party's unrelated `Initialize`
happening concurrently, even from the same taker wallet, as long as that
`Initialize` doesn't reuse an account this `Execute` is writing.

### `Cancel`

| Account | Signer | Writable | Notes |
|---|---|---|---|
| Maker wallet | yes | yes | receives back the vault's tokens and the state account's rent |
| Escrow state (PDA) | no | yes | closed |
| Vault token account | no | yes | emptied and closed |
| Maker's token account | no | yes | credited with the returned tokens |
| SPL Token Program | no | no | executable, invoked via CPI |

**Conflicts with**: `Deposit` or `Execute` racing against the same escrow
(exactly the race the program's business logic — not the runtime — has to
resolve: whichever transaction lands first should make the other fail
validation, e.g. "vault already closed" or "escrow already executed", not
silently double-spend).

## The central idea

**Two transactions that write different escrow accounts and different
token accounts may run independently.**

**Two transactions that both write the same escrow state or the same vault
conflict with each other.**

Concretely, from the tables above: every instruction on a given escrow
shares that escrow's state PDA and vault as writable accounts. That means
`Initialize`, `Deposit`, `Execute`, and `Cancel` calls **on the same escrow**
always serialize against each other, no matter which specific pair it is —
the runtime doesn't know or care that "a `Cancel` and an `Execute` racing
for the same escrow" is business-logically interesting; it just sees two
transactions wanting a write lock on the same account and orders them.
Meanwhile, the same four instructions running against **two unrelated
escrows** (different maker, different PDA seeds, different vault) share no
writable accounts at all and can run fully in parallel — the escrow
program's design, being one state account + one vault per deal rather than
one global account, is exactly what makes many simultaneous trades possible
instead of funneling every user through a single point of serialization.

Hour 4's locking half is complete once, for any two instructions picked from
the tables above, the writable accounts that would force them to run
serially (rather than in parallel) can be named without re-reading the
tables.
