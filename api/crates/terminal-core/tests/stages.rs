#[path = "fixtures.rs"]
mod fixtures;
use terminal_core::stages::*;

const SRC: &str =
    "pragma solidity ^0.8.0;\ncontract Token {\n  function f() public pure { uint x; }\n}\n";

fn files() -> Vec<(String, String)> {
    vec![("Token.sol".into(), SRC.into())]
}

#[test]
fn input_has_the_pinned_settings() {
    let v = solc_input(&files());
    assert_eq!(v["language"], "Solidity");
    assert_eq!(v["sources"]["Token.sol"]["content"], SRC);
    assert_eq!(v["settings"]["optimizer"]["enabled"], true);
    assert_eq!(v["settings"]["optimizer"]["runs"], 200);
    assert_eq!(v["settings"]["evmVersion"], "shanghai");
    assert_eq!(
        v["settings"]["outputSelection"]["*"]["*"],
        serde_json::json!(["abi", "evm.bytecode.object", "evm.deployedBytecode.object"])
    );
}

#[test]
fn several_deployable_contracts_need_a_choice() {
    let out: serde_json::Value = serde_json::from_str(&fixtures::text("solc-output.json")).unwrap();
    let e = parse_solc_output(&out, &files(), None).unwrap_err();
    assert_eq!(e.kind, "unsupported");
    assert!(
        e.message.contains("Token")
            && e.message.contains("Helper")
            && !e.message.contains("IERC20"),
        "{}",
        e.message
    );
    let r = parse_solc_output(&out, &files(), Some("Token")).unwrap();
    assert_eq!(r.contract, "Token");
    assert_eq!(r.contracts, vec!["Helper".to_string(), "Token".to_string()]);
    assert_eq!(r.runtime, vec![0x60, 0x80, 0x60, 0x40]);
    assert_eq!(r.creation_hex, "6080604052");
    assert_eq!(r.abi[0]["name"], "f");
    assert!(parse_solc_output(&out, &files(), Some("Nope"))
        .unwrap_err()
        .message
        .contains("Nope"));
}

#[test]
fn errors_become_diagnostics() {
    let out = serde_json::json!({"errors": [{"severity": "error", "message": "Expected ';'", "sourceLocation": {"file": "Token.sol", "start": 41, "end": 42}}]});
    let e = parse_solc_output(&out, &files(), None).unwrap_err();
    assert_eq!(e.kind, "compile");
    assert_eq!(e.diagnostics.len(), 1);
    let d = &e.diagnostics[0];
    assert_eq!(
        (d.file.as_str(), d.line, d.col, d.severity.as_str()),
        ("Token.sol", 3, 1, "error")
    );
}

#[test]
fn line_col_counts_bytes_on_the_line() {
    assert_eq!(line_col("ab\ncd\n", 0), (1, 1));
    assert_eq!(line_col("ab\ncd\n", 4), (2, 2));
    assert_eq!(line_col("ab", 99), (1, 3));
}

#[test]
fn evm_shim_equals_what_evm2rv_writes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../circuits");
    let reference = root.join("target/terminal-shim-test/shim");
    let _ = std::fs::remove_dir_all(&reference);
    evm2rv::shim::write_crate(&reference, "contract", "int x;\n").unwrap();
    let ours = tempfile::tempdir().unwrap();
    write_evm_shim(ours.path(), "contract", "int x;\n").unwrap();
    for f in [
        "contract.c",
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
        "src/main.rs",
        "shim.ld",
    ] {
        assert_eq!(
            std::fs::read_to_string(ours.path().join(f)).unwrap(),
            std::fs::read_to_string(reference.join(f)).unwrap(),
            "{f}"
        );
    }
    let _ = std::fs::remove_dir_all(root.join("target/terminal-shim-test"));
}

#[test]
fn sbpf_shim_equals_what_sbpf2rv_writes() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../circuits");
    let reference = root.join("target/terminal-sbpf-test/shim");
    let _ = std::fs::remove_dir_all(&reference);
    let a = terminal_core::sbpf_analysis::analyse(&fixtures::bytes("spl_token.so")).unwrap();
    let body = a
        .program_c
        .strip_suffix(&sbpf2rv::shim::view_digest_c(&a.digest))
        .unwrap();
    sbpf2rv::shim::write_crate(&reference, "spl-token", body, &a.digest).unwrap();
    let ours = tempfile::tempdir().unwrap();
    write_sbpf_shim(ours.path(), "spl-token", &a.program_c).unwrap();
    for f in [
        "program.c",
        "Cargo.toml",
        "Cargo.lock",
        "build.rs",
        "src/main.rs",
        "shim.ld",
    ] {
        assert_eq!(
            std::fs::read_to_string(ours.path().join(f)).unwrap(),
            std::fs::read_to_string(reference.join(f)).unwrap(),
            "{f}"
        );
    }
    let _ = std::fs::remove_dir_all(root.join("target/terminal-sbpf-test"));
}

#[test]
fn rustc_json_diagnostics() {
    let stdout = r#"{"reason":"compiler-artifact","package_id":"x"}
{"reason":"compiler-message","message":{"level":"error","message":"cannot find value `y` in this scope","spans":[{"file_name":"src/lib.rs","line_start":4,"column_start":9,"is_primary":true},{"file_name":"src/lib.rs","line_start":1,"column_start":1,"is_primary":false}]}}
{"reason":"compiler-message","message":{"level":"warning","message":"unused import","spans":[]}}
not json
"#;
    let d = rustc_diagnostics(stdout);
    assert_eq!(d.len(), 2);
    assert_eq!(
        (
            d[0].file.as_str(),
            d[0].line,
            d[0].col,
            d[0].severity.as_str()
        ),
        ("src/lib.rs", 4, 9, "error")
    );
    assert_eq!((d[1].file.as_str(), d[1].line), ("", 0));
}

use std::path::PathBuf;
