//! Turning a form a Solidity developer fills in (function, arguments, caller, initial storage)
//! into the guest's input vector, including the storage witnesses the developer never sees.
//!
//! The guest reads storage only through Merkle witnesses carried in its input, and a slot read
//! without one halts with `Halt::NoWitness`, which does not say which slot. [`build`] therefore
//! discovers the witnesses by dry-running the host interpreter under [`TraceHost`], which records
//! every 64-byte Keccak message (Solidity's mapping slot `keccak256(key ‖ base)`): the most recent
//! such hash not yet witnessed is the next candidate, else the lowest untried constant slot below
//! 16. Once the call stops asking, slots it does not need are pruned.
use crate::views::{DiscoveryStep, EvmCallForm, StorageSeed};
use evm_core::abi::{run_call_with, Workspace};
use evm_core::interp::{Halt, Outcome};
use evm_core::u256::U256;
use evm_core::Host;
use rand_zkvm::evm::{
    mapping_slot, mapping_slot2, selector, slot_index, EvmCall, HostRef, SparseTree, ALICE, BOB, SLOT_BALANCES,
    SLOT_TOTAL_SUPPLY, TOKEN,
};
use std::collections::BTreeSet;

/// `evm_core::storage::MAX_WITNESSES`: the most witnesses one guest input carries.
pub const MAX_WITNESSES: usize = evm_core::storage::MAX_WITNESSES;
/// Constant slots `0..CONSTANT_SLOTS` are tried when no mapping hash explains a missing witness.
const CONSTANT_SLOTS: u32 = 16;

/// `0x` + 64 hex digits, big-endian.
pub fn u256_hex(v: &U256) -> String { format!("0x{}", hex::encode(v.to_be_bytes())) }

/// A decimal or `0x`-hex number of at most 256 bits.
pub fn parse_u256(s: &str) -> Result<U256, String> {
    let s = s.trim();
    if s.is_empty() { return Err("empty number".into()); }
    if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        if h.is_empty() { return Err(format!("{s} has no digits")); }
        let h = h.trim_start_matches('0');
        if h.len() > 64 { return Err(format!("{s} is wider than 256 bits")); }
        let padded = format!("{h:0>64}");
        let bytes: [u8; 32] = hex::decode(&padded).map_err(|e| format!("{s}: {e}"))?.try_into().unwrap();
        return Ok(U256::from_be_bytes(&bytes));
    }
    // Big-endian bytes × 10 + digit, with the carry out of byte 0 meaning overflow.
    let mut be = [0u8; 32];
    for ch in s.chars() {
        let d = ch.to_digit(10).ok_or_else(|| format!("{s} is not a number"))?;
        let mut carry = d;
        for b in be.iter_mut().rev() {
            let x = *b as u32 * 10 + carry;
            *b = x as u8;
            carry = x >> 8;
        }
        if carry != 0 { return Err(format!("{s} does not fit in 256 bits")); }
    }
    Ok(U256::from_be_bytes(&be))
}

/// A `0x`-hex address of at most 20 significant bytes. Leading zeros are padding, so a 32-byte
/// word holding a right-aligned address (what [`u256_hex`] prints) is accepted.
fn parse_address(arg: &str) -> Result<U256, String> {
    let arg = arg.trim();
    let h = arg.strip_prefix("0x").ok_or_else(|| format!("address must be 0x-hex: {arg}"))?;
    if h.trim_start_matches('0').len() > 40 { return Err(format!("address wider than 20 bytes: {arg}")); }
    parse_u256(arg)
}

/// The canonical spelling the selector is hashed over: `uint` and `int` are `uint256`/`int256`.
fn canonical_type(ty: &str) -> String {
    match ty { "uint" => "uint256".into(), "int" => "int256".into(), t => t.into() }
}

fn split_params(sig: &str) -> Result<(&str, Vec<String>), String> {
    let open = sig.find('(').ok_or_else(|| format!("{sig}: not a signature"))?;
    let inner = sig[open + 1..].strip_suffix(')').ok_or_else(|| format!("{sig}: not a signature"))?;
    if inner.contains('(') { return Err(format!("{sig}: tuple parameters are not supported")); }
    let params = if inner.is_empty() { vec![] } else { inner.split(',').map(|p| canonical_type(p.trim())).collect() };
    Ok((&sig[..open], params))
}

fn encode_arg(ty: &str, arg: &str) -> Result<[u8; 32], String> {
    let arg = arg.trim();
    if ty.ends_with(']') || ty == "string" || ty == "bytes" {
        return Err(format!("{ty}: dynamic and array types are not supported by the terminal's encoder"));
    }
    if ty == "bool" {
        return match arg { "true" | "1" => Ok(U256::ONE.to_be_bytes()), "false" | "0" => Ok([0; 32]), _ => Err(format!("bool: {arg}")) };
    }
    if ty == "address" {
        return Ok(parse_address(arg)?.to_be_bytes());
    }
    if let Some(n) = ty.strip_prefix("bytes") {
        let n: usize = n.parse().map_err(|_| format!("{ty}: not supported by the terminal's encoder"))?;
        if !(1..=32).contains(&n) { return Err(format!("{ty}: not supported")); }
        let b = hex::decode(arg.trim_start_matches("0x")).map_err(|e| format!("{ty}: {e}"))?;
        if b.len() > n { return Err(format!("{ty}: {} bytes given", b.len())); }
        let mut out = [0u8; 32];
        out[..b.len()].copy_from_slice(&b);
        return Ok(out);
    }
    let (signed, bits) = if let Some(b) = ty.strip_prefix("uint") { (false, b) } else if let Some(b) = ty.strip_prefix("int") { (true, b) } else {
        return Err(format!("{ty}: not supported by the terminal's encoder"));
    };
    let bits: usize = if bits.is_empty() { 256 } else { bits.parse().map_err(|_| format!("{ty}: not supported"))? };
    if bits == 0 || bits > 256 || bits % 8 != 0 { return Err(format!("{ty}: not supported")); }
    let (neg, mag) = match arg.strip_prefix('-') { Some(m) if signed => (true, m), Some(_) => return Err(format!("{ty} cannot be negative")), None => (false, arg) };
    let be = parse_u256(mag)?.to_be_bytes();
    // |v| < 2^limit, or ≤ 2^limit for a negative signed value. Big-endian byte arrays compare
    // lexicographically in numeric order.
    let limit = if signed { bits - 1 } else { bits };
    if limit < 256 {
        let mut bound = [0u8; 32];
        bound[31 - limit / 8] = 1 << (limit % 8);
        let ok = if neg { be <= bound } else { be < bound };
        if !ok { return Err(format!("{arg} does not fit in {ty}")); }
    }
    Ok(if neg { neg_be(be) } else { be })
}

/// Two's complement of a big-endian 256-bit value.
fn neg_be(mut b: [u8; 32]) -> [u8; 32] {
    for x in b.iter_mut() { *x = !*x; }
    for x in b.iter_mut().rev() { let (s, c) = x.overflowing_add(1); *x = s; if !c { break; } }
    b
}

/// Selector plus head words for a signature whose parameters are all static (`uintN`, `intN`,
/// `address`, `bool`, `bytesN`). Dynamic, array and tuple types are refused by name.
pub fn encode_calldata(sig: &str, args: &[String]) -> Result<Vec<u8>, String> {
    let (name, params) = split_params(sig)?;
    if params.len() != args.len() { return Err(format!("{sig} expects {} argument(s), got {}", params.len(), args.len())); }
    let canonical = format!("{}({})", name.trim(), params.join(","));
    let mut out = selector(&canonical).to_vec();
    for (ty, a) in params.iter().zip(args) { out.extend_from_slice(&encode_arg(ty, a)?); }
    Ok(out)
}

/// A [`Host`] that delegates to [`HostRef`] and records the hash of every single-block 64-byte
/// Keccak message.
///
/// `evm_core::keccak256` absorbs byte `k` of a block into byte `k % 4` (little-endian) of state word
/// `k / 4`, pads with `0x01` right after the message and `0x80` in byte 135, and reads the digest
/// out of the first 32 state bytes the same way. A 64-byte message is therefore one permutation of
/// a state whose bytes 0..64 are the message, byte 64 is `0x01`, byte 135 is `0x80`, and every
/// other byte — the rest of the rate and the whole capacity, which a fresh sponge leaves zero — is
/// zero. Requiring the capacity to be zero excludes the last block of a multi-block message.
#[derive(Default)]
pub struct TraceHost {
    /// The digests of the 64-byte messages hashed so far, oldest first, as `U256::from_be_bytes`.
    pub mapping_hashes: Vec<U256>,
}

impl Host for TraceHost {
    fn keccak_f(&mut self, state: &mut [u32; 50]) {
        let byte = |s: &[u32; 50], k: usize| s[k / 4].to_le_bytes()[k % 4];
        let is_64 = byte(state, 64) == 0x01
            && byte(state, 135) == 0x80
            && (65..135).chain(136..200).all(|k| byte(state, k) == 0);
        HostRef.keccak_f(state);
        if is_64 {
            let mut h = [0u8; 32];
            for (k, b) in h.iter_mut().enumerate() { *b = byte(state, k); }
            self.mapping_hashes.push(U256::from_be_bytes(&h));
        }
    }
    fn poseidon2(&mut self, words: &mut [u32], n: usize) { HostRef.poseidon2(words, n) }
}

fn dry_run(call: &EvmCall) -> (Outcome, Vec<U256>) {
    let words = call.input_words();
    let mut ws = Box::new(Workspace::ZERO);
    let mut host = TraceHost::default();
    let (_, o) = run_call_with(&mut host, &mut ws, |i| words[i as usize], words.len() as u32);
    (o, host.mapping_hashes)
}

fn collides(touched: &[U256], s: &U256) -> bool { touched.iter().any(|t| slot_index(t) == slot_index(s)) }

/// The pre-state tree and the distinct seeded slots, in form order.
fn seed_tree(seeds: &[StorageSeed]) -> Result<(SparseTree, Vec<U256>), String> {
    let mut tree = SparseTree::new();
    let mut slots: Vec<U256> = Vec::new();
    for s in seeds {
        let (slot, value) = match s {
            StorageSeed::Slot { slot, value } => (parse_u256(slot)?, parse_u256(value)?),
            StorageSeed::Mapping { base, key, value } => (mapping_slot(&parse_u256(key)?, *base), parse_u256(value)?),
            StorageSeed::Mapping2 { base, key1, key2, value } => (mapping_slot2(&parse_u256(key1)?, &parse_u256(key2)?, *base), parse_u256(value)?),
        };
        if slots.contains(&slot) { return Err(format!("storage slot {} is seeded twice", u256_hex(&slot))); }
        if collides(&slots, &slot) {
            return Err(format!("storage slot {} shares a leaf position with another seeded slot", u256_hex(&slot)));
        }
        tree.insert(slot, value);
        slots.push(slot);
    }
    Ok((tree, slots))
}

/// The call the form describes, with `touched` discovered, and the discovery steps taken.
pub fn build(code: &[u8], form: &EvmCallForm) -> Result<(EvmCall, Vec<DiscoveryStep>), String> {
    if form.gas_limit > u32::MAX as u64 {
        return Err(format!("gas limit {} is above {}, the most the guest's one gas word carries", form.gas_limit, u32::MAX));
    }
    let calldata = encode_calldata(&form.function, &form.args)?;
    let (tree, seeded) = seed_tree(&form.storage)?;
    if seeded.len() > MAX_WITNESSES { return Err(format!("{} seeded slots; the guest takes at most {MAX_WITNESSES}", seeded.len())); }
    let mut call = EvmCall {
        code: code.to_vec(), calldata,
        address: form.address.as_deref().map(parse_address).transpose()?.unwrap_or(TOKEN),
        caller: parse_address(&form.caller)?, callvalue: parse_u256(&form.value)?,
        gas_limit: form.gas_limit, tree, touched: seeded,
    };
    let mut steps = Vec::new();
    let mut tried_constants: BTreeSet<u32> = BTreeSet::new();
    // Every run either returns or adds one witness, and the cap returns at MAX_WITNESSES, so this
    // loop runs at most MAX_WITNESSES + 1 times.
    for run in 0.. {
        let (o, hashes) = dry_run(&call);
        if o.halt != Halt::NoWitness {
            prune(&mut call, &o);
            return Ok((call, steps));
        }
        if call.touched.len() >= MAX_WITNESSES {
            // Constant guesses fill the cap one by one, so once any were made the honest message
            // is that the slot was never identified, not that the call reads too much.
            if !tried_constants.is_empty() {
                return Err(format!("{UNIDENTIFIED} within the {MAX_WITNESSES}-witness limit"));
            }
            return Err(format!("the call reads more than {MAX_WITNESSES} storage slots, the most one guest input can carry"));
        }
        let usable = |s: &U256, touched: &[U256]| !touched.contains(s) && !collides(touched, s);
        let next = hashes.iter().rev().find(|h| usable(h, &call.touched)).copied().or_else(|| {
            let c = (0..CONSTANT_SLOTS).find(|c| !tried_constants.contains(c) && usable(&U256::from_u32(*c), &call.touched))?;
            tried_constants.insert(c);
            Some(U256::from_u32(c))
        });
        let Some(slot) = next else {
            return Err(UNIDENTIFIED.into());
        };
        call.touched.push(slot);
        steps.push(DiscoveryStep { run, slot: u256_hex(&slot), gas_used: o.gas_used });
    }
    unreachable!("the witness cap ends discovery")
}

const UNIDENTIFIED: &str = "could not identify the storage slot the call reads (it is neither a constant below 16 nor a mapping entry)";

/// Remove every witness the call does not need — discovered or seeded — keeping one only when the
/// call's halt or gas changes without it. A seeded slot that is never read stays in the pre-state
/// (`tree`), just without a witness, as `rand_zkvm::evm::erc20_transfer` leaves `_totalSupply`.
fn prune(call: &mut EvmCall, reference: &Outcome) {
    let mut i = 0;
    while i < call.touched.len() {
        let s = call.touched.remove(i);
        let (o, _) = dry_run(call);
        if o.halt == reference.halt && o.gas_used == reference.gas_used { continue; }
        call.touched.insert(i, s);
        i += 1;
    }
}

/// The ERC-20 sample's default call: ALICE sends BOB 250 of her 1 000, the pre-state
/// `rand_zkvm::evm::erc20_transfer(ALICE, BOB, 250, &[(ALICE, 1000)])` builds (her balance and a
/// `_totalSupply` of 1 000).
pub fn erc20_transfer_form() -> EvmCallForm {
    EvmCallForm {
        function: "transfer(address,uint256)".into(), args: vec![u256_hex(&BOB), "250".into()],
        caller: u256_hex(&ALICE), address: None, value: "0".into(), gas_limit: 1_000_000,
        storage: vec![
            StorageSeed::Mapping { base: SLOT_BALANCES, key: u256_hex(&ALICE), value: "1000".into() },
            StorageSeed::Slot { slot: SLOT_TOTAL_SUPPLY.to_string(), value: "1000".into() },
        ],
    }
}
