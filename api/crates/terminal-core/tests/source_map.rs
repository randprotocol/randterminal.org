use terminal_core::source_map::{compose, parse_debug_line, src_pc_for_line};
use terminal_core::views::OpLine;

const DWARF: &str = r#"guest.elf:	file format elf32-littleriscv

.debug_line contents:
debug_line[0x00000000]
Line table prologue:
    total_length: 0x00000123
          format: DWARF32
         version: 5
include_directories[  0] = "/rand-circuits/jobs/job/shim"
file_names[  0]:
           name: "contract.c"
      dir_index: 0
file_names[  1]:
           name: "evm_rt.c"
      dir_index: 0

Address            Line   Column File   ISA Discriminator OpIndex Flags
------------------ ------ ------ ------ --- ------------- ------- -------------
0x0000000000010000    105      5      0   0             0       0  is_stmt
0x0000000000010004    105     12      0   0             0       0
0x0000000000010008    107      5      0   0             0       0  is_stmt
0x0000000000010008    107      5      0   0             0       0  is_stmt
0x000000000001000c     40      1      1   0             0       0  is_stmt
0x0000000000010010    107      5      0   0             0       0  is_stmt end_sequence
"#;

#[test]
fn debug_line_rows_for_the_contract_file_only() {
    let rows = parse_debug_line(DWARF, "contract.c");
    assert_eq!(rows, vec![(0x10000, 105), (0x10008, 107)]);
}

#[test]
fn compose_and_lookup() {
    let op_map = vec![OpLine { pc: 0, line: 105 }, OpLine { pc: 2, line: 106 }, OpLine { pc: 4, line: 107 }];
    let sm = compose(&[(0x10000, 105), (0x10008, 107)], &op_map);
    assert_eq!(sm.pc_to_c, vec![(0x10000, 105), (0x10008, 107)]);
    assert_eq!(sm.c_to_src, vec![(105, 0), (106, 2), (107, 4)]);
    assert_eq!(src_pc_for_line(&op_map, 104), None);
    assert_eq!(src_pc_for_line(&op_map, 105), Some(0));
    assert_eq!(src_pc_for_line(&op_map, 106), Some(2));
    assert_eq!(src_pc_for_line(&op_map, 1000), Some(4));
}
