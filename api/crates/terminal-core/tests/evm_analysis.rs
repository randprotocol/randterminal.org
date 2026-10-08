#[path = "fixtures.rs"]
mod fixtures;
use terminal_core::evm_analysis::{compile_view, transpile, EvmTranspileError};
use terminal_core::source_map::evm_markers;
use terminal_core::views::OpLine;

#[test]
fn erc20_compile_view_counts() {
    let code = fixtures::erc20_code();
    let v = compile_view(&code, "ERC20", vec!["ERC20".into()], String::new(), serde_json::json!([]));
    assert_eq!(v.size_bytes, 1296);
    assert_eq!(v.runtime_hex, hex::encode(&code));
    assert_eq!(v.blocks, 74);
    assert_eq!(v.jumpdests, 51);
    assert_eq!(v.opcodes.len(), 798);
    let first = &v.opcodes[0];
    assert_eq!((first.pc, first.opcode, first.mnemonic.as_str(), first.push_hex.as_deref()), (0, 0x60, "PUSH1", Some("80")));
    assert_eq!(first.static_gas, 3);
    assert!(!first.jumpdest);
    assert!(v.opcodes.iter().filter(|o| o.jumpdest).count() == 51);
    assert!(v.opcodes.windows(2).all(|w| w[0].block <= w[1].block));
}

#[test]
fn erc20_transpiles_at_stage_two() {
    let code = fixtures::erc20_code();
    let (view, c) = transpile(&code, 13, 2).unwrap();
    assert_eq!(view.stats.blocks, 74);
    assert_eq!(view.stats.ops, 798);
    assert!(view.stats.locals > 0);
    assert_eq!(view.stats.c_lines, c.lines().count());
    assert!(c.contains("/* 0x0000 PUSH1 0x80 */"));
    assert_eq!(view.graph.nodes.len(), 74);
    assert_eq!(view.graph.nodes[0].start, 0);
    assert!(view.graph.edges.iter().all(|e| e.to < 74 && e.from < 74));
    assert_eq!(view.op_map, evm_markers(&c));
    assert_eq!(view.op_map[0], OpLine { pc: 0, line: c.lines().position(|l| l.trim_start().starts_with("/* 0x0000 ")).unwrap() + 1 });
    assert!(view.op_map.len() >= 700);
    assert!(view.warnings.iter().all(|w| !w.fatal));
}

#[test]
fn chain_id_is_required_when_the_code_uses_chainid() {
    let code = vec![0x46, 0x00]; // CHAINID STOP
    match transpile(&code, 0, 2) {
        Err(EvmTranspileError::ChainIdRequired { pc }) => assert_eq!(pc, 0),
        other => panic!("{other:?}"),
    }
}

#[test]
fn transpile_with_explicit_chain_id_works_for_chainid() {
    let code = vec![0x46, 0x00];
    assert!(transpile(&code, 20, 2).is_ok());
}

#[test]
fn stage_one_is_allowed_and_three_is_not() {
    let code = fixtures::erc20_code();
    assert!(transpile(&code, 13, 1).is_ok());
    assert!(matches!(transpile(&code, 13, 3), Err(EvmTranspileError::BadStage(3))));
}

#[test]
fn trapping_opcodes_are_warnings_not_errors() {
    let code = vec![0x42, 0x00];
    let (view, _) = transpile(&code, 20, 2).unwrap();
    let w = view.warnings.iter().find(|w| w.kind == "trap").expect("a trap warning");
    assert_eq!(w.pc, Some(0));
    assert!(w.detail.contains("TIMESTAMP"));
    assert!(!w.fatal);
}

#[test]
fn too_large_code_is_fatal() {
    let code = vec![0x00; 24_577];
    match transpile(&code, 20, 2) {
        Err(EvmTranspileError::Unsupported(w)) => assert_eq!(w.kind, "code_too_large"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn markers_parse_lines_one_based() {
    let c = "int x;\n    /* 0x0000 PUSH1 0x80 */\n    /* 0x0002 PUSH1 0x40 */\n    /* 0x0004 MSTORE */ evm_mstore();\n";
    assert_eq!(evm_markers(c), vec![OpLine { pc: 0, line: 2 }, OpLine { pc: 2, line: 3 }, OpLine { pc: 4, line: 4 }]);
}

fn edges_of(code: &[u8]) -> (Vec<evm2rv::blocks::Block>, Vec<(usize, usize, String)>) {
    let bs = evm2rv::blocks::blocks(code);
    let g = terminal_core::evm_analysis::graph(&bs);
    let es = g.edges.iter().map(|e| (e.from, e.to, e.kind.clone())).collect();
    (bs, es)
}

#[test]
fn push_jump_to_a_jumpdest_is_a_jump_edge() {
    // PUSH1 4; JUMP; INVALID; JUMPDEST; STOP
    let (bs, es) = edges_of(&[0x60, 0x04, 0x56, 0xfe, 0x5b, 0x00]);
    let to = bs.iter().position(|b| b.start == 4).unwrap();
    assert!(es.contains(&(0, to, "jump".to_string())), "{es:?}");
}

#[test]
fn push_jumpi_has_taken_and_not_taken_edges() {
    // PUSH1 1; PUSH1 8; JUMPI; STOP; JUMPDEST; STOP  (JUMPI at 4, STOP at 5, JUMPDEST at 6)
    let code = [0x60, 0x01, 0x60, 0x06, 0x57, 0x00, 0x5b, 0x00];
    let (bs, es) = edges_of(&code);
    let jd = bs.iter().position(|b| b.start == 6).unwrap();
    let ft = bs.iter().position(|b| b.start == 5).unwrap();
    assert!(es.contains(&(0, jd, "taken".to_string())), "{es:?}");
    assert!(es.contains(&(0, ft, "not-taken".to_string())), "{es:?}");
}

#[test]
fn push_to_a_non_jumpdest_gets_no_edge() {
    // PUSH1 3; JUMP; INVALID; STOP  (3 is not a JUMPDEST)
    let (_, es) = edges_of(&[0x60, 0x03, 0x56, 0xfe, 0x00]);
    assert!(es.iter().all(|e| e.2 != "jump" && e.2 != "taken"), "{es:?}");
}

#[test]
fn dynamic_jump_gets_no_edge() {
    // CALLDATASIZE; JUMP; JUMPDEST; STOP
    let (_, es) = edges_of(&[0x36, 0x56, 0x5b, 0x00]);
    assert!(es.iter().all(|e| e.2 != "jump"), "{es:?}");
}

#[test]
fn erc20_jump_edges_match_the_gotos_in_the_c() {
    use std::collections::BTreeSet;
    let code = fixtures::erc20_code();
    let (view, c) = transpile(&code, 13, 2).unwrap();
    let g = &view.graph;
    let n = g.nodes.len();
    assert!(g.edges.iter().all(|e| e.to < n && e.from < n));
    let ours: BTreeSet<(usize, usize)> = g.edges.iter().filter(|e| e.kind == "jump" || e.kind == "taken")
        .map(|e| (g.nodes[e.from].start, g.nodes[e.to].start)).collect();
    assert!(!ours.is_empty());
    // The C: each block opens with `/* [0xSTART, ...` and its resolved jumps are `goto L_<n>;`.
    let mut theirs = BTreeSet::new();
    let mut cur = None;
    for line in c.lines() {
        if let Some(rest) = line.trim_start().strip_prefix("/* [0x").or_else(|| line.split("/* [0x").nth(1)) {
            if let Some(h) = rest.split(',').next() { cur = usize::from_str_radix(h, 16).ok(); }
            continue;
        }
        if line.contains("case ") { continue; }
        if let Some(from) = cur {
            let mut rest = line;
            while let Some(i) = rest.find("goto L_") {
                let t: String = rest[i + 7..].chars().take_while(|ch| ch.is_ascii_digit()).collect();
                theirs.insert((from, t.parse::<usize>().unwrap()));
                rest = &rest[i + 7..];
            }
        }
    }
    // evm_entry's `goto L_0` precedes any block header, so cur is None there.
    assert_eq!(ours, theirs);
}

#[test]
fn a_destination_moved_by_swap_still_resolves() {
    // PUSH1 7; PUSH1 1; SWAP1; JUMP; INVALID; JUMPDEST; STOP  (stage two folds this to a goto)
    let (bs, es) = edges_of(&[0x60, 0x07, 0x60, 0x01, 0x90, 0x56, 0xfe, 0x5b, 0x00]);
    let to = bs.iter().position(|b| b.start == 7).unwrap();
    assert!(es.contains(&(0, to, "jump".to_string())), "{es:?}");
}
