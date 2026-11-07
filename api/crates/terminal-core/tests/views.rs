use serde_json::json;
use terminal_core::views::*;

#[test]
fn new_job_without_options_gets_defaults() {
    let j: NewJob = serde_json::from_str(r#"{"lang":"solidity","files":[]}"#).unwrap();
    assert_eq!((j.options.chain_id, j.options.stage), (20, 2));
    assert!(j.options.contract.is_none());
}

#[test]
fn empty_and_default_options_agree() {
    let e: JobOptions = serde_json::from_str("{}").unwrap();
    let d = JobOptions::default();
    for o in [e, d] {
        assert_eq!((o.chain_id, o.stage), (20, 2));
        assert!(o.contract.is_none());
    }
}

#[test]
fn tag_and_rename_shapes() {
    assert_eq!(
        serde_json::to_value(Lang::SolanaRust).unwrap(),
        json!("solana-rust")
    );
    let v = CompileView::Evm(EvmCompileView {
        contract: "C".into(),
        contracts: vec![],
        creation_hex: String::new(),
        runtime_hex: String::new(),
        size_bytes: 0,
        abi: json!([]),
        opcodes: vec![],
        blocks: 0,
        jumpdests: 0,
        evm_version: "shanghai".into(),
        warnings: vec![],
    });
    assert_eq!(serde_json::to_value(&v).unwrap()["lang"], "evm");
    // A compile view written before the fork picker reads as shanghai with no warnings.
    let mut old = serde_json::to_value(&v).unwrap();
    old.as_object_mut().unwrap().remove("evm_version");
    old.as_object_mut().unwrap().remove("warnings");
    let CompileView::Evm(back) = serde_json::from_value(old).unwrap() else {
        panic!()
    };
    assert_eq!(back.evm_version, "shanghai");
    assert!(back.warnings.is_empty());
    // Job options: evm_version is optional on the wire and absent when unset.
    let o: JobOptions = serde_json::from_value(json!({"chain_id": 20, "stage": 2})).unwrap();
    assert_eq!(o.evm_version, None);
    assert!(serde_json::to_value(&o)
        .unwrap()
        .get("evm_version")
        .is_none());
    let o: JobOptions = serde_json::from_value(json!({"evm_version": "cancun"})).unwrap();
    assert_eq!(o.evm_version.as_deref(), Some("cancun"));
    let s = StorageSeed::Mapping2 {
        base: 1,
        key1: "a".into(),
        key2: "b".into(),
        value: "c".into(),
    };
    assert_eq!(serde_json::to_value(&s).unwrap()["kind"], "mapping2");
}

#[test]
fn evm_call_form_defaults() {
    let c: CallForm =
        serde_json::from_str(r#"{"lang":"evm","function":"f()","args":[],"caller":"0x01"}"#)
            .unwrap();
    let CallForm::Evm(e) = c else {
        panic!("not evm")
    };
    assert_eq!(e.value, "0");
    assert_eq!(e.gas_limit, 1_000_000);
    assert!(e.storage.is_empty());
    assert!(e.address.is_none());
}
