#[path = "fixtures.rs"]
mod fixtures;

#[test]
fn fixtures_are_present() {
    assert_eq!(fixtures::erc20_code().len(), 1296);
    assert_eq!(fixtures::words("erc20-transfer.words").len(), 921);
    assert_eq!(fixtures::bytes("spl.image.bin").len(), 259_708);
}
