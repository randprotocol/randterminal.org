//! The compact on-disk trace. `rand_zkvm`'s `Execution` holds a `Vec` per cycle (hundreds of MB
//! for SPL Token's 766 k cycles), so a run is folded once into fixed-width files the debugger
//! pages through:
//!
//! * `cycles.bin`     36 B per cycle: clk, pc, next_pc, word, rd, flags, n_alu, rd_val, mem_addr, mem_val, sys_idx
//! * `regwrites.bin`  12 B per register write: clk, reg, value (x0 never recorded)
//! * `memwrites.bin`  12 B per RAM write: clk, word address, value
//! * `regs.ckpt`      32 × u32 every REG_CKPT cycles: the file before that cycle
//! * `memckpt.bin`    every MEM_CKPT cycles: u32 n, then n × (addr, value); `memckpt.idx` is u64 offsets
//! * `syscalls.json`  Vec<SyscallView>
//!
//! All integers little-endian. State at `clk` is the state before cycle `clk` runs.
use crate::views::{CycleView, SyscallView};
use rand_zkvm::emulator::{Execution, MemAccess, Syscall, SPACE_RAM, SPACE_REG};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};

pub const REG_CKPT: u32 = 1024;
pub const MEM_CKPT: u32 = 65_536;
pub const REC_BYTES: u64 = 36;
pub const LOG_BYTES: u64 = 12;
pub const MAX_PAGE: u32 = 4096;
pub const MAX_WINDOW: u32 = 256;

const F_WRITES_RD: u8 = 1;
const F_MEM_READ: u8 = 2;
const F_MEM_WRITE: u8 = 4;
const F_SYSCALL: u8 = 8;
const F_HASH_ROW: u8 = 16;

pub fn syscall_view(clk: u32, s: &Syscall) -> SyscallView {
    let (kind, detail) = match *s {
        Syscall::Halt => ("halt", String::new()),
        Syscall::WriteOutput { slot, word } => ("write_output", format!("out[{slot}] = {word}")),
        Syscall::ReadInput { idx, word } => ("read_input", format!("in[{idx}] → {word}")),
        Syscall::ReadPublic { idx, word } => ("read_public", format!("pub[{idx}] → {word}")),
        Syscall::Poseidon2 { ptr, n } => ("poseidon2", format!("{n} words at {ptr:#x}")),
        Syscall::Poseidon2Len { ptr, n } => ("poseidon2_len", format!("{n} words at {ptr:#x}")),
        Syscall::Keccak { ptr } => ("keccak", format!("state at {ptr:#x}")),
        Syscall::Sha256 { ptr } => ("sha256", format!("block at {ptr:#x}")),
    };
    SyscallView { clk, kind: kind.into(), detail }
}

fn all_accesses(e: &rand_zkvm::emulator::CycleEvent) -> impl Iterator<Item = &MemAccess> {
    e.accesses.iter().chain(e.keccak_accesses.iter()).chain(e.sha256_accesses.iter())
}

fn put(w: &mut impl Write, v: u32) -> io::Result<()> { w.write_all(&v.to_le_bytes()) }

fn write_reg_ckpt(w: &mut impl Write, regs: &[u32; 32]) -> io::Result<()> {
    for &r in regs { put(w, r)?; }
    Ok(())
}

fn write_mem_ckpt(w: &mut impl Write, mem: &BTreeMap<u32, u32>, idx: &mut Vec<u64>, off: &mut u64) -> io::Result<()> {
    idx.push(*off);
    put(w, mem.len() as u32)?;
    for (&a, &v) in mem { put(w, a)?; put(w, v)?; }
    *off += 4 + 8 * mem.len() as u64;
    Ok(())
}

pub fn write(dir: &Path, exec: &Execution) -> io::Result<u32> {
    std::fs::create_dir_all(dir)?;
    let mut cycles = BufWriter::new(File::create(dir.join("cycles.bin"))?);
    let mut regw = BufWriter::new(File::create(dir.join("regwrites.bin"))?);
    let mut memw = BufWriter::new(File::create(dir.join("memwrites.bin"))?);
    let mut rck = BufWriter::new(File::create(dir.join("regs.ckpt"))?);
    let mut mck = BufWriter::new(File::create(dir.join("memckpt.bin"))?);
    let mut idx: Vec<u64> = Vec::new();
    let mut mck_off: u64 = 0;
    let mut regs = [0u32; 32];
    let mut mem: BTreeMap<u32, u32> = BTreeMap::new();
    let mut syscalls: Vec<SyscallView> = Vec::new();
    for (i, e) in exec.events.iter().enumerate() {
        let clk = i as u32;
        debug_assert_eq!(e.clk, clk);
        if clk % REG_CKPT == 0 { write_reg_ckpt(&mut rck, &regs)?; }
        if clk % MEM_CKPT == 0 { write_mem_ckpt(&mut mck, &mem, &mut idx, &mut mck_off)?; }
        let rdw = e.accesses.iter().find(|a| a.space == SPACE_REG && a.is_write);
        let (rd, rd_val, writes_rd) = match rdw { Some(a) => (a.addr as u8, a.value, true), None => (e.dec.rd as u8, 0, false) };
        let mut flags = 0u8;
        if writes_rd { flags |= F_WRITES_RD; }
        if e.dec.is_lb | e.dec.is_lh | e.dec.is_lw == 1 { flags |= F_MEM_READ; }
        if e.dec.is_sb | e.dec.is_sh | e.dec.is_sw == 1 { flags |= F_MEM_WRITE; }
        if e.sys.is_some() { flags |= F_SYSCALL; }
        if e.hash_row.is_some() { flags |= F_HASH_ROW; }
        let sys_idx = match &e.sys { Some(s) => { syscalls.push(syscall_view(clk, s)); (syscalls.len() - 1) as u32 } None => u32::MAX };
        put(&mut cycles, clk)?; put(&mut cycles, e.pc)?; put(&mut cycles, e.next_pc)?; put(&mut cycles, e.instr.encode())?;
        cycles.write_all(&[rd, flags])?; cycles.write_all(&(e.alu.len().min(u16::MAX as usize) as u16).to_le_bytes())?;
        put(&mut cycles, rd_val)?; put(&mut cycles, e.mem_addr)?; put(&mut cycles, e.mem_val)?; put(&mut cycles, sys_idx)?;
        for a in all_accesses(e).filter(|a| a.is_write) {
            if a.space == SPACE_REG {
                if a.addr == 0 { continue; }
                regs[a.addr as usize] = a.value;
                put(&mut regw, clk)?; put(&mut regw, a.addr)?; put(&mut regw, a.value)?;
            } else if a.space == SPACE_RAM {
                mem.insert(a.addr, a.value);
                put(&mut memw, clk)?; put(&mut memw, a.addr)?; put(&mut memw, a.value)?;
            }
        }
    }
    // `regs_at(total)` / `memory_at(total, ..)` are legal, so a checkpoint is owed at clk == total
    // too when total is a multiple of the interval (including total == 0). The loop only emits
    // checkpoints for clk < total.
    let total = exec.events.len() as u32;
    if total % REG_CKPT == 0 { write_reg_ckpt(&mut rck, &regs)?; }
    if total % MEM_CKPT == 0 { write_mem_ckpt(&mut mck, &mem, &mut idx, &mut mck_off)?; }
    cycles.flush()?; regw.flush()?; memw.flush()?; rck.flush()?; mck.flush()?;
    let mut idxf = BufWriter::new(File::create(dir.join("memckpt.idx"))?);
    for o in idx { idxf.write_all(&o.to_le_bytes())?; }
    idxf.flush()?;
    // Written last: its presence is the completion marker `Trace::open` checks.
    std::fs::write(dir.join("syscalls.json"), serde_json::to_vec(&syscalls)?)?;
    Ok(total)
}

pub struct Trace {
    dir: PathBuf,
    pub total: u32,
    cycles: File,
    regwrites: File,
    memwrites: File,
    regs_ckpt: File,
    memckpt: File,
    memckpt_idx: Vec<u64>,
}

fn corrupt(msg: &str) -> io::Error { io::Error::new(io::ErrorKind::InvalidData, msg.to_string()) }
fn bad(msg: String) -> io::Error { io::Error::new(io::ErrorKind::InvalidInput, msg) }
fn u32_at(b: &[u8], i: usize) -> u32 { u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) }

/// `(clk, key, value)` records of a 12-byte log, sorted by clk. `from..to` by record index.
fn read_log(f: &File, from: u64, to: u64) -> io::Result<Vec<(u32, u32, u32)>> {
    let mut buf = vec![0u8; ((to - from) * LOG_BYTES) as usize];
    f.read_exact_at(&mut buf, from * LOG_BYTES)?;
    Ok(buf.chunks_exact(12).map(|c| (u32_at(c, 0), u32_at(c, 4), u32_at(c, 8))).collect())
}

/// The first record index whose clk is ≥ `clk` (binary search over positional reads).
fn lower_bound(f: &File, n: u64, clk: u32) -> io::Result<u64> {
    let (mut lo, mut hi) = (0u64, n);
    let mut b = [0u8; 4];
    while lo < hi {
        let mid = (lo + hi) / 2;
        f.read_exact_at(&mut b, mid * LOG_BYTES)?;
        if u32::from_le_bytes(b) < clk { lo = mid + 1; } else { hi = mid; }
    }
    Ok(lo)
}

impl Trace {
    pub fn open(dir: &Path) -> io::Result<Trace> {
        if !dir.join("syscalls.json").exists() { return Err(corrupt("incomplete trace")); }
        let cycles = File::open(dir.join("cycles.bin"))?;
        let total = (cycles.metadata()?.len() / REC_BYTES) as u32;
        let idx_bytes = std::fs::read(dir.join("memckpt.idx"))?;
        let memckpt_idx = idx_bytes.chunks_exact(8).map(|c| u64::from_le_bytes(c.try_into().unwrap())).collect();
        Ok(Trace {
            dir: dir.to_path_buf(), total, cycles,
            regwrites: File::open(dir.join("regwrites.bin"))?,
            memwrites: File::open(dir.join("memwrites.bin"))?,
            regs_ckpt: File::open(dir.join("regs.ckpt"))?,
            memckpt: File::open(dir.join("memckpt.bin"))?,
            memckpt_idx,
        })
    }

    fn log_len(f: &File) -> io::Result<u64> { Ok(f.metadata()?.len() / LOG_BYTES) }

    pub fn syscalls(&self) -> io::Result<Vec<SyscallView>> {
        Ok(serde_json::from_slice(&std::fs::read(self.dir.join("syscalls.json"))?)?)
    }

    pub fn cycles(&self, from: u32, count: u32) -> io::Result<Vec<CycleView>> {
        if count > MAX_PAGE { return Err(bad(format!("count {count} exceeds {MAX_PAGE}"))); }
        if from >= self.total { return Ok(Vec::new()); }
        let n = count.min(self.total - from) as u64;
        let mut buf = vec![0u8; (n * REC_BYTES) as usize];
        self.cycles.read_exact_at(&mut buf, from as u64 * REC_BYTES)?;
        Ok(buf.chunks_exact(REC_BYTES as usize).map(|c| {
            let flags = c[17];
            CycleView {
                clk: u32_at(c, 0), pc: u32_at(c, 4), next_pc: u32_at(c, 8), word: u32_at(c, 12),
                rd: c[16], writes_rd: flags & F_WRITES_RD != 0, rd_val: u32_at(c, 20),
                mem_read: flags & F_MEM_READ != 0, mem_write: flags & F_MEM_WRITE != 0, mem_addr: u32_at(c, 24), mem_val: u32_at(c, 28),
                syscall: flags & F_SYSCALL != 0, hash_row: flags & F_HASH_ROW != 0,
                n_alu: u16::from_le_bytes([c[18], c[19]]), sys_idx: u32_at(c, 32),
            }
        }).collect())
    }

    pub fn cycle(&self, clk: u32) -> io::Result<CycleView> {
        self.cycles(clk, 1)?.into_iter().next().ok_or_else(|| bad(format!("cycle {clk} is past the end ({})", self.total)))
    }

    pub fn regs_at(&self, clk: u32) -> io::Result<[u32; 32]> {
        if clk > self.total { return Err(bad(format!("clk {clk} > total {}", self.total))); }
        let ck = clk / REG_CKPT;
        let mut buf = [0u8; 128];
        self.regs_ckpt.read_exact_at(&mut buf, ck as u64 * 128)?;
        let mut regs = [0u32; 32];
        for (i, r) in regs.iter_mut().enumerate() { *r = u32_at(&buf, 4 * i); }
        let n = Self::log_len(&self.regwrites)?;
        let from = lower_bound(&self.regwrites, n, ck * REG_CKPT)?;
        let to = lower_bound(&self.regwrites, n, clk)?;
        for (_, reg, val) in read_log(&self.regwrites, from, to)? { if reg != 0 && reg < 32 { regs[reg as usize] = val; } }
        Ok(regs)
    }

    pub fn memory_at(&self, clk: u32, addr: u32, words: u32) -> io::Result<Vec<u32>> {
        if words > MAX_WINDOW { return Err(bad(format!("window {words} exceeds {MAX_WINDOW}"))); }
        if clk > self.total { return Err(bad(format!("clk {clk} > total {}", self.total))); }
        let ck = (clk / MEM_CKPT) as usize;
        let off = *self.memckpt_idx.get(ck).ok_or_else(|| corrupt("missing memory checkpoint"))?;
        let mut nb = [0u8; 4];
        self.memckpt.read_exact_at(&mut nb, off)?;
        let n = u32::from_le_bytes(nb) as usize;
        let mut buf = vec![0u8; 8 * n];
        self.memckpt.read_exact_at(&mut buf, off + 4)?;
        let end = addr.saturating_add(words);
        let mut out = vec![0u32; words as usize];
        for c in buf.chunks_exact(8) {
            let a = u32_at(c, 0);
            if a >= addr && a < end { out[(a - addr) as usize] = u32_at(c, 4); }
        }
        let total = Self::log_len(&self.memwrites)?;
        let from = lower_bound(&self.memwrites, total, ck as u32 * MEM_CKPT)?;
        let to = lower_bound(&self.memwrites, total, clk)?;
        for (_, a, v) in read_log(&self.memwrites, from, to)? { if a >= addr && a < end { out[(a - addr) as usize] = v; } }
        Ok(out)
    }
}

use crate::tiers;
use crate::views::{DiscoveryStep, RunSummary, TableRows};
use rand_zkvm::emulator::ExecError;
use rand_zkvm::isa::Program;

pub fn summarize(rid: &str, program: &Program, n_in: usize, n_pub: usize, exec: &Execution, discovery: Vec<DiscoveryStep>) -> RunSummary {
    let rows = tiers::rows(program, n_in, n_pub, exec);
    let total = tiers::cycles_total(&rows);
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut hits: std::collections::HashMap<u32, usize> = std::collections::HashMap::new();
    for e in &exec.events {
        if let Some(s) = &e.sys { *counts.entry(syscall_view(0, s).kind).or_default() += 1; }
        *hits.entry(e.pc).or_default() += 1;
    }
    let mut hot: Vec<(u32, usize)> = hits.into_iter().collect();
    hot.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    hot.truncate(32);
    RunSummary {
        rid: rid.into(), cycles: exec.cycles(), halted: exec.halted, trap: None, trap_clk: None,
        outputs: exec.outputs, status: exec.outputs[0], tier: tiers::pick(&rows, total),
        tiers: tiers::capacities(&rows, total), rows,
        syscall_counts: counts.into_iter().collect(), hot_pcs: hot,
        input_words: n_in, public_words: n_pub, discovery,
    }
}

pub fn trapped(rid: &str, err: &ExecError, n_in: usize, n_pub: usize, discovery: Vec<DiscoveryStep>) -> RunSummary {
    let rows = TableRows { cpu: 0, memory: 0, alu: 0, poseidon2_permutations: 0, keccak_permutations: 0, sha256_compressions: 0, digest_rows: 0 };
    RunSummary {
        rid: rid.into(), cycles: 0, halted: false, trap: Some(format!("{err:?}")),
        trap_clk: match err { ExecError::OutOfCycles(n) => Some(*n as u32), _ => None },
        outputs: [0; 8], status: 2, tier: None, tiers: tiers::capacities(&rows, 0), rows,
        syscall_counts: vec![], hot_pcs: vec![], input_words: n_in, public_words: n_pub, discovery,
    }
}
