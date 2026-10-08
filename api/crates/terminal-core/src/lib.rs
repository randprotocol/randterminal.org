//! Rand Terminal's core: the four pipeline stages as pure functions over files and the circuits
//! crates, and the DTOs the API serves. No HTTP, no Docker here.
pub mod rv_disasm;
pub mod views;
pub mod sbpf_disasm;
pub mod evm_analysis;
pub mod source_map;
pub mod sbpf_analysis;
