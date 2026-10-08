#[path = "fixtures.rs"]
mod fixtures;
use evm_core::u256::U256;
use rand_zkvm::evm::{abi_call, erc20_transfer, mapping_slot, mapping_slot2, ALICE, BOB};
use terminal_core::evm_call::{build, encode_calldata, erc20_transfer_form, parse_u256, u256_hex, TraceHost};
use terminal_core::views::{EvmCallForm, StorageSeed};

#[test]
fn trace_host_matches_evm_core_keccak_absorption() {
    // A 64-byte `key ‖ base` message through evm-core's own sponge: one fresh-state permutation,
    // recognised by its padding, and recorded as the same U256 research's mapping_slot computes.
    for (key, base) in [(ALICE, 0u32), (BOB, 0), (ALICE, 1), (U256::from_u32(7), 5)] {
        let mut msg = [0u8; 64];
        msg[..32].copy_from_slice(&key.to_be_bytes());
        msg[32..].copy_from_slice(&U256::from_u32(base).to_be_bytes());
        let mut h = TraceHost::default();
        let digest = evm_core::keccak256(&mut h, &msg);
        assert_eq!(h.mapping_hashes, vec![mapping_slot(&key, base)]);
        assert_eq!(U256::from_be_bytes(&digest), mapping_slot(&key, base));
    }
    // The nested form is also one 64-byte message.
    let mut msg = [0u8; 64];
    msg[..32].copy_from_slice(&BOB.to_be_bytes());
    msg[32..].copy_from_slice(&mapping_slot(&ALICE, 1).to_be_bytes());
    let mut h = TraceHost::default();
    evm_core::keccak256(&mut h, &msg);
    assert_eq!(h.mapping_hashes, vec![mapping_slot2(&ALICE, &BOB, 1)]);
    // Other lengths (32 bytes: a leaf index; 63, 65; and a multi-block message) are not recorded.
    for len in [0usize, 32, 63, 65, 136, 200, 136 + 64] {
        let mut h = TraceHost::default();
        evm_core::keccak256(&mut h, &vec![0xabu8; len]);
        assert!(h.mapping_hashes.is_empty(), "len {len}: {:?}", h.mapping_hashes);
    }
}

#[test]
fn parses_numbers() {
    assert_eq!(parse_u256("250").unwrap(), U256::from_u32(250));
    assert_eq!(parse_u256("0xfa").unwrap(), U256::from_u32(250));
    assert_eq!(parse_u256("1000000000000000000").unwrap(), U256::from_u64(1_000_000_000_000_000_000));
    assert!(parse_u256("").is_err());
    assert!(parse_u256("12a").is_err());
    assert!(parse_u256(&format!("0x1{}", "0".repeat(64))).is_err(), "257 bits");
    assert!(parse_u256("115792089237316195423570985008687907853269984665640564039457584007913129639936").is_err(), "2^256");
}

#[test]
fn encodes_like_abi_call_for_static_types() {
    let bob = u256_hex(&BOB);
    let got = encode_calldata("transfer(address,uint256)", &[bob, "250".into()]).unwrap();
    assert_eq!(got, abi_call("transfer(address,uint256)", &[BOB, U256::from_u32(250)]));
    let got = encode_calldata("f(bool,bytes4,int8)", &["true".into(), "0xdeadbeef".into(), "-1".into()]).unwrap();
    assert_eq!(&got[4 + 31..4 + 32], &[1]);
    assert_eq!(&got[4 + 32..4 + 36], &[0xde, 0xad, 0xbe, 0xef]);
    assert!(got[4 + 64..4 + 96].iter().all(|&b| b == 0xff), "int8 -1 is sign-extended");
}

#[test]
fn refuses_dynamic_types_by_name() {
    for sig in ["f(string)", "f(bytes)", "f(uint256[])", "f(uint256[2])", "f((uint256,bool))"] {
        let e = encode_calldata(sig, &["x".into()]).unwrap_err();
        assert!(e.contains("not supported"), "{sig}: {e}");
    }
    assert!(encode_calldata("f(uint256)", &[]).unwrap_err().contains("expects 1"));
    assert!(encode_calldata("f(address)", &["0x1234".into()]).is_ok(), "short addresses are left-padded");
    assert!(encode_calldata("f(address)", &[format!("0x{}", "1".repeat(42))]).is_err(), "21 bytes");
    assert!(encode_calldata("f(uint8)", &["256".into()]).is_err());
}

#[test]
fn discovers_the_erc20_transfer_witnesses() {
    let code = fixtures::erc20_code();
    let form = erc20_transfer_form();
    let (call, steps) = build(&code, &form).unwrap();
    let want = erc20_transfer(ALICE, BOB, U256::from_u32(250), &[(ALICE, U256::from_u32(1000))]);
    let mut got_touched = call.touched.clone();
    let mut want_touched = want.touched.clone();
    got_touched.sort_by_key(|s| s.0); want_touched.sort_by_key(|s| s.0);
    assert_eq!(got_touched, want_touched, "discovered {steps:?}");
    // same input words as the reference vector, up to witness order
    assert_eq!(call.calldata, want.calldata);
    assert_eq!(call.tree.root(), want.tree.root());
    // the reference outcome: success
    let (_, outcome, _) = call.expected();
    assert_eq!(outcome.status(), 1);
    assert!(steps.iter().any(|s| s.slot == u256_hex(&mapping_slot(&BOB, 0))), "{steps:?}");
}

#[test]
fn a_revert_is_not_an_error() {
    // ALICE has nothing: transfer reverts after reading her balance.
    let mut form = erc20_transfer_form();
    form.storage.clear();
    let (call, _) = build(&fixtures::erc20_code(), &form).unwrap();
    assert_eq!(call.expected().1.status(), 0);
}

#[test]
fn witness_cap_is_an_error() {
    // 17 constant SLOADs (slots 0..=16) cannot be satisfied in 16 witnesses.
    let mut code = Vec::new();
    for s in 0u8..=16 { code.extend_from_slice(&[0x60, s, 0x54, 0x50]); } // PUSH1 s SLOAD POP
    code.push(0x00);
    let form = EvmCallForm { function: "f()".into(), args: vec![], caller: u256_hex(&ALICE), address: None, value: "0".into(), gas_limit: 1_000_000, storage: vec![] };
    let e = build(&code, &form).err().expect("17 slots cannot fit");
    assert!(e.contains("16"), "{e}");
}

#[test]
fn seeds_become_storage() {
    let form = EvmCallForm {
        function: "f()".into(), args: vec![], caller: u256_hex(&ALICE), address: None, value: "0".into(), gas_limit: 100_000,
        storage: vec![StorageSeed::Slot { slot: "2".into(), value: "7".into() }],
    };
    let code = vec![0x60, 0x02, 0x54, 0x50, 0x00]; // PUSH1 2 SLOAD POP STOP
    let (call, steps) = build(&code, &form).unwrap();
    assert_eq!(call.touched, vec![U256::from_u32(2)]);
    assert!(steps.is_empty(), "a seeded slot needs no discovery");
}
