//! The EVM side of stages 1 and 2: what the bytecode looks like to the transpiler, and what the
//! transpiler makes of it.
use crate::source_map::evm_markers;
use crate::views::*;
use evm2rv::blocks::{self, Block, Term, Warning};
use evm2rv::emit::{self, EmitError, Options, Stage};

#[derive(Debug, thiserror::Error)]
pub enum EvmTranspileError {
    #[error("the code uses CHAINID at pc {pc}; a chain id is required")]
    ChainIdRequired { pc: usize },
    #[error("stage must be 1 or 2, not {0}")]
    BadStage(u8),
    #[error("unsupported: {}", .0.detail)]
    Unsupported(WarningView),
}

fn push_hex(op: &blocks::Op) -> Option<String> {
    let bytes = op.push?;
    let n = (op.opcode as usize).checked_sub(0x5f)?.clamp(1, 32);
    Some(hex::encode(&bytes[32 - n..]))
}

/// Where solc's CBOR metadata trailer starts: the code's last two bytes are the trailer's length,
/// and the trailer is a CBOR map (head 0xa0-0xbf). `code.len()` when there is no such trailer.
/// The trailer is data no jump can reach, so its bytes are never reported as opcodes.
pub fn metadata_start(code: &[u8]) -> usize {
    let len = code.len();
    if len < 2 {
        return len;
    }
    let n = u16::from_be_bytes([code[len - 2], code[len - 1]]) as usize;
    match len.checked_sub(n + 2) {
        Some(start) if n > 0 && (0xa0..=0xbf).contains(&code[start]) => start,
        _ => len,
    }
}

/// Names for opcodes the zkVM's interpreter does not implement but a newer fork defines.
fn fork_mnemonic(op: u8) -> Option<&'static str> {
    Some(match op {
        0x1e => "CLZ",
        0x49 => "BLOBHASH",
        0x4a => "BLOBBASEFEE",
        0x5c => "TLOAD",
        0x5d => "TSTORE",
        0x5e => "MCOPY",
        _ => return None,
    })
}

/// evm2rv's mnemonic, with the newer forks' names in place of `UNDEFINED_0x..`.
pub fn mnemonic(op: u8) -> String {
    fork_mnemonic(op)
        .map(str::to_string)
        .unwrap_or_else(|| emit::mnemonic(op))
}

/// Every opcode the zkVM would trap on, in pc order: evm2rv's own warnings (block context,
/// cross-contract, calls, code size) and, which evm2rv does not report, every opcode byte the
/// interpreter does not implement (Cancun's TLOAD/TSTORE/MCOPY, Osaka's CLZ, undefined bytes),
/// which its block split turns into a trap. Bytes in solc's metadata trailer are skipped.
pub fn zkvm_warnings(code: &[u8]) -> Vec<WarningView> {
    let end = metadata_start(code);
    let mut out: Vec<WarningView> = blocks::warnings(code)
        .iter()
        .map(warning_view)
        .filter(|w| w.pc.map(|pc| pc < end).unwrap_or(true))
        .collect();
    for b in blocks::blocks(code) {
        let Term::TrapOp(op) = b.term else { continue };
        if blocks::TRAP_TERM.contains(&op) {
            continue;
        }
        let Some(pc) = b.ops.last().map(|o| o.pc) else {
            continue;
        };
        if pc >= end {
            continue;
        }
        out.push(unsupported_view(pc, op));
    }
    out.sort_by_key(|w| w.pc.unwrap_or(0));
    out
}

fn unsupported_view(pc: usize, op: u8) -> WarningView {
    let (detail, explanation) = match fork_mnemonic(op) {
        Some(name) => (
            format!("{name} ({op:#04x}) at {pc:#06x}"),
            format!("{name} comes from a fork newer than the zkVM's EVM, which implements Shanghai: the translated program traps if this opcode executes. Compile for shanghai, or wait for the circuits pin that adds it."),
        ),
        None => (
            format!("undefined opcode {op:#04x} at {pc:#06x}"),
            "No EVM fork the zkVM implements defines this byte: the translated program traps if it executes. Code that never reaches it runs normally.".to_string(),
        ),
    };
    WarningView {
        kind: "unsupported_opcode".into(),
        pc: Some(pc),
        detail,
        explanation,
        fatal: false,
    }
}

pub fn compile_view(
    code: &[u8],
    contract: &str,
    contracts: Vec<String>,
    creation_hex: String,
    abi: serde_json::Value,
    evm_version: &str,
) -> EvmCompileView {
    let bs = blocks::blocks(code);
    let jd = blocks::jumpdests(code);
    let mut opcodes = Vec::new();
    for (bi, b) in bs.iter().enumerate() {
        for op in &b.ops {
            opcodes.push(EvmOpcode {
                pc: op.pc,
                opcode: op.opcode,
                mnemonic: mnemonic(op.opcode),
                push_hex: push_hex(op),
                static_gas: blocks::static_gas(op.opcode),
                jumpdest: op.opcode == 0x5b,
                block: bi,
            });
        }
    }
    EvmCompileView {
        contract: contract.to_string(),
        contracts,
        creation_hex,
        runtime_hex: hex::encode(code),
        size_bytes: code.len(),
        abi,
        opcodes,
        blocks: bs.len(),
        jumpdests: jd.len(),
        evm_version: evm_version.to_string(),
        warnings: zkvm_warnings(code),
    }
}

fn term_text(t: &Term) -> String {
    match t {
        Term::Fallthrough(pc) => format!("fallthrough → {pc:#06x}"),
        Term::Jump => "jump (dynamic)".into(),
        Term::JumpI(pc) => format!("jumpi, else → {pc:#06x}"),
        Term::Stop => "stop".into(),
        Term::Return => "return".into(),
        Term::Revert => "revert".into(),
        Term::Invalid => "invalid".into(),
        Term::TrapOp(op) => format!("trap {}", mnemonic(*op)),
        Term::OutOfBounds => "code too large".into(),
    }
}

/// Add the resolved-jump edges to `g`, read from the emitted C itself so they are exactly what
/// evm2rv resolved at the chosen stage: every `goto L_<n>` inside a block's region (a region opens
/// at its `/* [0xSTART, ...` head comment) is an edge to the block starting at `n`, `jump` or
/// `taken` by the block's terminator. Targets that are the block's own fallthrough/not-taken
/// successor are skipped (already drawn). The dispatch table's `case N: goto L_N` lines are not
/// inside any block region's gotos and are skipped.
fn add_goto_edges(g: &mut Graph, c: &str) {
    let mut cur: Option<usize> = None;
    let mut found: Vec<(usize, usize)> = Vec::new();
    for line in c.lines() {
        if let Some(rest) = line.split("/* [0x").nth(1) {
            cur = rest
                .split(',')
                .next()
                .and_then(|h| usize::from_str_radix(h, 16).ok())
                .and_then(|start| g.nodes.iter().position(|n| n.start == start));
            continue;
        }
        if line.trim_start().starts_with("case ") {
            continue;
        }
        let Some(from) = cur else { continue };
        let mut rest = line;
        while let Some(i) = rest.find("goto L_") {
            rest = &rest[i + 7..];
            let digits: String = rest.chars().take_while(char::is_ascii_digit).collect();
            if let Some(to) = digits
                .parse::<usize>()
                .ok()
                .and_then(|t| g.nodes.iter().position(|n| n.start == t))
            {
                found.push((from, to));
            }
        }
    }
    for (from, to) in found {
        let kind = if g.nodes[from].term.starts_with("jumpi") {
            "taken"
        } else if g.nodes[from].term.starts_with("jump") {
            "jump"
        } else {
            continue;
        };
        if g.edges.iter().any(|e| e.from == from && e.to == to) {
            continue;
        }
        g.edges.push(GraphEdge {
            from,
            to,
            kind: kind.into(),
        });
    }
}

pub fn graph(bs: &[Block]) -> Graph {
    let index_of = |pc: usize| bs.iter().position(|b| b.start == pc);
    let nodes = bs
        .iter()
        .enumerate()
        .map(|(i, b)| GraphNode {
            id: i,
            label: format!("{:#06x}", b.start),
            start: b.start,
            end: b.end,
            term: term_text(&b.term),
            function: None,
            static_gas: Some(b.static_gas),
        })
        .collect();
    let mut edges = Vec::new();
    for (i, b) in bs.iter().enumerate() {
        match b.term {
            Term::Fallthrough(pc) => {
                if let Some(to) = index_of(pc) {
                    edges.push(GraphEdge {
                        from: i,
                        to,
                        kind: "fallthrough".into(),
                    })
                }
            }
            Term::JumpI(pc) => {
                if let Some(to) = index_of(pc) {
                    edges.push(GraphEdge {
                        from: i,
                        to,
                        kind: "not-taken".into(),
                    })
                }
            }
            _ => {}
        }
    }
    Graph { nodes, edges }
}

pub fn warning_view(w: &Warning) -> WarningView {
    match *w {
        Warning::Trap { pc, opcode } => WarningView {
            kind: "trap".into(), pc: Some(pc), detail: format!("{} at {pc:#06x}", emit::mnemonic(opcode)),
            explanation: "Block-context, balance, external-code and CREATE opcodes have no meaning inside a proof: the translated program halts with a trap if this opcode executes. Code that never reaches it runs normally.".into(),
            fatal: false,
        },
        Warning::Call { pc, opcode } => WarningView {
            kind: "call".into(), pc: Some(pc), detail: format!("{} at {pc:#06x}", emit::mnemonic(opcode)),
            explanation: "Calls into other contracts are translated as calls into the precompile set only; a CALL with a nonzero value or to an unknown address traps at run time.".into(),
            fatal: false,
        },
        Warning::CodeTooLarge { len } => WarningView {
            kind: "code_too_large".into(), pc: None, detail: format!("{len} bytes of runtime code; the limit is {}", evm_core::interp::MAX_CODE_BYTES),
            explanation: "The guest's code buffer is fixed at MAX_CODE_BYTES; split the contract or strip metadata.".into(),
            fatal: true,
        },
    }
}

pub fn transpile(
    code: &[u8],
    chain_id: u64,
    stage: u8,
) -> Result<(TranspileView, String), EvmTranspileError> {
    let stage = match stage {
        1 => Stage::One,
        2 => Stage::Two,
        s => return Err(EvmTranspileError::BadStage(s)),
    };
    let warnings = zkvm_warnings(code);
    if let Some(f) = warnings.iter().find(|w| w.fatal) {
        return Err(EvmTranspileError::Unsupported(f.clone()));
    }
    let opts = Options {
        chain_id: if chain_id == 0 { None } else { Some(chain_id) },
        stage,
    };
    let emitted = emit::translate(code, &opts).map_err(|e| match e {
        EmitError::ChainIdRequired { pc } => EvmTranspileError::ChainIdRequired { pc },
    })?;
    let bs = blocks::blocks(code);
    let op_map = evm_markers(&emitted.c);
    let mut g = graph(&bs);
    add_goto_edges(&mut g, &emitted.c);
    let view = TranspileView {
        warnings,
        graph: g,
        c_source: emitted.c.clone(),
        op_map,
        stats: TranspileStats {
            blocks: emitted.blocks,
            ops: emitted.opcodes,
            locals: emitted.locals,
            c_lines: emitted.c.lines().count(),
            functions: None,
        },
    };
    Ok((view, emitted.c))
}
