use solana_program::pubkey::Pubkey;

/// Seed prefix for the escrow state PDA. See docs/pda-design.md.
pub const ESCROW_SEED_PREFIX: &[u8] = b"escrow";

/// Derives the escrow state PDA and its canonical bump for `(maker, escrow_id)`
/// under `program_id`. The returned bump is the one that must be stored in
/// `EscrowState` and reused for every later `create_program_address` /
/// `invoke_signed` call — see docs/pda-design.md.
pub fn derive_escrow_pda(maker: &Pubkey, escrow_id: u64, program_id: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[ESCROW_SEED_PREFIX, maker.as_ref(), &escrow_id.to_le_bytes()],
        program_id,
    )
}
