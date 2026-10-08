//! The EVM side of stages 1 and 2: what the bytecode looks like to the transpiler, and what the
//! transpiler makes of it.
use crate::source_map::evm_markers;
use crate::views::*;
use evm2rv::blocks::{self, Block, Term, Warning};
use evm_core::u256::U256;
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

pub fn compile_view(code: &[u8], contract: &str, contracts: Vec<String>, creation_hex: String, abi: serde_json::Value) -> EvmCompileView {
    let bs = blocks::blocks(code);
    let jd = blocks::jumpdests(code);
    let mut opcodes = Vec::new();
    for (bi, b) in bs.iter().enumerate() {
        for op in &b.ops {
            opcodes.push(EvmOpcode {
                pc: op.pc, opcode: op.opcode, mnemonic: emit::mnemonic(op.opcode), push_hex: push_hex(op),
                static_gas: blocks::static_gas(op.opcode), jumpdest: op.opcode == 0x5b, block: bi,
            });
        }
    }
    EvmCompileView {
        contract: contract.to_string(), contracts, creation_hex, runtime_hex: hex::encode(code), size_bytes: code.len(),
        abi, opcodes, blocks: bs.len(), jumpdests: jd.len(),
    }
}

fn term_text(t: &Term) -> String {
    match t {
        Term::Fallthrough(pc) => format!("fallthrough → {pc:#06x}"),
        Term::Jump => "jump (dynamic)".into(),
        Term::JumpI(pc) => format!("jumpi, else → {pc:#06x}"),
        Term::Stop => "stop".into(), Term::Return => "return".into(), Term::Revert => "revert".into(),
        Term::Invalid => "invalid".into(), Term::TrapOp(op) => format!("trap {}", emit::mnemonic(*op)),
        Term::OutOfBounds => "code too large".into(),
    }
}

/// The constant a block's final JUMP/JUMPI destination folds to, by the same rule as evm2rv's stage
/// two (`lift.rs`): the block is run symbolically from an unknown entry stack; PUSH, PC, DUPn,
/// SWAPn, POP and the pure arithmetic/comparison/bitwise ops over constants keep known values, every
/// other op leaves unknowns. CHAINID is not folded here (the graph has no chain id), so a jump
/// through it shows no edge. The caller checks that the value names a JUMPDEST block.
fn const_jump_dest(b: &Block) -> Option<usize> {
    let (_, body) = b.ops.split_last()?;
    let mut st: Vec<Option<U256>> = Vec::new(); // top is last; missing entries are the unknown entry stack
    let pop = |st: &mut Vec<Option<U256>>| st.pop().flatten();
    for op in body {
        let o = op.opcode;
        if let Some(v) = op.push { st.push(Some(U256::from_be_bytes(&v))); continue; }
        match o {
            0x58 => st.push(Some(U256::from_u64(op.pc as u64))),
            0x50 => { pop(&mut st); }
            0x80..=0x8f => {
                let n = (o - 0x7f) as usize;
                while st.len() < n { st.insert(0, None); }
                st.push(st[st.len() - n]);
            }
            0x90..=0x9f => {
                let n = (o - 0x8f) as usize;
                while st.len() <= n { st.insert(0, None); }
                let top = st.len() - 1;
                st.swap(top, top - n);
            }
            _ => {
                let (pops, delta) = blocks::stack_effect(o);
                let args: Vec<Option<U256>> = (0..pops).map(|_| pop(&mut st)).collect();
                let folded = args.iter().copied().collect::<Option<Vec<U256>>>().and_then(|v| fold(o, &v));
                let pushes = (pops as i32 + delta).max(0) as usize;
                for k in 0..pushes { st.push(if k == 0 { folded } else { None }); }
            }
        }
    }
    let d = st.pop().flatten()?;
    (d.0[1..].iter().all(|&x| x == 0)).then_some(d.0[0] as usize)
}

/// `op` over constant operands (`v[0]` the top): evm2rv's `fold`, with the interpreter's arithmetic.
fn fold(op: u8, v: &[U256]) -> Option<U256> {
    let b = |x: bool| if x { U256::ONE } else { U256::ZERO };
    Some(match op {
        0x01 => v[0].add(&v[1]), 0x02 => v[0].mul(&v[1]), 0x03 => v[0].sub(&v[1]), 0x04 => v[0].div(&v[1]),
        0x05 => v[0].sdiv(&v[1]), 0x06 => v[0].rem(&v[1]), 0x07 => v[0].smod(&v[1]),
        0x08 => v[0].addmod(&v[1], &v[2]), 0x09 => v[0].mulmod(&v[1], &v[2]), 0x0b => v[1].signextend(&v[0]),
        0x10 => b(v[0].lt(&v[1])), 0x11 => b(v[1].lt(&v[0])), 0x12 => b(v[0].slt(&v[1])), 0x13 => b(v[1].slt(&v[0])),
        0x14 => b(v[0] == v[1]), 0x15 => b(v[0].is_zero()),
        0x16 => v[0].and(&v[1]), 0x17 => v[0].or(&v[1]), 0x18 => v[0].xor(&v[1]), 0x19 => v[0].not(),
        0x1a => v[1].byte(&v[0]), 0x1b => v[1].shl(&v[0]), 0x1c => v[1].shr(&v[0]), 0x1d => v[1].sar(&v[0]),
        _ => return None,
    })
}

pub fn graph(bs: &[Block]) -> Graph {
    let index_of = |pc: usize| bs.iter().position(|b| b.start == pc);
    let nodes = bs.iter().enumerate().map(|(i, b)| GraphNode {
        id: i, label: format!("{:#06x}", b.start), start: b.start, end: b.end, term: term_text(&b.term), function: None, static_gas: Some(b.static_gas),
    }).collect();
    let static_target = |b: &Block| -> Option<usize> {
        let t = const_jump_dest(b)?;
        bs.iter().position(|d| d.start == t && d.ops.first().is_some_and(|o| o.opcode == 0x5b))
    };
    let mut edges = Vec::new();
    for (i, b) in bs.iter().enumerate() {
        match b.term {
            Term::Jump => if let Some(to) = static_target(b) { edges.push(GraphEdge { from: i, to, kind: "jump".into() }) },
            Term::Fallthrough(pc) => if let Some(to) = index_of(pc) { edges.push(GraphEdge { from: i, to, kind: "fallthrough".into() }) },
            Term::JumpI(pc) => {
                if let Some(to) = static_target(b) { edges.push(GraphEdge { from: i, to, kind: "taken".into() }) }
                if let Some(to) = index_of(pc) { edges.push(GraphEdge { from: i, to, kind: "not-taken".into() }) }
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

pub fn transpile(code: &[u8], chain_id: u64, stage: u8) -> Result<(TranspileView, String), EvmTranspileError> {
    let stage = match stage { 1 => Stage::One, 2 => Stage::Two, s => return Err(EvmTranspileError::BadStage(s)) };
    let warnings: Vec<WarningView> = blocks::warnings(code).iter().map(warning_view).collect();
    if let Some(f) = warnings.iter().find(|w| w.fatal) { return Err(EvmTranspileError::Unsupported(f.clone())); }
    let opts = Options { chain_id: if chain_id == 0 { None } else { Some(chain_id) }, stage };
    let emitted = emit::translate(code, &opts).map_err(|e| match e { EmitError::ChainIdRequired { pc } => EvmTranspileError::ChainIdRequired { pc } })?;
    let bs = blocks::blocks(code);
    let op_map = evm_markers(&emitted.c);
    let view = TranspileView {
        warnings, graph: graph(&bs), c_source: emitted.c.clone(), op_map,
        stats: TranspileStats { blocks: emitted.blocks, ops: emitted.opcodes, locals: emitted.locals, c_lines: emitted.c.lines().count(), functions: None },
    };
    Ok((view, emitted.c))
}
