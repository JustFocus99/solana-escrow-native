# PDA Design

[account-model.md](account-model.md#key-pubkey) already established the
rule this doc makes concrete: a PDA's key is "deterministically derived from
seeds + a program ID," and the program must re-derive it on every
instruction rather than trust whichever account the caller happened to pass
in slot N. This doc records exactly what those seeds are for the escrow
state account, why that seed set was chosen, and how the resulting bump
flows into `invoke_signed` later.

## The seeds

The escrow state PDA is derived from three seed components, in this order:

1. **`b"escrow"`** — a fixed domain-separation prefix. This guarantees the
   escrow program's state PDAs can never collide with some *other* PDA the
   same program might derive for an unrelated purpose using the same
   maker/ID inputs (there are none today, but the prefix costs nothing and
   forecloses the entire class of future collision).
2. **maker public key** — scopes every escrow PDA to the wallet that created
   it. Without this, two different makers could never safely reuse the same
   `escrow_id`, since the ID alone wouldn't be enough to keep their PDAs
   apart.
3. **escrow ID** (`u64`, little-endian bytes) — lets a single maker run more
   than one escrow concurrently. Without this, a maker could have at most
   one live escrow at a time, since the `(prefix, maker)` pair alone would
   always derive the same address.

Together, `(maker, escrow_id)` is the compound key that must be unique per
escrow — the seed set exists to turn that logical key into a physical
address the runtime can locate and lock.

```rust
let (escrow_pda, bump) = Pubkey::find_program_address(
    &[
        b"escrow",
        maker.as_ref(),
        &escrow_id.to_le_bytes(),
    ],
    program_id,
);
```

`escrow_id` is serialized as fixed-width little-endian bytes (not a decimal
string or a variable-length encoding) so that the seed's byte layout is
unambiguous and deterministic — the same `u64` value always produces the
same 8 seed bytes, independent of platform, and there's no risk of two
different IDs (e.g. via leading-zero variance) ever serializing to the same
bytes.

## Determinism and the canonical bump

`find_program_address` iterates candidate bump seeds from 255 downward and
returns the *first* one whose resulting address falls off the ed25519 curve
— that's what makes an address usable as a PDA (no private key can exist for
it) rather than a plain keypair account. The **first** bump that works is
the *canonical* bump for that seed set; every other valid off-curve bump for
the same seeds is technically constructible but is not the one any part of
the program should ever produce or accept.

This has a direct consequence for the escrow program's design: the bump
found once during `Initialize` must be **stored in `EscrowState`**, not
re-derived by searching on every subsequent instruction. Two reasons, both
already covered elsewhere in these docs:

- **Cost** — `find_program_address`'s search is not free; re-running it in
  every instruction pays that iteration cost repeatedly for a value that
  never changes. See
  [compute-notes.md](compute-notes.md#what-consumes-compute).
- **Correctness** — the only way to *prove* a given account is the
  program's escrow PDA (as opposed to some other off-curve address the
  caller supplied) is to recompute the address from the stored seeds
  *including the stored bump*, via `create_program_address`, and compare it
  against the account key the caller passed in. Storing the bump makes that
  comparison a single non-searching hash instead of a fresh search.

`derive_escrow_pda` in
[programs/escrow/src/validation.rs](../programs/escrow/src/validation.rs)
is the single place that performs the search; every other call site is
expected to already have the bump in hand (read from `EscrowState`, or
freshly returned by this function during `Initialize`) and pass it straight
to `create_program_address`.

## Signer seeds for `invoke_signed`

The escrow PDA has no private key, so it cannot sign a transaction the way a
wallet does. Instead, the escrow program authorizes CPIs *on the PDA's
behalf* by calling `invoke_signed` with the exact seeds (including the
bump) that derive it. The runtime accepts this as proof of authority only
because it can independently recompute the same address from those seeds
and confirm it matches the account being "signed" for — and only the
program whose ID was used in the derivation is allowed to do this.

The signer seeds slice mirrors the derivation seeds with one addition — the
bump byte appended at the end:

```rust
&[
    b"escrow",
    maker.as_ref(),
    &escrow_id.to_le_bytes(),
    &[bump],
]
```

This is the array `invoke_signed` expects in its `signers_seeds` argument
when the escrow program needs the vault's transfer/close authority to move —
e.g. paying the taker during `Exchange` or refunding the maker during
`Cancel`. The `bump` here must be the *stored* canonical bump from
`EscrowState`, not a freshly searched one: `invoke_signed` calls
`create_program_address` internally (no searching), so an incorrect bump
simply fails to reproduce the expected PDA and the CPI is rejected.

## Summary

- Seeds: `b"escrow"`, maker pubkey, `escrow_id.to_le_bytes()`.
- `find_program_address` runs once, during `Initialize`; its bump is stored
  in `EscrowState` from then on.
- Every later instruction re-derives the PDA via `create_program_address`
  with the stored bump to validate the caller-supplied account, and reuses
  the same stored bump in the signer seeds passed to `invoke_signed`.
- `derive_escrow_pda(maker, escrow_id, program_id) -> (Pubkey, u8)` in
  [validation.rs](../programs/escrow/src/validation.rs) is covered by tests
  for determinism, sensitivity to each seed component, bump reproduction via
  `create_program_address`, and canonical-bump reuse.
