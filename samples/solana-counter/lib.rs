use solana_program::{account_info::{next_account_info, AccountInfo}, entrypoint, entrypoint::ProgramResult, msg, program_error::ProgramError, pubkey::Pubkey};

entrypoint!(process_instruction);

/// Adds the instruction's u64 to the u64 stored in the first account.
pub fn process_instruction(_program_id: &Pubkey, accounts: &[AccountInfo], data: &[u8]) -> ProgramResult {
    let acc = next_account_info(&mut accounts.iter())?;
    let by = u64::from_le_bytes(data.get(..8).ok_or(ProgramError::InvalidInstructionData)?.try_into().unwrap());
    let mut d = acc.try_borrow_mut_data()?;
    let now = u64::from_le_bytes(d.get(..8).ok_or(ProgramError::AccountDataTooSmall)?.try_into().unwrap());
    d[..8].copy_from_slice(&now.wrapping_add(by).to_le_bytes());
    msg!("counter");
    Ok(())
}
