#[path = "fixtures.rs"]
mod fixtures;
use rand_zkvm::sbpf::spl_transfer;
use terminal_core::sbpf_call::{build, parse_key, spl_transfer_form};

#[test]
fn spl_transfer_round_trips() {
    let want = spl_transfer(250);
    let form = spl_transfer_form(250);
    assert_eq!(form.accounts.len(), 4);
    assert!(form.accounts[2].is_signer);
    assert_eq!(form.data, "0x03fa00000000000000");
    let got = build(&want.elf, &form).unwrap();
    assert_eq!(got.input, want.input);
    assert_eq!(got.public_words(), fixtures::words("spl-transfer.public"));
    assert_eq!(got.input_words(), fixtures::words("spl-transfer.input"));
}

#[test]
fn keys() {
    let k = parse_key("11111111111111111111111111111111").unwrap();
    assert_eq!(k, [0u8; 32]);
    assert_eq!(parse_key(&format!("0x{}", "ab".repeat(32))).unwrap(), [0xab; 32]);
    assert!(parse_key("0x1234").is_err());
    assert!(parse_key("not base58 0OIl").is_err());
}

#[test]
fn limits() {
    let mut form = spl_transfer_form(1);
    form.data = "0xzz".into();
    assert!(build(b"elf", &form).is_err());
    let mut form = spl_transfer_form(1);
    form.accounts = (0..65).map(|_| form.accounts[0].clone()).collect();
    assert!(build(b"elf", &form).unwrap_err().contains("64"));
}
