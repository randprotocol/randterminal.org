//! Stage 4's view: the image as the chain sees it (words, hc, program id, cap) and as a developer
//! reads it (RV32IM listing with the source map).
use crate::rv_disasm::{ecall_name, text};
use crate::source_map::{compose, parse_debug_line};
use crate::views::*;
use rand_guest::{chain, check, pack};
use rand_zkvm::isa::{AluOp, Instr, Program, REG_A7};

/// The `hc` format: the eight digest words, each `{:08x}`, concatenated.
pub fn hex8(d: &[u32; 8]) -> String {
    d.iter().map(|w| format!("{w:08x}")).collect()
}

pub fn load(bytes: &[u8]) -> Result<Program, String> {
    Program::from_flat_image(bytes).map_err(|e| format!("{e:?}"))
}

fn rd_of(i: &Instr) -> Option<u32> {
    match *i {
        Instr::AluImm { rd, .. }
        | Instr::AluReg { rd, .. }
        | Instr::Load { rd, .. }
        | Instr::Lui { rd, .. }
        | Instr::Auipc { rd, .. }
        | Instr::Jal { rd, .. }
        | Instr::Jalr { rd, .. } => Some(rd),
        _ => None,
    }
}

/// The listing; an `ecall` is named when the last straight-line `addi a7, x0, n` fixes its number
/// (the same tracking `rand-guest check` uses).
pub fn disasm(p: &Program) -> Vec<DisasmLine> {
    let mut out = Vec::with_capacity(p.words.len());
    let mut a7: Option<u32> = None;
    for (i, &w) in p.words.iter().enumerate() {
        let pc = p.base_pc.wrapping_add((4 * i) as u32);
        match Instr::decode(w) {
            Ok(instr) => {
                let ecall = if matches!(instr, Instr::Ecall) {
                    a7.and_then(ecall_name).map(str::to_string)
                } else {
                    None
                };
                if let Instr::AluImm {
                    op: AluOp::Add,
                    rd: REG_A7,
                    rs1: 0,
                    imm,
                } = instr
                {
                    a7 = Some(imm);
                } else if rd_of(&instr) == Some(REG_A7)
                    || matches!(
                        instr,
                        Instr::Jal { .. } | Instr::Jalr { .. } | Instr::Branch { .. }
                    )
                {
                    a7 = None;
                }
                out.push(DisasmLine {
                    pc,
                    word: w,
                    text: text(&instr),
                    ecall,
                });
            }
            Err(e) => {
                a7 = None;
                out.push(DisasmLine {
                    pc,
                    word: w,
                    text: format!(".word {w:#010x} ; {e:?}"),
                    ecall: None,
                });
            }
        }
    }
    out
}

pub fn view(
    bytes: &[u8],
    cap: usize,
    op_map: &[OpLine],
    debug_line: Option<&str>,
    c_file: &str,
) -> Result<ImageView, String> {
    let (info, text_words, _data) = pack::split(bytes).map_err(|e| e.to_string())?;
    let p = load(bytes)?;
    // As `rand-guest info`/`check` do: the checker reads the image's text at its text base, and the
    // cap is measured against the whole program (text + data prologue).
    let report = check::check_text(info.text_base, &text_words, p.words.len(), cap);
    let (source_map, source_map_note) = match debug_line {
        Some(dl) => {
            let rows = parse_debug_line(dl, c_file);
            if rows.is_empty() {
                (
                    None,
                    Some(format!(
                        "the debug build's line table has no rows for {c_file}"
                    )),
                )
            } else {
                (Some(compose(&rows, op_map)), None)
            }
        }
        None => (
            None,
            Some(
                "no debug build: pc-level source map unavailable; the opcode-to-C map still works"
                    .to_string(),
            ),
        ),
    };
    Ok(ImageView {
        words: p.words.len(),
        text_words: info.n_text,
        data_words: info.n_data,
        prologue_words: p.words.len() - info.n_text,
        base_pc: p.base_pc,
        text_base: info.text_base,
        hc: hex8(&p.digest()),
        program_id: hex::encode(chain::program_id(p.base_pc, &p.words)),
        cap_words: cap,
        fits: p.words.len() <= cap,
        findings: report
            .findings
            .iter()
            .map(|f| CheckFinding {
                addr: f.addr,
                word: f.word,
                rule: format!("{:?}", f.rule),
                what: f.what.clone(),
            })
            .collect(),
        disasm: disasm(&p),
        source_map,
        source_map_note,
    })
}
