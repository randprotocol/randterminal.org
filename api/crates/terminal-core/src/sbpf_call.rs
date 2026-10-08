//! A Solana instruction as a developer writes it (program id, accounts, data) → the serialized
//! input the program's entrypoint reads, exactly as the runtime lays it out.
use crate::views::{AccountForm, SbpfCallForm};
use rand_zkvm::sbpf::{deserialize_accounts, deserialize_instruction, serialize_aligned, spl_transfer, Account, SbpfCall, MAX_ACCOUNTS};

pub fn parse_key(s: &str) -> Result<[u8; 32], String> {
    let s = s.trim();
    let bytes = if let Some(h) = s.strip_prefix("0x") { hex::decode(h).map_err(|e| format!("{s}: {e}"))? }
        else { bs58::decode(s).into_vec().map_err(|e| format!("{s}: {e}"))? };
    bytes.try_into().map_err(|b: Vec<u8>| format!("{s}: {} bytes, a key is 32", b.len()))
}

fn parse_hex(s: &str) -> Result<Vec<u8>, String> { hex::decode(s.trim().trim_start_matches("0x")).map_err(|e| format!("{s}: {e}")) }

pub fn build(elf: &[u8], form: &SbpfCallForm) -> Result<SbpfCall, String> {
    if form.accounts.len() > MAX_ACCOUNTS { return Err(format!("{} accounts; at most {MAX_ACCOUNTS}", form.accounts.len())); }
    let program_id = parse_key(&form.program_id)?;
    let accounts = form.accounts.iter().map(|a| Ok(Account {
        key: parse_key(&a.key)?, owner: parse_key(&a.owner)?, lamports: a.lamports, data: parse_hex(&a.data)?,
        is_signer: a.is_signer, is_writable: a.is_writable, executable: a.executable, rent_epoch: u64::MAX,
    })).collect::<Result<Vec<_>, String>>()?;
    let data = parse_hex(&form.data)?;
    let input = serialize_aligned(&accounts, &data, &program_id);
    if input.len() > sbpf_core::abi::MAX_INPUT_BYTES { return Err(format!("serialized input is {} bytes; the guest takes at most {}", input.len(), sbpf_core::abi::MAX_INPUT_BYTES)); }
    Ok(SbpfCall { elf: elf.to_vec(), input })
}

fn key_text(k: &[u8; 32]) -> String { bs58::encode(k).into_string() }

pub fn form_from_input(input: &[u8]) -> SbpfCallForm {
    let accounts = deserialize_accounts(input).into_iter().map(|a| AccountForm {
        key: key_text(&a.key), owner: key_text(&a.owner), lamports: a.lamports, data: format!("0x{}", hex::encode(&a.data)),
        is_signer: a.is_signer, is_writable: a.is_writable, executable: a.executable,
    }).collect();
    let (data, program_id) = deserialize_instruction(input);
    SbpfCallForm { program_id: key_text(&program_id), accounts, data: format!("0x{}", hex::encode(data)) }
}

pub fn spl_transfer_form(amount: u64) -> SbpfCallForm { form_from_input(&spl_transfer(amount).input) }
