use escrow::validation::{derive_escrow_pda, ESCROW_SEED_PREFIX};
use solana_program::pubkey::Pubkey;

#[test]
fn same_inputs_produce_the_same_pda() {
    let maker = Pubkey::new_unique();
    let program_id = Pubkey::new_unique();
    let escrow_id = 42u64;

    let (pda_a, bump_a) = derive_escrow_pda(&maker, escrow_id, &program_id);
    let (pda_b, bump_b) = derive_escrow_pda(&maker, escrow_id, &program_id);

    assert_eq!(pda_a, pda_b);
    assert_eq!(bump_a, bump_b);
}

#[test]
fn different_makers_produce_different_pdas() {
    let maker_a = Pubkey::new_unique();
    let maker_b = Pubkey::new_unique();
    let program_id = Pubkey::new_unique();
    let escrow_id = 42u64;

    let (pda_a, _) = derive_escrow_pda(&maker_a, escrow_id, &program_id);
    let (pda_b, _) = derive_escrow_pda(&maker_b, escrow_id, &program_id);

    assert_ne!(pda_a, pda_b);
}

#[test]
fn different_escrow_ids_produce_different_pdas() {
    let maker = Pubkey::new_unique();
    let program_id = Pubkey::new_unique();

    let (pda_a, _) = derive_escrow_pda(&maker, 1, &program_id);
    let (pda_b, _) = derive_escrow_pda(&maker, 2, &program_id);

    assert_ne!(pda_a, pda_b);
}

#[test]
fn different_program_ids_produce_different_pdas() {
    let maker = Pubkey::new_unique();
    let program_id_a = Pubkey::new_unique();
    let program_id_b = Pubkey::new_unique();
    let escrow_id = 42u64;

    let (pda_a, _) = derive_escrow_pda(&maker, escrow_id, &program_id_a);
    let (pda_b, _) = derive_escrow_pda(&maker, escrow_id, &program_id_b);

    assert_ne!(pda_a, pda_b);
}

#[test]
fn returned_bump_reproduces_the_pda() {
    let maker = Pubkey::new_unique();
    let program_id = Pubkey::new_unique();
    let escrow_id = 42u64;

    let (pda, bump) = derive_escrow_pda(&maker, escrow_id, &program_id);

    let recreated = Pubkey::create_program_address(
        &[
            ESCROW_SEED_PREFIX,
            maker.as_ref(),
            &escrow_id.to_le_bytes(),
            &[bump],
        ],
        &program_id,
    )
    .expect("canonical bump must produce a valid off-curve address");

    assert_eq!(pda, recreated);
}

#[test]
fn canonical_bump_is_stored_and_reused() {
    let maker = Pubkey::new_unique();
    let program_id = Pubkey::new_unique();
    let escrow_id = 42u64;

    // Derive once, as `Initialize` would, and persist the bump (this is
    // what `EscrowState.bump` stands in for).
    let (pda, stored_bump) = derive_escrow_pda(&maker, escrow_id, &program_id);

    // Every later instruction reuses the stored bump instead of calling
    // `find_program_address` again (see docs/compute-notes.md on the cost
    // of repeated PDA derivation).
    let reused = Pubkey::create_program_address(
        &[
            ESCROW_SEED_PREFIX,
            maker.as_ref(),
            &escrow_id.to_le_bytes(),
            &[stored_bump],
        ],
        &program_id,
    )
    .expect("stored bump must still produce a valid off-curve address");
    assert_eq!(pda, reused);

    // The canonical bump is not interchangeable with neighboring bump
    // values: only the exact stored bump reproduces the PDA.
    if let Some(other_bump) = stored_bump.checked_sub(1) {
        let other = Pubkey::create_program_address(
            &[
                ESCROW_SEED_PREFIX,
                maker.as_ref(),
                &escrow_id.to_le_bytes(),
                &[other_bump],
            ],
            &program_id,
        );
        if let Ok(other_pda) = other {
            assert_ne!(pda, other_pda);
        }
    }
}
