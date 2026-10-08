//! Paths and loaders for the checked-in fixtures. Included by every test file with
//! `#[path = "fixtures.rs"] mod fixtures;`.
#![allow(dead_code)]
use std::path::PathBuf;

pub fn dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}
pub fn bytes(name: &str) -> Vec<u8> {
    std::fs::read(dir().join(name)).unwrap_or_else(|e| panic!("{name}: {e}"))
}
pub fn text(name: &str) -> String {
    String::from_utf8(bytes(name)).unwrap()
}
/// `erc20.runtime.hex` as bytes (the file is hex text with a trailing newline).
pub fn erc20_code() -> Vec<u8> {
    hex::decode(text("erc20.runtime.hex").trim().trim_start_matches("0x")).unwrap()
}
/// Whitespace-separated decimal words, as `rand-guest run --input` takes them.
pub fn words(name: &str) -> Vec<u32> {
    text(name)
        .split_whitespace()
        .map(|w| w.parse().unwrap())
        .collect()
}
