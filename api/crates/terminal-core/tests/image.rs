#[path = "fixtures.rs"]
mod fixtures;
use terminal_core::image::{disasm, hex8, load, view};
use terminal_core::views::OpLine;

fn info_field(info: &str, key: &str) -> String {
    info.lines()
        .find_map(|l| l.strip_prefix(key))
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| panic!("{key}"))
}

#[test]
fn erc20_image_view_matches_rand_guest_info() {
    let bytes = fixtures::bytes("erc20.image.bin");
    let info = fixtures::text("erc20.info.txt");
    let v = view(&bytes, 65_535, &[], None, "contract.c").unwrap();
    assert_eq!(v.words, 11_686);
    assert_eq!(v.text_words, 11_147);
    assert_eq!(v.data_words, 408);
    assert_eq!(v.prologue_words, 539);
    assert_eq!(v.base_pc, 0xf794);
    assert_eq!(v.text_base, 0x10000);
    assert_eq!(v.hc, info_field(&info, "hc "));
    assert_eq!(v.program_id, info_field(&info, "program id "));
    assert!(v.fits);
    assert_eq!(v.cap_words, 65_535);
    assert_eq!(v.disasm.len(), 11_686);
    assert_eq!(v.disasm[0].pc, 0xf794);
    assert_eq!(v.disasm[539].pc, 0x10000);
    assert!(v.disasm.iter().all(|d| !d.text.is_empty()));
    assert!(v.disasm.iter().any(|d| d.ecall.as_deref() == Some("HALT")));
    assert!(v.source_map.is_none());
    assert!(v.source_map_note.is_some());
}

#[test]
fn over_cap_does_not_fit_but_still_views() {
    let bytes = fixtures::bytes("erc20.image.bin");
    let v = view(&bytes, 4_096, &[], None, "contract.c").unwrap();
    assert!(!v.fits);
    assert_eq!(v.cap_words, 4_096);
    assert!(
        v.findings.iter().any(|f| f.rule == "Cap"),
        "{:?}",
        v.findings
    );
}

#[test]
fn spl_image_view() {
    let bytes = fixtures::bytes("spl.image.bin");
    let info = fixtures::text("spl.info.txt");
    let v = view(&bytes, 65_535, &[], None, "program.c").unwrap();
    assert_eq!(v.words, 65_096);
    assert_eq!(v.hc, info_field(&info, "hc "));
}

#[test]
fn garbage_is_refused() {
    assert!(load(&[1, 2, 3]).is_err());
    assert!(view(&[0u8; 40], 4096, &[], None, "x.c").is_err());
}

#[test]
fn source_map_is_attached_when_debug_line_is_given() {
    let bytes = fixtures::bytes("erc20.image.bin");
    let dl = "file_names[  0]:\n name: \"contract.c\"\nAddress Line Column File ISA Discriminator OpIndex Flags\n0x0000000000010000 105 5 0 0 0 0 is_stmt\n";
    let v = view(
        &bytes,
        65_535,
        &[OpLine { pc: 0, line: 105 }],
        Some(dl),
        "contract.c",
    )
    .unwrap();
    let sm = v.source_map.unwrap();
    assert_eq!(sm.pc_to_c, vec![(0x10000, 105)]);
    assert_eq!(sm.c_to_src, vec![(105, 0)]);
}

#[test]
fn hex8_formats_words() {
    assert_eq!(
        hex8(&[0xa0feae92, 1, 2, 3, 4, 5, 6, 7]),
        "a0feae9200000001000000020000000300000004000000050000000600000007"
    );
}

#[test]
fn disasm_of_loaded_program_covers_every_word() {
    let p = load(&fixtures::bytes("erc20.image.bin")).unwrap();
    assert_eq!(disasm(&p).len(), p.words.len());
}
