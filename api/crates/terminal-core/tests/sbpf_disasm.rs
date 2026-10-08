use sbpf_core::isa::{decode, Insn};
use sbpf_core::syscalls::SOL_LOG;
use terminal_core::sbpf_disasm::{syscall_name, text};

fn i(opc: u8, dst: u8, src: u8, off: i16, imm: i32) -> Insn { Insn { opc, dst, src, off, imm } }

#[test]
fn alu_and_mov() {
    assert_eq!(text(0, &i(0x07, 1, 0, 0, 16), None), "add64 r1, 16");
    assert_eq!(text(0, &i(0x0f, 1, 2, 0, 0), None), "add64 r1, r2");
    assert_eq!(text(0, &i(0x04, 3, 0, 0, -1), None), "add32 r3, -1");
    assert_eq!(text(0, &i(0xb7, 0, 0, 0, 0), None), "mov64 r0, 0");
    assert_eq!(text(0, &i(0xbf, 6, 1, 0, 0), None), "mov64 r6, r1");
    assert_eq!(text(0, &i(0x67, 1, 0, 0, 32), None), "lsh64 r1, 32");
    assert_eq!(text(0, &i(0xc7, 1, 0, 0, 3), None), "arsh64 r1, 3");
    assert_eq!(text(0, &i(0x87, 1, 0, 0, 0), None), "neg64 r1");
    assert_eq!(text(0, &i(0xd4, 1, 0, 0, 16), None), "le16 r1");
    assert_eq!(text(0, &i(0xdc, 1, 0, 0, 64), None), "be64 r1");
}

#[test]
fn loads_stores() {
    assert_eq!(text(0, &i(0x79, 1, 10, -8, 0), None), "ldxdw r1, [r10-8]");
    assert_eq!(text(0, &i(0x61, 1, 2, 4, 0), None), "ldxw r1, [r2+4]");
    assert_eq!(text(0, &i(0x71, 1, 2, 0, 0), None), "ldxb r1, [r2+0]");
    assert_eq!(text(0, &i(0x7b, 10, 1, -16, 0), None), "stxdw [r10-16], r1");
    assert_eq!(text(0, &i(0x72, 10, 0, -1, 7), None), "stb [r10-1], 7");
    let lo = i(0x18, 1, 0, 0, 0x5678);
    let hi = i(0x00, 0, 0, 0, 0x1234);
    assert_eq!(text(0, &lo, Some(&hi)), "lddw r1, 0x123400005678");
}

#[test]
fn jumps_calls_exit() {
    assert_eq!(text(10, &i(0x05, 0, 0, 5, 0), None), "ja +5 ; -> 16");
    assert_eq!(text(10, &i(0x15, 1, 0, -3, 0), None), "jeq r1, 0, -3 ; -> 8");
    assert_eq!(text(10, &i(0x1d, 1, 2, 2, 0), None), "jeq r1, r2, +2 ; -> 13");
    assert_eq!(text(10, &i(0xad, 1, 2, 1, 0), None), "jlt r1, r2, +1 ; -> 12");
    // The scanner's convention: src 0 = internal call, src 1 = syscall by hash.
    assert_eq!(text(10, &i(0x85, 0, 0, 0, 4), None), "call +4 ; -> 15");
    assert_eq!(text(10, &i(0x85, 0, 1, 0, 0x1234), None), "syscall 0x1234");
    assert_eq!(text(10, &i(0x85, 0, 1, 0, SOL_LOG as i32), None), "syscall sol_log_");
    assert_eq!(text(10, &i(0x85, 0, 3, 0, 9), None), "call 9 ; bad src 3");
    assert_eq!(text(10, &i(0x8d, 0, 0, 0, 3), None), "callx r3");
    assert_eq!(text(10, &i(0x95, 0, 0, 0, 0), None), "exit");
    assert_eq!(text(10, &i(0xff, 0, 0, 0, 0), None), "db 0xff");
}

#[test]
fn syscall_names_match_the_transpiler() {
    for (hash, cname) in sbpf2rv::emit::SYSCALLS {
        let name = syscall_name(hash).expect(cname);
        // C names drop Solana's `sol_` prefix and trailing underscore: sol_log_ -> sbpf_sys_log.
        let bare = name.trim_end_matches('_');
        let bare = bare.strip_prefix("sol_").unwrap_or(bare);
        assert_eq!(cname, format!("sbpf_sys_{bare}"), "{name}");
    }
    assert_eq!(syscall_name(0xdeadbeef), None);
}

#[test]
fn sol_log_hash_is_the_brief_literal() {
    assert_eq!(SOL_LOG, 0x207559bd);
}

#[test]
fn decodes_real_slots() {
    let slot = u64::from_le_bytes([0x95, 0, 0, 0, 0, 0, 0, 0]);
    assert_eq!(text(0, &decode(slot), None), "exit");
}
