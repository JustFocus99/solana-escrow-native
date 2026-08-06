use solana_program::{
    account_info::AccountInfo, entrypoint::ProgramResult, program_error::ProgramError,
    pubkey::Pubkey,
};

/// Routes a decoded instruction to its handler. For now, every instruction
/// is rejected — no instruction decoding or account validation exists yet.
/// See docs/architecture.md for the intended shape of this router.
pub fn process_instruction(
    _program_id: &Pubkey,
    _accounts: &[AccountInfo],
    _instruction_data: &[u8],
) -> ProgramResult {
    Err(ProgramError::InvalidInstructionData)
}
