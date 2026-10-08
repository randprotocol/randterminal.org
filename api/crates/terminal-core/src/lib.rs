//! Rand Terminal's core: the four pipeline stages as pure functions over files and the circuits
//! crates, and the DTOs the API serves. No HTTP, no Docker here.
pub mod evm_analysis;
pub mod evm_call;
pub mod image;
pub mod rv_disasm;
pub mod sbpf_analysis;
pub mod sbpf_call;
pub mod sbpf_disasm;
pub mod source_map;
pub mod stages;
pub mod tiers;
pub mod trace;
pub mod views;
