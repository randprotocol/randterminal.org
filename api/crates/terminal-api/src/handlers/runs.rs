use crate::jobs::read_regular;
use crate::{
    error::{ApiResult, AppError},
    runs,
    state::AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    Json,
};
use rand_zkvm::isa::Instr;
use serde::Deserialize;
use terminal_core::source_map::src_pc_for_line;
use terminal_core::views::*;
use terminal_core::{rv_disasm, trace::Trace};

pub async fn create(
    State(s): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<RunRequest>,
) -> ApiResult<(StatusCode, Json<RunSummary>)> {
    Ok((StatusCode::CREATED, Json(runs::create(&s, &id, req).await?)))
}

pub async fn summary(
    State(s): State<AppState>,
    Path(rid): Path<String>,
) -> ApiResult<Json<RunSummary>> {
    let (rdir, _) = runs::open(&s, &rid)?;
    Ok(Json(serde_json::from_slice(&std::fs::read(
        rdir.join("summary.json"),
    )?)?))
}

#[derive(Deserialize)]
pub struct Page {
    #[serde(default)]
    from: u32,
    #[serde(default = "page")]
    count: u32,
}
fn page() -> u32 {
    1024
}

pub async fn cycles(
    State(s): State<AppState>,
    Path(rid): Path<String>,
    Query(q): Query<Page>,
) -> ApiResult<Json<CyclePage>> {
    let (rdir, _) = runs::open(&s, &rid)?;
    let t = runs::trace_of(&rdir)?;
    Ok(Json(CyclePage {
        from: q.from,
        cycles: t.cycles(q.from, q.count).map_err(runs::trace_err)?,
        total: t.total,
    }))
}

#[derive(Deserialize)]
pub struct At {
    at: u32,
}

fn c_line_for(map: &Option<SourceMap>, pc: u32) -> Option<usize> {
    let rows = &map.as_ref()?.pc_to_c;
    let i = rows.partition_point(|r| r.0 <= pc);
    (i > 0).then(|| rows[i - 1].1)
}

pub async fn state(
    State(s): State<AppState>,
    Path(rid): Path<String>,
    Query(q): Query<At>,
) -> ApiResult<Json<StateView>> {
    let (rdir, jdir) = runs::open(&s, &rid)?;
    let t: Trace = runs::trace_of(&rdir)?;
    if q.at >= t.total {
        return Err(AppError::BadRequest(format!(
            "at {} is past the last cycle ({})",
            q.at,
            t.total.saturating_sub(1)
        )));
    }
    let cycle = t.cycle(q.at).map_err(runs::trace_err)?;
    let regs = t.regs_at(q.at).map_err(runs::trace_err)?;
    let map: Option<SourceMap> = read_regular(&jdir.join("build/source_map.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .flatten();
    let op_map: Vec<OpLine> = read_regular(&jdir.join("transpile/op_map.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    let c_line = c_line_for(&map, cycle.pc);
    let text = Instr::decode(cycle.word)
        .map(|i| rv_disasm::text(&i))
        .unwrap_or_else(|_| format!(".word {:#010x}", cycle.word));
    Ok(Json(StateView {
        clk: q.at,
        regs,
        cycle,
        text,
        c_line,
        src_pc: c_line.and_then(|l| src_pc_for_line(&op_map, l)),
    }))
}

#[derive(Deserialize)]
pub struct Mem {
    at: u32,
    addr: u32,
    #[serde(default = "words")]
    words: u32,
}
fn words() -> u32 {
    64
}

pub async fn memory(
    State(s): State<AppState>,
    Path(rid): Path<String>,
    Query(q): Query<Mem>,
) -> ApiResult<Json<MemoryView>> {
    let (rdir, _) = runs::open(&s, &rid)?;
    let t = runs::trace_of(&rdir)?;
    Ok(Json(MemoryView {
        clk: q.at,
        addr: q.addr,
        words: t
            .memory_at(q.at, q.addr, q.words)
            .map_err(runs::trace_err)?,
    }))
}

pub async fn syscalls(
    State(s): State<AppState>,
    Path(rid): Path<String>,
) -> ApiResult<Json<Vec<SyscallView>>> {
    let (rdir, _) = runs::open(&s, &rid)?;
    Ok(Json(
        runs::trace_of(&rdir)?.syscalls().map_err(runs::trace_err)?,
    ))
}
