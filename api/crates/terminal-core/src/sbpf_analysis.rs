//! The Solana side of stages 1 and 2: the ELF as the transpiler scans it, and the C it emits.
use crate::sbpf_disasm::{syscall_name, text};
use crate::source_map::sbpf_labels;
use crate::views::*;
use sbpf2rv::scan::{self, Scan, Term, Warning};
use sbpf2rv::{emit, shim};
use sbpf_core::elf::Program;
use sbpf_core::isa;
use std::collections::BTreeMap;

#[derive(Debug, thiserror::Error)]
pub enum SbpfError {
    #[error("not a loadable sBPF ELF: {0}")]
    BadElf(String),
    #[error("the program is too large to scan: {0}")]
    TooMuchWork(String),
    #[error("unsupported: {}", .0.detail)]
    Unsupported(WarningView),
}

#[derive(Debug)]
pub struct Analysis { pub compile: SbpfCompileView, pub transpile: TranspileView, pub program_c: String, pub digest: [u32; 8] }

fn term_text(t: &Term) -> String {
    match t {
        Term::Fallthrough(n) => format!("fallthrough → {n}"),
        Term::Jump(n) => format!("ja → {n}"),
        Term::CondJump { taken, not } => format!("branch → {taken} / {not}"),
        Term::Exit => "exit".into(),
        Term::Call { target, next } => format!("call {target}, then {next}"),
        Term::Syscall { hash, next } => format!("syscall {}, then {next}", syscall_name(*hash).unwrap_or("?")),
        Term::CallX { next } => format!("callx, then {next}"),
        Term::Trap(k) => format!("trap {k:?}"),
    }
}

pub fn warning_view(w: &Warning) -> WarningView {
    let (kind, pc, detail, explanation, fatal) = match *w {
        Warning::UnknownSyscall { pc, hash } => ("unknown_syscall", pc, format!("syscall {hash:#010x} at {pc}"), "Only twelve Solana syscalls exist in the guest runtime; this one traps if executed.", false),
        Warning::Cpi { pc, name } => ("cpi", pc, format!("{name} at {pc}"), "Cross-program invocation is not available inside a proof; the program traps if it reaches this call.", false),
        Warning::RegisterOutOfRange { pc, opc } => ("register_out_of_range", pc, format!("opcode {opc:#04x} at {pc}"), "The instruction names a register above r10; the verifier would reject it and the guest traps.", false),
        Warning::UnknownOpcode { pc, opc } => ("unknown_opcode", pc, format!("opcode {opc:#04x} at {pc}"), "Not an sBPF v1 instruction; it traps if executed.", false),
        Warning::JumpOutOfText { pc, target } => ("jump_out_of_text", pc, format!("target {target} from {pc}"), "A jump past the text section traps.", false),
        Warning::BadCallImmSrc { pc, src } => ("bad_call", pc, format!("call with src {src} at {pc}"), "The call form is not one the translator knows; it traps.", false),
    };
    WarningView { kind: kind.into(), pc: Some(pc), detail, explanation: explanation.into(), fatal }
}

/// One entry per logical instruction: an `lddw` is one entry whose upper half is read from the
/// text at the next slot, and a trailing bad instruction the scanner left out is simply absent.
fn insn_views(p: &Program<'_>, b: &scan::Block) -> Vec<SbpfInsnView> {
    let slots = p.text.len() / 8;
    let mut pc = b.start;
    b.insns.iter().map(|i| {
        let hi = (i.opc == 0x18 && pc + 1 < slots).then(|| {
            let w = &p.text[(pc + 1) * 8..(pc + 2) * 8];
            isa::decode(u64::from_le_bytes(w.try_into().unwrap()))
        });
        let v = SbpfInsnView { pc, text: text(pc, i, hi.as_ref()), opc: i.opc };
        pc += if i.opc == 0x18 { 2 } else { 1 };
        v
    }).collect()
}

fn compile_view(elf_len: usize, p: &Program<'_>, s: &Scan) -> SbpfCompileView {
    let mut sys: BTreeMap<u32, usize> = BTreeMap::new();
    let mut instructions = 0;
    let functions = s.functions.iter().map(|f| SbpfFunctionView {
        entry: f.entry,
        blocks: f.blocks.iter().map(|b| {
            if let Term::Syscall { hash, .. } = b.term { *sys.entry(hash).or_default() += 1; }
            instructions += b.insns.len();
            SbpfBlockView { start: b.start, end: b.end, term: term_text(&b.term), insns: insn_views(p, b) }
        }).collect(),
    }).collect();
    SbpfCompileView {
        elf_bytes: elf_len, text_bytes: p.text.len(), rodata_bytes: p.rodata.len(), entry_pc: p.entry_pc,
        functions, instructions,
        syscalls: sys.into_iter().map(|(hash, count)| SbpfSyscallView { hash, name: syscall_name(hash).map(str::to_string).unwrap_or_else(|| format!("unknown {hash:#010x}")), count, supported: syscall_name(hash).is_some() }).collect(),
        warnings: s.warnings.iter().map(warning_view).collect(),
    }
}

fn graph(s: &Scan) -> Graph {
    // A block body shared by two functions is a node in each, so ids are per function.
    let mut nodes = Vec::new();
    let mut local: Vec<BTreeMap<usize, usize>> = Vec::new(); // per function: block start -> node id
    for (fi, f) in s.functions.iter().enumerate() {
        let mut m = BTreeMap::new();
        for b in &f.blocks {
            m.insert(b.start, nodes.len());
            nodes.push(GraphNode { id: nodes.len(), label: format!("{}", b.start), start: b.start, end: b.end, term: term_text(&b.term), function: Some(fi), static_gas: None });
        }
        local.push(m);
    }
    let entry_node: BTreeMap<usize, usize> = s.functions.iter().enumerate().filter_map(|(fi, f)| local[fi].get(&f.entry).map(|&n| (f.entry, n))).collect();
    let mut edges = Vec::new();
    for (fi, f) in s.functions.iter().enumerate() {
        for b in &f.blocks {
            let from = local[fi][&b.start];
            let mut e = |to: Option<&usize>, kind: &str| if let Some(&t) = to { edges.push(GraphEdge { from, to: t, kind: kind.into() }) };
            match b.term {
                Term::Fallthrough(n) => e(local[fi].get(&n), "fallthrough"),
                Term::Jump(n) => e(local[fi].get(&n), "jump"),
                Term::CondJump { taken, not } => { e(local[fi].get(&taken), "taken"); e(local[fi].get(&not), "not-taken"); }
                Term::Call { target, next } => { e(entry_node.get(&target), "call"); e(local[fi].get(&next), "return"); }
                Term::Syscall { next, .. } | Term::CallX { next } => e(local[fi].get(&next), "fallthrough"),
                Term::Exit | Term::Trap(_) => {}
            }
        }
    }
    Graph { nodes, edges }
}

pub fn analyse(elf: &[u8]) -> Result<Analysis, SbpfError> {
    let mut bytes = elf.to_vec();
    let p = sbpf_core::elf::load(&mut bytes).map_err(|h| SbpfError::BadElf(format!("{h:?}")))?;
    let s = scan::try_scan(&p).map_err(|e| SbpfError::TooMuchWork(e.to_string()))?;
    let compile = compile_view(elf.len(), &p, &s);
    let emitted = emit::emit_program(&p, &s);
    let digest = shim::view_digest(&p);
    let program_c = format!("{}{}", emitted.c, shim::view_digest_c(&digest));
    let op_map = sbpf_labels(&program_c);
    let transpile = TranspileView {
        warnings: compile.warnings.clone(), graph: graph(&s), c_source: program_c.clone(), op_map,
        stats: TranspileStats { blocks: emitted.blocks, ops: emitted.instructions, locals: 0, c_lines: program_c.lines().count(), functions: Some(emitted.functions) },
    };
    Ok(Analysis { compile, transpile, program_c, digest })
}
