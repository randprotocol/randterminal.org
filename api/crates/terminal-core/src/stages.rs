//! The file formats at each stage boundary: what goes into solc and comes out, the shim crate the
//! RV32 build compiles, and cargo's JSON diagnostics. Pure functions; the API drives the I/O.
use crate::views::{Diagnostic, StageError};
use serde_json::{json, Value};
use std::io;
use std::path::Path;

pub const SHIM_UP: &str = "../../..";

pub fn solc_input(files: &[(String, String)]) -> Value {
    let sources: serde_json::Map<String, Value> = files.iter().map(|(n, c)| (n.clone(), json!({ "content": c }))).collect();
    json!({
        "language": "Solidity",
        "sources": sources,
        "settings": {
            "optimizer": { "enabled": true, "runs": 200 },
            "evmVersion": "shanghai",
            "outputSelection": { "*": { "*": ["abi", "evm.bytecode.object", "evm.deployedBytecode.object"] } }
        }
    })
}

pub fn line_col(src: &str, byte: usize) -> (u32, u32) {
    let byte = byte.min(src.len());
    let before = &src.as_bytes()[..byte];
    let line = before.iter().filter(|&&b| b == b'\n').count() as u32 + 1;
    let col = (byte - before.iter().rposition(|&b| b == b'\n').map(|p| p + 1).unwrap_or(0)) as u32 + 1;
    (line, col)
}

#[derive(Debug, Clone)]
pub struct SolcResult { pub contract: String, pub contracts: Vec<String>, pub runtime: Vec<u8>, pub creation_hex: String, pub abi: Value }

fn err(kind: &str, message: String, diagnostics: Vec<Diagnostic>) -> StageError { StageError { kind: kind.into(), message, diagnostics } }

pub fn parse_solc_output(out: &Value, files: &[(String, String)], want: Option<&str>) -> Result<SolcResult, StageError> {
    let diagnostics: Vec<Diagnostic> = out["errors"].as_array().into_iter().flatten().map(|e| {
        let file = e["sourceLocation"]["file"].as_str().unwrap_or("").to_string();
        let start = e["sourceLocation"]["start"].as_u64().unwrap_or(0) as usize;
        let src = files.iter().find(|(n, _)| *n == file).map(|(_, c)| c.as_str()).unwrap_or("");
        let (line, col) = if file.is_empty() { (0, 0) } else { line_col(src, start) };
        Diagnostic { file, line, col, severity: e["severity"].as_str().unwrap_or("error").to_string(), message: e["message"].as_str().unwrap_or("").to_string() }
    }).collect();
    if let Some(first) = diagnostics.iter().find(|d| d.severity == "error") {
        let message = format!("{}:{}:{}: {}", first.file, first.line, first.col, first.message);
        return Err(err("compile", message, diagnostics));
    }
    let mut found: Vec<(String, Value)> = Vec::new();
    for (_, by_name) in out["contracts"].as_object().into_iter().flatten() {
        for (name, c) in by_name.as_object().into_iter().flatten() {
            if !c["evm"]["deployedBytecode"]["object"].as_str().unwrap_or("").is_empty() { found.push((name.clone(), c.clone())); }
        }
    }
    found.sort_by(|a, b| a.0.cmp(&b.0));
    let contracts: Vec<String> = found.iter().map(|(n, _)| n.clone()).collect();
    let chosen = match (want, found.len()) {
        (_, 0) => return Err(err("compile", "no deployable contract: every contract is abstract or an interface".into(), diagnostics)),
        (Some(w), _) => found.iter().find(|(n, _)| n == w).ok_or_else(|| err("unsupported", format!("no contract named {w}; the file defines {}", contracts.join(", ")), diagnostics.clone()))?,
        (None, 1) => &found[0],
        (None, _) => return Err(err("unsupported", format!("choose a contract: the file defines {}", contracts.join(", ")), diagnostics)),
    };
    let runtime = hex::decode(chosen.1["evm"]["deployedBytecode"]["object"].as_str().unwrap()).map_err(|e| err("compile", format!("solc printed bad hex: {e}"), vec![]))?;
    Ok(SolcResult {
        contract: chosen.0.clone(), contracts, runtime,
        creation_hex: chosen.1["evm"]["bytecode"]["object"].as_str().unwrap_or("").to_string(),
        abi: chosen.1["abi"].clone(),
    })
}

fn write_all(dir: &Path, entries: &[(&str, &str)]) -> io::Result<()> {
    std::fs::create_dir_all(dir.join("src"))?;
    for (name, content) in entries { std::fs::write(dir.join(name), content)?; }
    Ok(())
}

pub fn write_evm_shim(dir: &Path, name: &str, contract_c: &str) -> io::Result<()> {
    let f = evm2rv::shim::files(name, SHIM_UP);
    write_all(dir, &[("contract.c", contract_c), ("Cargo.toml", &f.cargo_toml), ("Cargo.lock", &f.cargo_lock), ("build.rs", &f.build_rs), ("src/main.rs", &f.main_rs), ("shim.ld", &f.ld)])
}

pub fn write_sbpf_shim(dir: &Path, name: &str, program_c: &str) -> io::Result<()> {
    use sbpf2rv::shim;
    write_all(dir, &[("program.c", program_c), ("Cargo.toml", &shim::cargo_toml(name, SHIM_UP)), ("Cargo.lock", &shim::cargo_lock(name)), ("build.rs", &shim::build_rs(SHIM_UP)), ("src/main.rs", shim::MAIN_RS), ("shim.ld", shim::LINKER_SCRIPT)])
}

pub fn rustc_diagnostics(stdout: &str) -> Vec<Diagnostic> {
    stdout.lines().filter_map(|l| serde_json::from_str::<Value>(l).ok()).filter(|v| v["reason"] == "compiler-message").map(|v| {
        let m = &v["message"];
        let span = m["spans"].as_array().and_then(|s| s.iter().find(|s| s["is_primary"] == true).or_else(|| s.first())).cloned().unwrap_or(Value::Null);
        Diagnostic {
            file: span["file_name"].as_str().unwrap_or("").to_string(),
            line: span["line_start"].as_u64().unwrap_or(0) as u32,
            col: span["column_start"].as_u64().unwrap_or(0) as u32,
            severity: m["level"].as_str().unwrap_or("error").to_string(),
            message: m["message"].as_str().unwrap_or("").to_string(),
        }
    }).collect()
}
