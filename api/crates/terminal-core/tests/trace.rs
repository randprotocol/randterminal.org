#[path = "fixtures.rs"]
mod fixtures;
use rand_zkvm::emulator::{execute, Execution, MemAccess, SPACE_RAM, SPACE_REG};
use rand_zkvm::isa::Program;
use terminal_core::trace::{self, Trace, MEM_CKPT, REG_CKPT};

fn all_accesses(e: &rand_zkvm::emulator::CycleEvent) -> impl Iterator<Item = &MemAccess> {
    e.accesses
        .iter()
        .chain(e.keccak_accesses.iter())
        .chain(e.sha256_accesses.iter())
}

/// The final register file and RAM, folded straight from the events' writes.
fn fold(
    events: &[rand_zkvm::emulator::CycleEvent],
) -> ([u32; 32], std::collections::BTreeMap<u32, u32>) {
    let mut regs = [0u32; 32];
    let mut mem = std::collections::BTreeMap::new();
    for e in events {
        for a in all_accesses(e).filter(|a| a.is_write) {
            if a.space == SPACE_REG {
                if a.addr != 0 {
                    regs[a.addr as usize] = a.value;
                }
            } else if a.space == SPACE_RAM {
                mem.insert(a.addr, a.value);
            }
        }
    }
    (regs, mem)
}

/// `memory_at(total, ..)` over the span of written addresses (capped to one window) equals the fold.
fn check_final(t: &Trace, events: &[rand_zkvm::emulator::CycleEvent]) {
    let (regs, mem) = fold(events);
    assert_eq!(t.regs_at(t.total).unwrap(), regs);
    let base = mem.keys().next().copied().unwrap_or(0);
    let got = t.memory_at(t.total, base, 256).unwrap();
    for (i, w) in got.iter().enumerate() {
        assert_eq!(
            *w,
            mem.get(&(base + i as u32)).copied().unwrap_or(0),
            "final mem {:#x}",
            base + i as u32
        );
    }
}

/// Every recorded register read equals the replayed register file before that cycle, and every
/// recorded RAM read equals the replayed memory before that cycle.
fn check_replay(t: &Trace, exec: &Execution, clks: impl Iterator<Item = u32>) {
    for clk in clks {
        let e = &exec.events[clk as usize];
        let regs = t.regs_at(clk).unwrap();
        for a in all_accesses(e).filter(|a| a.space == SPACE_REG && !a.is_write) {
            assert_eq!(regs[a.addr as usize], a.value, "clk {clk} reg x{}", a.addr);
        }
        for a in all_accesses(e).filter(|a| a.space == SPACE_RAM && !a.is_write) {
            let w = t.memory_at(clk, a.addr, 1).unwrap();
            assert_eq!(w[0], a.value, "clk {clk} mem {:#x}", a.addr);
        }
        let c = t.cycle(clk).unwrap();
        assert_eq!(
            (c.clk, c.pc, c.next_pc, c.word),
            (e.clk, e.pc, e.next_pc, e.instr.encode())
        );
    }
}

#[test]
fn hand_assembled_guests_round_trip() {
    let mut ok = 0;
    for (name, program, inputs) in rand_zkvm::guests::all() {
        let Ok(exec) = execute(&program, &inputs, &[], 1 << 16) else {
            continue;
        };
        ok += 1;
        let dir = tempfile::tempdir().unwrap();
        let total = trace::write(dir.path(), &exec).unwrap();
        assert_eq!(total as usize, exec.cycles(), "{name}");
        let t = Trace::open(dir.path()).unwrap();
        assert_eq!(t.total, total);
        check_replay(&t, &exec, 0..total);
        assert_eq!(t.regs_at(0).unwrap(), [0u32; 32], "{name}");
        check_final(&t, &exec.events);
        assert!(t.regs_at(total + 1).is_err());
        assert!(t.memory_at(total + 1, 0, 1).is_err());
        let page = t.cycles(0, 4096).unwrap();
        assert_eq!(page.len(), (total.min(4096)) as usize);
        assert!(t.cycles(total, 10).unwrap().is_empty());
        let sys = t.syscalls().unwrap();
        assert_eq!(
            sys.len(),
            exec.events.iter().filter(|e| e.sys.is_some()).count()
        );
        assert!(
            sys.last().map(|s| s.kind == "halt").unwrap_or(true),
            "{name}"
        );
    }
    assert!(ok >= 5, "only {ok} guests ran");
}

#[test]
fn erc20_transfer_crosses_both_checkpoint_sizes() {
    let program = Program::from_flat_image(&fixtures::bytes("erc20.image.bin")).unwrap();
    let inputs = fixtures::words("erc20-transfer.words");
    let exec = execute(&program, &inputs, &[], 1 << 18).unwrap();
    assert_eq!(exec.cycles(), 66_235);
    assert!(exec.cycles() as u32 > MEM_CKPT && exec.cycles() as u32 > REG_CKPT);
    let dir = tempfile::tempdir().unwrap();
    let total = trace::write(dir.path(), &exec).unwrap();
    let t = Trace::open(dir.path()).unwrap();
    let samples = (0..total).step_by(997).chain([
        REG_CKPT - 1,
        REG_CKPT,
        REG_CKPT + 1,
        MEM_CKPT - 1,
        MEM_CKPT,
        MEM_CKPT + 1,
        total - 1,
    ]);
    check_replay(&t, &exec, samples);
    // a window straddles the sparse map and the write log
    let w = t.memory_at(total, 0x4000, 256).unwrap();
    assert_eq!(w.len(), 256);
    check_final(&t, &exec.events);
    // the cycle records flag what they should
    let halt = t.cycle(total - 1).unwrap();
    assert!(halt.syscall);
    let page = t.cycles(total - 5, 100).unwrap();
    assert_eq!(page.len(), 5);
    assert_eq!(page[4].clk, total - 1);
}

#[test]
fn page_and_window_limits() {
    let (_, program, inputs) = rand_zkvm::guests::all().into_iter().next().unwrap();
    let exec = execute(&program, &inputs, &[], 1 << 16).unwrap();
    let dir = tempfile::tempdir().unwrap();
    trace::write(dir.path(), &exec).unwrap();
    let t = Trace::open(dir.path()).unwrap();
    assert!(t.cycles(0, 4097).is_err());
    assert!(t.memory_at(0, 0, 257).is_err());
}

#[test]
fn checkpoint_owed_at_exact_multiples_and_empty() {
    let (_, program, inputs) = rand_zkvm::guests::all()
        .into_iter()
        .find(|(n, ..)| *n == "fib(20)")
        .unwrap();
    let mut exec = execute(&program, &inputs, &[], 1 << 16).unwrap();
    // loop the events so there are at least 1024 of them, then truncate to exactly 1024
    let base = exec.events.clone();
    while exec.events.len() < 1024 {
        exec.events.extend(base.iter().cloned());
    }
    exec.events.truncate(1024);
    for (i, e) in exec.events.iter_mut().enumerate() {
        e.clk = i as u32;
    }
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(trace::write(dir.path(), &exec).unwrap(), 1024);
    let t = Trace::open(dir.path()).unwrap();
    check_final(&t, &exec.events);
    assert!(t.regs_at(1025).is_err());

    exec.events.clear();
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(trace::write(dir.path(), &exec).unwrap(), 0);
    let t = Trace::open(dir.path()).unwrap();
    assert_eq!(t.regs_at(0).unwrap(), [0u32; 32]);
    assert_eq!(t.memory_at(0, 0, 4).unwrap(), vec![0; 4]);
    assert!(t.cycles(0, 10).unwrap().is_empty());
}

#[test]
fn incomplete_trace_is_rejected() {
    let (_, program, inputs) = rand_zkvm::guests::all().into_iter().next().unwrap();
    let exec = execute(&program, &inputs, &[], 1 << 16).unwrap();
    let dir = tempfile::tempdir().unwrap();
    trace::write(dir.path(), &exec).unwrap();
    std::fs::remove_file(dir.path().join("syscalls.json")).unwrap();
    assert_eq!(
        Trace::open(dir.path()).err().unwrap().kind(),
        std::io::ErrorKind::InvalidData
    );
}

/// I6: a store's cycle record carries the word it wrote (`mem_val`) and the word it replaced
/// (`mem_old`); the emulator's own `mem_val` is the old word for stores. Loads have
/// `mem_old == mem_val` (the word read).
#[test]
fn stores_record_the_written_word_and_the_old_one() {
    let mut stores = 0;
    for (_, program, inputs) in rand_zkvm::guests::all() {
        let Ok(exec) = execute(&program, &inputs, &[], 1 << 16) else {
            continue;
        };
        let dir = tempfile::tempdir().unwrap();
        trace::write(dir.path(), &exec).unwrap();
        let t = Trace::open(dir.path()).unwrap();
        for e in &exec.events {
            let c = t.cycle(e.clk).unwrap();
            if c.mem_write {
                let w = e
                    .accesses
                    .iter()
                    .find(|a| a.space == SPACE_RAM && a.is_write)
                    .unwrap();
                assert_eq!((c.mem_addr, c.mem_val, c.mem_old), (w.addr, w.value, e.mem_val));
                // The new word is what memory holds after the cycle.
                assert_eq!(t.memory_at(e.clk + 1, c.mem_addr, 1).unwrap()[0], c.mem_val);
                assert_eq!(t.memory_at(e.clk, c.mem_addr, 1).unwrap()[0], c.mem_old);
                stores += 1;
            } else if c.mem_read {
                assert_eq!((c.mem_val, c.mem_old), (e.mem_val, e.mem_val));
            }
        }
    }
    assert!(stores > 0, "the guests store something");
}
