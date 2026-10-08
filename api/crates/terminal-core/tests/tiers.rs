#[path = "fixtures.rs"]
mod fixtures;
use rand_zkvm::emulator::{execute, ExecError};
use rand_zkvm::isa::Program;
use terminal_core::tiers::{capacities, cycles_total, pick, rows};
use terminal_core::trace::{summarize, trapped};

#[test]
fn erc20_transfer_lands_in_tier_18_like_rand_guest_run() {
    let program = Program::from_flat_image(&fixtures::bytes("erc20.image.bin")).unwrap();
    let inputs = fixtures::words("erc20-transfer.words");
    let exec = execute(&program, &inputs, &[], 1 << 20).unwrap();
    let r = rows(&program, inputs.len(), 0, &exec);
    assert_eq!(r.cpu, 66_235);
    assert_eq!(r.digest_rows, program.digest_rows() + rand_zkvm::hash::input_digest_row_count(inputs.len()) + rand_zkvm::hash::public_digest_row_count(0));
    assert!(r.memory > r.cpu, "every cycle has at least one access");
    assert!(r.alu > 0 && r.poseidon2_permutations >= r.digest_rows);
    assert_eq!(pick(&r, cycles_total(&r)), Some(18), "run-transfer.txt says tier 18");
    let caps = capacities(&r, cycles_total(&r));
    assert_eq!(caps.iter().map(|c| c.tier).collect::<Vec<_>>(), vec![10, 12, 14, 16, 18, 20]);
    assert_eq!(caps.iter().map(|c| c.fits).collect::<Vec<_>>(), vec![false, false, false, false, true, true]);
    let t18 = &caps[4];
    assert_eq!(t18.cpu, 1 << 18);
    assert_eq!(t18.alu, 1 << 19);
    assert_eq!(t18.poseidon2_permutations, (1 << 20) / 32);
    assert_eq!(t18.memory_min, 1 << 20);
    assert_eq!(t18.memory_max, 1 << 24);
}

#[test]
fn spl_transfer_lands_in_tier_20() {
    let program = Program::from_flat_image(&fixtures::bytes("spl.image.bin")).unwrap();
    let public = fixtures::words("spl-transfer.public");
    let inputs = fixtures::words("spl-transfer.input");
    let exec = execute(&program, &inputs, &public, 1 << 20).unwrap();
    let r = rows(&program, inputs.len(), public.len(), &exec);
    assert_eq!(r.cpu, 765_851);
    assert_eq!(pick(&r, cycles_total(&r)), Some(20));
    let s = summarize("r1", &program, inputs.len(), public.len(), &exec, vec![]);
    assert_eq!(s.cycles, 765_851);
    assert_eq!(s.tier, Some(20));
    assert_eq!(s.status, exec.outputs[0]);
    assert_eq!(s.outputs, exec.outputs);
    assert!(s.halted);
    assert_eq!(s.hot_pcs.len(), 32);
    assert!(s.hot_pcs.windows(2).all(|w| w[0].1 >= w[1].1));
    assert!(s.syscall_counts.iter().any(|(k, _)| k == "read_public"));
}

#[test]
fn trapped_summary_names_the_error() {
    let s = trapped("r2", &ExecError::OutOfCycles(1023), 3, 0, vec![]);
    assert_eq!(s.cycles, 0);
    assert!(!s.halted);
    assert_eq!(s.trap.as_deref(), Some("OutOfCycles(1023)"));
    assert_eq!(s.trap_clk, Some(1023));
    assert_eq!(s.tier, None);
    let s = trapped("r3", &ExecError::BadPc(0x10), 3, 0, vec![]);
    assert_eq!(s.trap_clk, None);
}
