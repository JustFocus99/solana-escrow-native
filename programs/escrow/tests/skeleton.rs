use escrow::processor::process_instruction;
use solana_program::instruction::{Instruction, InstructionError};
use solana_program::pubkey::Pubkey;
use solana_program_test::{processor, ProgramTest};
use solana_sdk::signature::Signer;
use solana_sdk::transaction::{Transaction, TransactionError};

#[tokio::test]
async fn program_loads_and_rejects_empty_instruction() {
    let program_id = Pubkey::new_unique();
    let program_test = ProgramTest::new("escrow", program_id, processor!(process_instruction));

    // Starting the test environment loads the program (via the native
    // processor injected above) into a fresh in-process runtime.
    let (banks_client, payer, recent_blockhash) = program_test.start().await;

    // An instruction with no accounts and no data still reaches the
    // program's process_instruction — the runtime doesn't require either to
    // be non-empty before invoking it.
    let instruction = Instruction {
        program_id,
        accounts: vec![],
        data: vec![],
    };
    let transaction = Transaction::new_signed_with_payer(
        &[instruction],
        Some(&payer.pubkey()),
        &[&payer],
        recent_blockhash,
    );

    let result = banks_client.process_transaction(transaction).await;
    let banks_error = result.expect_err("empty instruction should be rejected");

    assert_eq!(
        banks_error.unwrap(),
        TransactionError::InstructionError(0, InstructionError::InvalidInstructionData),
    );
}
