//! sBPF (Solana BPF v1) text for `sbpf_core::isa::Insn`, in the mnemonics `solana-rbpf`'s
//! disassembler prints, so a developer can compare against `cargo build-sbf --dump`.
//!
//! `call imm` follows the scanner (`sbpf2rv::scan`): `src == 0` is a slot-relative internal call
//! (`pc + 1 + imm`), `src == 1` is a syscall whose `imm` is the murmur3-32 hash of its name.
use sbpf_core::isa::{lddw_imm64, Insn};
use sbpf_core::syscalls::SUPPORTED;

/// The syscalls the transpiler supports, by murmur3-32 hash, named as Solana names them.
pub fn syscall_name(hash: u32) -> Option<&'static str> {
    SUPPORTED.iter().find(|(h, _)| *h == hash).map(|(_, n)| *n)
}

fn alu(op: u8) -> Option<&'static str> {
    Some(match op & 0xf0 {
        0x00 => "add", 0x10 => "sub", 0x20 => "mul", 0x30 => "div", 0x40 => "or", 0x50 => "and", 0x60 => "lsh",
        0x70 => "rsh", 0x80 => "neg", 0x90 => "mod", 0xa0 => "xor", 0xb0 => "mov", 0xc0 => "arsh", 0xd0 => "end",
        _ => return None,
    })
}

fn jmp(op: u8) -> Option<&'static str> {
    Some(match op & 0xf0 {
        0x00 => "ja", 0x10 => "jeq", 0x20 => "jgt", 0x30 => "jge", 0x40 => "jset", 0x50 => "jne", 0x60 => "jsgt",
        0x70 => "jsge", 0xa0 => "jlt", 0xb0 => "jle", 0xc0 => "jslt", 0xd0 => "jsle",
        _ => return None,
    })
}

fn size(op: u8) -> &'static str {
    match op & 0x18 {
        0x00 => "w",
        0x08 => "h",
        0x10 => "b",
        _ => "dw",
    }
}

fn mem(reg: u8, off: i16) -> String {
    if off < 0 { format!("[r{reg}{off}]") } else { format!("[r{reg}+{off}]") }
}

/// A slot-relative offset and the absolute slot it lands on.
fn rel(pc: usize, off: i64) -> String {
    let t = pc as i64 + 1 + off;
    if off < 0 { format!("{off} ; -> {t}") } else { format!("+{off} ; -> {t}") }
}

/// One instruction at slot `pc`. `next` is the following slot, the second half of an `lddw`.
pub fn text(pc: usize, i: &Insn, next: Option<&Insn>) -> String {
    let (opc, dst, src, off, imm) = (i.opc, i.dst, i.src, i.off, i.imm);
    match opc & 0x07 {
        0x07 | 0x04 => {
            let w = if opc & 0x07 == 0x07 { "64" } else { "32" };
            let Some(name) = alu(opc) else { return format!("db {opc:#04x}") };
            match name {
                "neg" => format!("neg{w} r{dst}"),
                "end" => format!("{}{} r{dst}", if opc & 0x08 != 0 { "be" } else { "le" }, imm),
                _ if opc & 0x08 != 0 => format!("{name}{w} r{dst}, r{src}"),
                _ => format!("{name}{w} r{dst}, {imm}"),
            }
        }
        0x05 => match opc {
            0x85 => match src {
                0 => format!("call {}", rel(pc, imm as i64)),
                1 => match syscall_name(imm as u32) {
                    Some(n) => format!("syscall {n}"),
                    None => format!("syscall {:#x}", imm as u32),
                },
                _ => format!("call {imm} ; bad src {src}"),
            },
            0x8d => format!("callx r{imm}"),
            0x95 => "exit".to_string(),
            0x05 => format!("ja {}", rel(pc, off as i64)),
            _ => match jmp(opc) {
                Some(name) if opc & 0x08 != 0 => format!("{name} r{dst}, r{src}, {}", rel(pc, off as i64)),
                Some(name) => format!("{name} r{dst}, {imm}, {}", rel(pc, off as i64)),
                None => format!("db {opc:#04x}"),
            },
        },
        0x00 if opc == 0x18 => match next {
            Some(hi) => format!("lddw r{dst}, {:#x}", lddw_imm64(*i, *hi)),
            None => format!("lddw r{dst}, {:#x} ; truncated", imm as u32),
        },
        0x01 => format!("ldx{} r{dst}, {}", size(opc), mem(src, off)),
        0x02 => format!("st{} {}, {imm}", size(opc), mem(dst, off)),
        0x03 => format!("stx{} {}, r{src}", size(opc), mem(dst, off)),
        _ => format!("db {opc:#04x}"),
    }
}
