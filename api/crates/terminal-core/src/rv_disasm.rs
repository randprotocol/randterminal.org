//! RV32IM text for `rand_zkvm::isa::Instr`, GNU-assembler style with ABI register names and the
//! usual pseudo-instructions (`li`, `j`, `ret`, `neg`, `seqz`), which is what a developer reading
//! objdump output expects.
use rand_zkvm::isa::{AluOp, BranchCond, Instr, Width};

const NAMES: [&str; 32] = [
    "zero", "ra", "sp", "gp", "tp", "t0", "t1", "t2", "s0", "s1", "a0", "a1", "a2", "a3", "a4", "a5",
    "a6", "a7", "s2", "s3", "s4", "s5", "s6", "s7", "s8", "s9", "s10", "s11", "t3", "t4", "t5", "t6",
];

pub fn reg_name(r: u32) -> &'static str { NAMES[(r as usize) & 31] }

pub fn ecall_name(a7: u32) -> Option<&'static str> {
    Some(match a7 {
        0 => "HALT", 1 => "WRITE_OUTPUT", 2 => "READ_INPUT", 3 => "POSEIDON2", 4 => "KECCAK",
        5 => "SHA256", 6 => "READ_PUBLIC", 7 => "POSEIDON2_LEN", _ => return None,
    })
}

fn alu_name(op: AluOp) -> &'static str {
    match op {
        AluOp::Add => "add", AluOp::Sub => "sub", AluOp::And => "and", AluOp::Or => "or", AluOp::Xor => "xor",
        AluOp::Sll => "sll", AluOp::Srl => "srl", AluOp::Sra => "sra", AluOp::Slt => "slt", AluOp::Sltu => "sltu",
        AluOp::Eq => "eq", AluOp::Mul => "mul", AluOp::Mulh => "mulh", AluOp::Mulhu => "mulhu",
        AluOp::Mulhsu => "mulhsu", AluOp::Div => "div", AluOp::Divu => "divu", AluOp::Rem => "rem", AluOp::Remu => "remu",
    }
}

fn branch_name(c: BranchCond) -> &'static str {
    match c { BranchCond::Eq => "beq", BranchCond::Ne => "bne", BranchCond::Lt => "blt", BranchCond::Ge => "bge", BranchCond::Ltu => "bltu", BranchCond::Geu => "bgeu" }
}

fn simm(imm: u32) -> i32 { imm as i32 }

fn width_suffix(w: Width, signed: bool, load: bool) -> &'static str {
    match (w, signed || !load) {
        (Width::Byte, true) => "b", (Width::Byte, false) => "bu",
        (Width::Half, true) => "h", (Width::Half, false) => "hu",
        (Width::Word, _) => "w",
    }
}

pub fn text(i: &Instr) -> String {
    let r = reg_name;
    match *i {
        Instr::Lui { rd, imm } => format!("lui {}, {:#x}", r(rd), imm >> 12),
        Instr::Auipc { rd, imm } => format!("auipc {}, {:#x}", r(rd), imm >> 12),
        Instr::Jal { rd: 0, imm } => format!("j {}", simm(imm)),
        Instr::Jal { rd, imm } => format!("jal {}, {}", r(rd), simm(imm)),
        Instr::Jalr { rd: 0, rs1: 1, imm: 0 } => "ret".to_string(),
        Instr::Jalr { rd, rs1, imm } => format!("jalr {}, {}({})", r(rd), simm(imm), r(rs1)),
        Instr::Branch { cond, rs1, rs2, imm } => format!("{} {}, {}, {}", branch_name(cond), r(rs1), r(rs2), simm(imm)),
        Instr::Load { rd, rs1, imm, width, signed } => format!("l{} {}, {}({})", width_suffix(width, signed, true), r(rd), simm(imm), r(rs1)),
        Instr::Store { rs1, rs2, imm, width } => format!("s{} {}, {}({})", width_suffix(width, true, false), r(rs2), simm(imm), r(rs1)),
        Instr::AluImm { op: AluOp::Add, rd, rs1: 0, imm } => format!("li {}, {}", r(rd), simm(imm)),
        Instr::AluImm { op: AluOp::Sltu, rd, rs1, imm: 1 } => format!("seqz {}, {}", r(rd), r(rs1)),
        Instr::AluImm { op, rd, rs1, imm } => format!("{}i {}, {}, {}", alu_name(op), r(rd), r(rs1), simm(imm)),
        Instr::AluReg { op: AluOp::Sub, rd, rs1: 0, rs2 } => format!("neg {}, {}", r(rd), r(rs2)),
        Instr::AluReg { op, rd, rs1, rs2 } => format!("{} {}, {}, {}", alu_name(op), r(rd), r(rs1), r(rs2)),
        Instr::Ecall => "ecall".to_string(),
    }
}
