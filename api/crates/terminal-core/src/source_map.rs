//! Source maps: the C markers the transpilers emit, the DWARF line table of the `-g` side build,
//! the symbol table of the production ELF, and their composition. Task 6 adds the DWARF and nm
//! parsers and `compose`.
use crate::views::OpLine;

/// evm2rv writes `/* 0xPC MNEMONIC ... */` at the start (after indentation) of the C for each
/// opcode. One entry per marker, in file order, lines 1-based.
pub fn evm_markers(c: &str) -> Vec<OpLine> {
    c.lines().enumerate().filter_map(|(i, l)| {
        let t = l.trim_start();
        let rest = t.strip_prefix("/* 0x")?;
        let hex: String = rest.chars().take_while(|ch| ch.is_ascii_hexdigit()).collect();
        if hex.is_empty() || !rest[hex.len()..].starts_with(' ') { return None; }
        Some(OpLine { pc: usize::from_str_radix(&hex, 16).ok()?, line: i + 1 })
    }).collect()
}

/// sbpf2rv labels every block `L_<pc>:` (pc in slots). One entry per label, lines 1-based.
pub fn sbpf_labels(c: &str) -> Vec<OpLine> {
    c.lines().enumerate().filter_map(|(i, l)| {
        let t = l.trim();
        let rest = t.strip_prefix("L_")?.strip_suffix(':')?;
        Some(OpLine { pc: rest.parse().ok()?, line: i + 1 })
    }).collect()
}
