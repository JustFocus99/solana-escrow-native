#[cfg(not(feature = "no-entrypoint"))]
use crate::processor::process_instruction;
#[cfg(not(feature = "no-entrypoint"))]
use solana_program::entrypoint;

#[cfg(not(feature = "no-entrypoint"))]
entrypoint!(process_instruction);
