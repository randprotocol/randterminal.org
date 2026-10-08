use rand_zkvm::isa::{AluOp, BranchCond, Instr, Width};
use terminal_core::rv_disasm::{ecall_name, reg_name, text};

#[test]
fn abi_register_names() {
    assert_eq!(reg_name(0), "zero");
    assert_eq!(reg_name(1), "ra");
    assert_eq!(reg_name(2), "sp");
    assert_eq!(reg_name(8), "s0");
    assert_eq!(reg_name(10), "a0");
    assert_eq!(reg_name(17), "a7");
    assert_eq!(reg_name(31), "t6");
}

#[test]
fn formats_each_shape() {
    assert_eq!(
        text(&Instr::Lui {
            rd: 10,
            imm: 0x12345000
        }),
        "lui a0, 0x12345"
    );
    assert_eq!(text(&Instr::Auipc { rd: 2, imm: 0x1000 }), "auipc sp, 0x1");
    assert_eq!(text(&Instr::Jal { rd: 1, imm: 8 }), "jal ra, 8");
    assert_eq!(
        text(&Instr::Jal {
            rd: 0,
            imm: (-4i32) as u32
        }),
        "j -4"
    );
    assert_eq!(
        text(&Instr::Jalr {
            rd: 0,
            rs1: 1,
            imm: 0
        }),
        "ret"
    );
    assert_eq!(
        text(&Instr::Jalr {
            rd: 1,
            rs1: 5,
            imm: 12
        }),
        "jalr ra, 12(t0)"
    );
    assert_eq!(
        text(&Instr::Branch {
            cond: BranchCond::Eq,
            rs1: 10,
            rs2: 0,
            imm: 16
        }),
        "beq a0, zero, 16"
    );
    assert_eq!(
        text(&Instr::Branch {
            cond: BranchCond::Geu,
            rs1: 11,
            rs2: 12,
            imm: (-8i32) as u32
        }),
        "bgeu a1, a2, -8"
    );
    assert_eq!(
        text(&Instr::Load {
            rd: 10,
            rs1: 2,
            imm: 4,
            width: Width::Word,
            signed: true
        }),
        "lw a0, 4(sp)"
    );
    assert_eq!(
        text(&Instr::Load {
            rd: 10,
            rs1: 2,
            imm: 0,
            width: Width::Byte,
            signed: false
        }),
        "lbu a0, 0(sp)"
    );
    assert_eq!(
        text(&Instr::Load {
            rd: 10,
            rs1: 2,
            imm: 0,
            width: Width::Half,
            signed: true
        }),
        "lh a0, 0(sp)"
    );
    assert_eq!(
        text(&Instr::Store {
            rs1: 2,
            rs2: 10,
            imm: (-4i32) as u32,
            width: Width::Word
        }),
        "sw a0, -4(sp)"
    );
    assert_eq!(
        text(&Instr::Store {
            rs1: 2,
            rs2: 10,
            imm: 0,
            width: Width::Byte
        }),
        "sb a0, 0(sp)"
    );
    assert_eq!(
        text(&Instr::AluImm {
            op: AluOp::Add,
            rd: 10,
            rs1: 0,
            imm: 7
        }),
        "li a0, 7"
    );
    assert_eq!(
        text(&Instr::AluImm {
            op: AluOp::Add,
            rd: 10,
            rs1: 10,
            imm: (-1i32) as u32
        }),
        "addi a0, a0, -1"
    );
    assert_eq!(
        text(&Instr::AluImm {
            op: AluOp::Sll,
            rd: 10,
            rs1: 10,
            imm: 2
        }),
        "slli a0, a0, 2"
    );
    assert_eq!(
        text(&Instr::AluImm {
            op: AluOp::Sltu,
            rd: 10,
            rs1: 11,
            imm: 1
        }),
        "seqz a0, a1"
    );
    assert_eq!(
        text(&Instr::AluReg {
            op: AluOp::Mul,
            rd: 10,
            rs1: 11,
            rs2: 12
        }),
        "mul a0, a1, a2"
    );
    assert_eq!(
        text(&Instr::AluReg {
            op: AluOp::Sub,
            rd: 10,
            rs1: 0,
            rs2: 11
        }),
        "neg a0, a1"
    );
    assert_eq!(
        text(&Instr::AluReg {
            op: AluOp::Remu,
            rd: 10,
            rs1: 11,
            rs2: 12
        }),
        "remu a0, a1, a2"
    );
    assert_eq!(text(&Instr::Ecall), "ecall");
}

#[test]
fn ecall_names() {
    assert_eq!(ecall_name(0), Some("HALT"));
    assert_eq!(ecall_name(1), Some("WRITE_OUTPUT"));
    assert_eq!(ecall_name(2), Some("READ_INPUT"));
    assert_eq!(ecall_name(3), Some("POSEIDON2"));
    assert_eq!(ecall_name(4), Some("KECCAK"));
    assert_eq!(ecall_name(5), Some("SHA256"));
    assert_eq!(ecall_name(6), Some("READ_PUBLIC"));
    assert_eq!(ecall_name(7), Some("POSEIDON2_LEN"));
    assert_eq!(ecall_name(8), None);
}

#[test]
fn round_trips_through_encode_decode() {
    for i in [
        Instr::Lui {
            rd: 10,
            imm: 0x12345000,
        },
        Instr::Jalr {
            rd: 1,
            rs1: 5,
            imm: 12,
        },
        Instr::AluReg {
            op: AluOp::Mulhsu,
            rd: 3,
            rs1: 4,
            rs2: 5,
        },
        Instr::Branch {
            cond: BranchCond::Ltu,
            rs1: 1,
            rs2: 2,
            imm: 32,
        },
    ] {
        let back = Instr::decode(i.encode()).unwrap();
        assert_eq!(text(&back), text(&i));
    }
}
