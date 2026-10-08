//! Source maps: the C markers the transpilers emit, the DWARF line table of the `-g` side build,
//! the symbol table of the production ELF, and their composition. Task 6 adds the DWARF and nm
//! parsers and `compose`.
use crate::views::{OpLine, SourceMap};

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

/// Rows of `llvm-dwarfdump --debug-line` whose file is `c_file` (by base name). Only `is_stmt`
/// rows count (the others are mid-statement), `end_sequence` rows are not instructions, and the
/// first row wins when an address repeats.
pub fn parse_debug_line(text: &str, c_file: &str) -> Vec<(u32, usize)> {
    let mut files: Vec<String> = Vec::new();
    let mut pending_index: Option<usize> = None;
    let mut rows: Vec<(u32, usize)> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if let Some(rest) = t.strip_prefix("file_names[") {
            pending_index = rest.split(']').next().and_then(|n| n.trim().parse().ok());
            continue;
        }
        if let (Some(i), Some(rest)) = (pending_index, t.strip_prefix("name: \"")) {
            let name = rest.trim_end_matches('"').to_string();
            if files.len() <= i { files.resize(i + 1, String::new()); }
            files[i] = name;
            pending_index = None;
            continue;
        }
        if !t.starts_with("0x") { continue; }
        let cols: Vec<&str> = t.split_whitespace().collect();
        if cols.len() < 4 { continue; }
        let Ok(addr) = u64::from_str_radix(cols[0].trim_start_matches("0x"), 16) else { continue };
        let Ok(ln) = cols[1].parse::<usize>() else { continue };
        let Ok(fi) = cols[3].parse::<usize>() else { continue };
        let flags = &cols[7..];
        if !flags.contains(&"is_stmt") || flags.contains(&"end_sequence") { continue; }
        let matches = files.get(fi).map(|f| f.rsplit('/').next() == Some(c_file)).unwrap_or(false);
        if !matches { continue; }
        rows.push((addr as u32, ln));
    }
    rows.sort_by_key(|r| r.0);
    rows.dedup_by_key(|r| r.0);
    rows
}

pub fn compose(pc_rows: &[(u32, usize)], op_map: &[OpLine]) -> SourceMap {
    let mut c_to_src: Vec<(usize, usize)> = op_map.iter().map(|o| (o.line, o.pc)).collect();
    c_to_src.sort();
    SourceMap { pc_to_c: pc_rows.to_vec(), c_to_src }
}

/// The source pc whose C begins at or before `line` (markers cover the lines after them).
pub fn src_pc_for_line(op_map: &[OpLine], line: usize) -> Option<usize> {
    op_map.iter().take_while(|o| o.line <= line).last().map(|o| o.pc)
}
