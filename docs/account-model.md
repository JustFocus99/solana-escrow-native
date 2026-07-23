# Account Model

Every account a native Solana program touches arrives as an `AccountInfo`. None
of its fields are inherently trustworthy just because they're present in the
struct — the runtime guarantees only that the *bytes* are accurate as of the
start of the transaction. Whether those bytes mean what the program *hopes*
they mean is something the program has to check itself. This doc goes
field-by-field, then applies that reasoning to the four account types the
escrow program actually handles.

## The fields

### `key: &Pubkey`

- **What it means**: the account's on-chain address — either an ed25519
  keypair address or a Program Derived Address (PDA).
- **Who controls it**: nobody "controls" it at runtime; it's immutable and
  fixed for the life of the account. A keypair account's key comes from
  whoever generated the keypair; a PDA's key is deterministically derived
  from seeds + a program ID via `find_program_address`.
- **May the program trust it?**: only as an identity to check *against*, never
  as proof of role. The caller chooses which accounts go in which instruction
  slots, so the program cannot assume "the account in slot 2 is the vault"
  just because that's the convention — it must confirm the key it received is
  the one it expects.
- **What must be validated**: for PDAs, re-derive the address from seeds and
  `program_id` and compare with `==`. For expected well-known accounts (the
  SPL Token program, the System program), compare against the known program
  ID constant.
- **Escrow usage**: the escrow PDA's key is re-derived from
  `[b"escrow", maker.key.as_ref(), ...]` (or similar seeds) on every
  instruction to confirm the caller passed the real vault-authority PDA, not
  an attacker-supplied lookalike account.

### `lamports: Rc<RefCell<&mut u64>>`

- **What it means**: the account's SOL balance, in lamports.
- **Who controls it**: the runtime enforces that only the *owning* program can
  **decrease** an account's lamports; **any** program can **increase**
  lamports in any account (e.g. simple transfers into it). So a non-owning
  program can fund an account but never drain it.
- **May the program trust it?**: the number itself is accurate, but a
  sufficient balance does not imply the account is initialized, rent-exempt,
  or owned by the right program.
- **What must be validated**: when creating an account, verify
  `lamports >= Rent::get()?.minimum_balance(len)` so it's rent-exempt. When
  closing an account, the program must be the owner (otherwise it can't zero
  the balance) and should also zero the data.
- **Escrow usage**: on `Initialize`, the maker funds the escrow state account
  up to the rent-exempt minimum; on `Cancel`/`Exchange` completion, the escrow
  program (as owner) drains the state account's lamports back to the maker
  and zeroes its data as part of closing it.

### `data: Rc<RefCell<&mut [u8]>>`

- **What it means**: the raw byte buffer stored at that address — this is
  where all account-specific state lives (serialized structs, token balances,
  etc. depending on account type).
- **Who controls it**: only the owning program may write to it. Reallocating
  its length is likewise owner-only.
- **May the program trust it?**: never blindly. Bytes that deserialize
  cleanly into the expected struct are not proof the account is legitimate —
  an attacker can hand you *any* account whose data happens to match your
  struct's byte layout.
- **What must be validated**: check `owner == program_id` (or the expected
  program's ID) *before* deserializing. Only after that check does a
  successful deserialization mean anything.
- **Escrow usage**: the escrow state account's data is trusted to be a valid
  `EscrowState` only after confirming `escrow_account.owner == program_id`.
  The vault token account's data is trusted to be a valid SPL token account
  only after confirming `vault_account.owner == spl_token::ID`, and is then
  unpacked with `spl_token::state::Account::unpack`.

### `owner: &Pubkey`

- **What it means**: the program ID of the program allowed to modify this
  account's data and debit its lamports.
- **Who controls it**: set at account creation (`system_instruction::create_account`
  can assign an owner directly), and thereafter changeable only by the
  *current* owner (e.g. via `assign`). No other program can seize ownership.
- **May the program trust it?**: yes — the runtime itself enforces the
  write-permission rules based on this field, so if the runtime let a write
  through, the owner check upstream in the program logic is the thing making
  that write meaningful. In other words: this is *the* trust boundary field.
  Every account handling in the escrow program should start with an owner
  check.
- **What must be validated**: explicitly compare `account.owner` to the
  expected program ID for every account before trusting its data —
  `program_id` for the escrow state account, `spl_token::ID` for token
  accounts and mints, `system_program::ID` for the maker's wallet.
- **Escrow usage**: this is the field that answers "why does the escrow
  program own the state account but not the vault?" — see the worked
  explanation at the end of this doc.

### `executable: bool`

- **What it means**: whether the account holds loaded program bytecode (and
  can therefore be the target of a CPI / instruction call) rather than
  ordinary state data.
- **Who controls it**: set once by the BPF Loader when a program is deployed
  and finalized; never changes afterward for that account.
- **May the program trust it?**: yes, it's runtime-enforced — you cannot
  execute an account where this is `false`. But it doesn't tell you *which*
  program it is.
- **What must be validated**: when an "account" argument is supposed to be a
  program (e.g. "the SPL Token program"), check both `executable == true` and
  that the key matches the known program ID — checking one without the other
  leaves a gap.
- **Escrow usage**: the escrow program checks that the token program account
  passed into `Initialize`/`Exchange`/`Cancel` has `key == spl_token::ID`
  before issuing any CPI to it. `executable` itself is rarely inspected
  directly in native programs since `invoke`/`invoke_signed` will fail loudly
  against a non-executable target anyway.

### `rent_epoch: Epoch`

- **What it means**: legacy field recording the epoch through which rent was
  last considered "paid" for this account, from the pre-rent-exemption model
  where accounts could pay rent periodically instead of holding a minimum
  balance.
- **Who controls it**: the runtime's rent-collection mechanism.
- **May the program trust it?**: largely irrelevant today — nearly all
  accounts on mainnet are rent-exempt, and this field carries no information
  a native program needs to act on.
- **What must be validated**: nothing, in practice. Do not build logic around
  it.
- **Escrow usage**: not read or relied upon anywhere in the escrow program.
  Rent-exemption is instead handled explicitly at account-creation time (see
  below).

### `is_signer: bool`

- **What it means**: whether this account's private key was used to sign the
  current transaction (or, inside a CPI, whether it was signed via
  `invoke_signed` with matching PDA seeds).
- **Who controls it**: whoever assembles and signs the transaction (the
  client/wallet), or a calling program invoking a CPI with the right seeds on
  behalf of a PDA.
- **May the program trust it?**: yes, the runtime guarantees this is accurate
  — but the program must actually *check* it for every account whose
  authorization matters. The field being available doesn't enforce anything
  by itself; an unchecked `is_signer` is a classic missing-signer-check bug.
- **What must be validated**: explicitly assert `is_signer == true` for any
  account that must have authorized the instruction.
- **Escrow usage**: the maker's wallet account must have `is_signer == true`
  on `Initialize`, `Deposit`, and `Cancel` — these are actions only the maker
  should be able to trigger. The escrow PDA itself is never a transaction
  signer from a client; it "signs" CPIs internally via `invoke_signed` with
  its known seeds, authorizing token transfers out of the vault.

### `is_writable: bool`

- **What it means**: whether this account was included in the transaction's
  account list as writable, which the runtime uses to allow or forbid data/
  lamport mutation and to enable parallel scheduling of transactions that
  don't share writable accounts.
- **Who controls it**: the client that builds the transaction, via
  `AccountMeta::new` (writable) vs. `AccountMeta::new_readonly`. The runtime
  enforces the declared setting.
- **May the program trust it?**: yes — the runtime will reject any attempt to
  write to an account not marked writable, so this is self-enforcing. Still
  worth checking explicitly so the program fails with a clear custom error
  instead of an opaque runtime error.
- **What must be validated**: assert `is_writable == true` for every account
  the instruction intends to mutate.
- **Escrow usage**: the escrow state account, the vault token account, and
  the maker's wallet must all be writable on the instructions that mutate
  them (state transitions, token transfers, lamport refunds respectively).
  The token mint and the SPL Token program account are read-only /
  executable references and are never required to be writable.

## Worked examples

### Escrow state account

- **Owning program**: the escrow program itself (`program_id`).
- **Data**: a serialized `EscrowState` (maker pubkey, vault account pubkey,
  expected amount, escrow bump seed, a status/initialized flag, etc).
- **Writable**: during `Initialize` (to write the initial state) and during
  `Exchange`/`Cancel` (to close the account and zero it out). Read-only
  access would suffice if the program only ever inspected it, but every real
  instruction transitions or closes it.
- **Trust boundary**: before deserializing, confirm
  `escrow_account.owner == program_id`. Since this program *is* the owner,
  it's also the only program allowed to write the account's data — that's
  precisely why this account's data can be trusted as a faithful
  `EscrowState` once the owner check passes.

### Vault token account

- **Owning program**: the SPL Token Program, not the escrow program.
- **Token authority**: the escrow PDA (set via `SetAuthority` or by creating
  the account with the PDA as owner/authority at initialization) — this is a
  concept internal to the *token account's data*, distinct from the
  `AccountInfo.owner` field, which is always the SPL Token Program for any
  token account.
- **Stores**: the deposited input tokens, held in escrow until `Exchange` or
  `Cancel`.
- **Trust boundary**: confirm `vault_account.owner == spl_token::ID`, then
  `spl_token::state::Account::unpack` the data, then check the *unpacked*
  `authority` field equals the escrow PDA before trusting that the escrow
  program can move funds out of it via CPI.

### Maker wallet

- **Owning program**: the System Program (an ordinary, unallocated keypair
  account).
- **Must sign**: `Initialize`, `Deposit`, and `Cancel` — every instruction
  where the maker is authorizing their own funds to move or the escrow to be
  torn down. Not required to sign `Exchange`, since that's the taker's
  action.
- **Trust boundary**: check `is_signer == true`; also check `owner ==
  system_program::ID` if the program cares that it's a plain wallet rather
  than some other kind of account (mostly relevant if lamports are being
  transferred directly to/from it).

### Token mint

- **Owning program**: the SPL Token Program.
- **Defines**: the token type (its own pubkey is the type identity) and
  `decimals`, used to interpret raw token-account amounts as human-scale
  quantities.
- **Trust boundary**: confirm `mint_account.owner == spl_token::ID` and
  unpack it as `spl_token::state::Mint` before trusting `decimals`. If the
  escrow enforces "vault and taker's receiving account must be the same
  mint," compare unpacked-token-account `mint` fields against this account's
  key, not just against each other.

## Rent-exempt account creation

Since the account creation exemption model replaced the old pay-rent-per-epoch
model, an account must maintain a minimum lamport balance to be exempt from
(no longer applicable) rent collection and eligible to stay on-chain
indefinitely. The runtime computes that minimum from the account's data
length:

```rust
let rent = Rent::get()?;
let required_lamports = rent.minimum_balance(ESCROW_STATE_LEN);
```

Creating the escrow state account is then a CPI to the System Program,
funded by the maker and assigned to the escrow program:

```rust
invoke(
    &system_instruction::create_account(
        maker.key,
        escrow_state.key,
        required_lamports,
        ESCROW_STATE_LEN as u64,
        program_id,          // owner = escrow program
    ),
    &[maker.clone(), escrow_state.clone(), system_program.clone()],
)?;
```

If `escrow_state` is a PDA, this becomes `invoke_signed` with the PDA's seeds
and bump instead of `invoke`, since the PDA has no private key to sign with —
the program authorizes the creation on the PDA's behalf.

## Why the escrow program owns the state account but not the vault

The `owner` field is a permission, not a label — it says "only this program
may write this account's data or debit its lamports." Ownership should
therefore go wherever the *authoritative interpretation logic* for that data
lives:

- The **state account**'s bytes only mean something through the escrow
  program's own `EscrowState` struct and business rules (what amount was
  deposited, whose deal this is, what state it's in). No other program has
  any reason to write those bytes, so the escrow program owns it — that
  ownership is what lets the program trust its own state without an
  additional authority check beyond the owner comparison itself.

- The **vault**, by contrast, must still obey the SPL Token Program's own
  rules for balances, transfers, and authority checks — it's specifically the
  Token Program's binary layout (`spl_token::state::Account`) and the Token
  Program's transfer instruction that give the vault its meaning as "a token
  account." If the escrow program owned it directly, the Token Program would
  refuse to operate on it (a `Transfer` instruction requires `owner ==
  spl_token::ID`), and the tokens inside would become unmovable through any
  standard SPL tooling.

  Instead, the escrow program controls the vault *indirectly*: it holds
  **authority** over the token account (a field inside the Token Program's
  account data, set to the escrow PDA), not **ownership** of the account
  itself (the `AccountInfo.owner` field, which stays `spl_token::ID`).
  Authority lets the escrow program sign token `Transfer`/`CloseAccount`
  instructions via `invoke_signed` with its PDA seeds, without ever needing
  write access to the account's raw bytes — the Token Program is the one
  actually mutating them, on the escrow program's instruction.

In short: **ownership** determines who may write an account's raw data;
**authority** (where it exists, as an application-level field inside token
account data) determines who may direct actions on that data through the
owning program. The escrow program needs ownership of the state account
because it invented that data format and is the only program that
understands it. It needs only authority — not ownership — over the vault,
because the vault's data format and validity rules belong to the Token
Program, and the escrow program only needs to *direct* transfers, not define
what a token account is.
