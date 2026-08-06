# solana-escrow-native

A native (no-framework) Solana escrow program, built from the runtime up:
account model, transaction model, PDAs, and CPIs are each worked through and
documented before any business logic is written.

## What this is

Two parties want to trade tokens without trusting each other or a
middleman. The maker deposits tokens into a vault the program controls;
either a taker exchanges their own tokens for it, or the maker cancels and
gets their deposit back. The program itself never holds a private key —
every account it "controls" is a Program Derived Address, and every custody
guarantee comes from Solana's account-ownership and signature rules rather
than from trusting a person or a server.

"Native" means this is written directly against `solana-program`, with no
Anchor or other framework — instruction decoding, account validation, and
CPI construction are all explicit, by design, so nothing about how the
runtime enforces safety is hidden behind a macro.

## Repository layout

```
programs/escrow/          the on-chain program (native Rust, no framework)
  src/
    validation.rs          PDA derivation (see docs/pda-design.md)
  tests/
    pda_derivation.rs       integration tests for PDA derivation

docs/                      design and study notes, read in this order
  account-model.md          what each AccountInfo field means and how far it can be trusted
  transaction-model.md      how an instruction becomes a committed (or rolled-back) state change
  compute-notes.md          what spends compute units, and what to measure once the program exists
  parallel-execution.md     how account locking determines which transactions can run concurrently
  pda-design.md             the escrow PDA's seeds, canonical bump, and invoke_signed usage
  cpi-model.md              the CPI flow into the SPL Token Program and the checks required before it
  architecture.md           how the program's components fit together
  threat-model.md           what an attacker controls when calling this program, and what they don't

SECURITY.md                 how to report a vulnerability
```

## Status

This program is under active development and has not been audited. The
current codebase implements PDA derivation only (`derive_escrow_pda`); the
processor, instruction decoding, and CPI logic have not been written yet.
See [docs/architecture.md](docs/architecture.md) for the intended shape of
the finished program and [docs/threat-model.md](docs/threat-model.md) for
the assumptions its validation logic is being built to hold.

## Building and testing

```bash
cargo build --manifest-path programs/escrow/Cargo.toml
cargo test --manifest-path programs/escrow/Cargo.toml
```

Building the actual on-chain (SBF) artifact requires the Solana CLI's
`cargo-build-sbf`:

```bash
cargo build-sbf --manifest-path programs/escrow/Cargo.toml
```

## Security

Do not open a public issue for a suspected vulnerability. See
[SECURITY.md](SECURITY.md) for how to report one.
