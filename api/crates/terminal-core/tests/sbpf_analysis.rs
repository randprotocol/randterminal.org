#[path = "fixtures.rs"]
mod fixtures;
use terminal_core::sbpf_analysis::{analyse, SbpfError};
use terminal_core::source_map::sbpf_labels;

#[test]
fn spl_token_analyses() {
    let elf = fixtures::bytes("spl_token.so");
    let a = analyse(&elf).unwrap();
    let c = &a.compile;
    assert_eq!(c.elf_bytes, elf.len());
    assert!(c.text_bytes > 100_000 && c.text_bytes.is_multiple_of(8));
    assert!(c.functions.len() > 20, "{}", c.functions.len());
    assert!(c.instructions > 10_000);
    // Every syscall SPL Token calls is one the guest runtime implements since constraint set 11
    // (circuits ZKVM-116 added sol_set_return_data and sol_get_sysvar, its last two, 4 call sites):
    // no unknown_syscall warning, and each one is named.
    let unsupported: usize = c
        .syscalls
        .iter()
        .filter(|s| !s.supported)
        .map(|s| s.count)
        .sum();
    assert_eq!(
        unsupported,
        c.warnings
            .iter()
            .filter(|w| w.kind == "unknown_syscall")
            .count()
    );
    assert_eq!(c.syscalls.iter().filter(|s| !s.supported).count(), 0);
    for name in ["sol_set_return_data", "sol_get_sysvar"] {
        assert!(c.syscalls.iter().any(|s| s.name == name), "{name}");
    }
    assert!(c.syscalls.iter().any(|s| s.name == "sol_log_"));
    assert!(c.warnings.iter().all(|w| !w.fatal));
    // the entry function comes first and starts at the ELF's entry pc
    assert_eq!(c.functions[0].entry, c.entry_pc);
    assert_eq!(c.functions[0].blocks[0].start, c.entry_pc);
    // every instruction has text and the first block's first pc is its start
    for f in &c.functions {
        for b in &f.blocks {
            assert_eq!(b.insns[0].pc, b.start);
            assert!(b.insns.iter().all(|i| !i.text.is_empty()));
        }
    }

    let t = &a.transpile;
    assert_eq!(t.stats.functions, Some(c.functions.len()));
    assert_eq!(
        t.graph.nodes.len(),
        c.functions.iter().map(|f| f.blocks.len()).sum::<usize>()
    );
    assert!(t.graph.edges.iter().all(|e| e.to < t.graph.nodes.len()));
    assert!(t.graph.nodes.iter().all(|n| n.function.is_some()));
    assert_eq!(t.op_map, sbpf_labels(&t.c_source));
    assert!(t.op_map.len() >= 1000);
    let mut elf = fixtures::bytes("spl_token.so");
    let p = sbpf_core::elf::load(&mut elf).unwrap();
    assert!(a.program_c.ends_with(&sbpf2rv::shim::baked_c(&p, false)));
    assert!(t.c_source.contains("sbpf_entry"));
}

#[test]
fn garbage_is_a_bad_elf() {
    match analyse(&[0u8; 64]) {
        Err(SbpfError::BadElf(_)) => {}
        other => panic!("{other:?}"),
    }
}

#[test]
fn labels_parse() {
    let c = "void f(void) {\nL_225:\n  x();\n    L_227:\n}\n";
    let l = sbpf_labels(c);
    assert_eq!(l.len(), 2);
    assert_eq!((l[0].pc, l[0].line), (225, 2));
    assert_eq!((l[1].pc, l[1].line), (227, 4));
}
