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
    let (map, op_map) = maps_of(&jdir);
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

/// The job's pc → C line map (absent without a reproducing -g build) and its C line → source op map.
fn maps_of(jdir: &std::path::Path) -> (Option<SourceMap>, Vec<OpLine>) {
    let map = read_regular(&jdir.join("build/source_map.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .flatten();
    let op_map = read_regular(&jdir.join("transpile/op_map.json"))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default();
    (map, op_map)
}

/// The most cycles one `next_src` request scans (40 MB of cycle records at most).
pub const NEXT_SRC_SCAN: u32 = 1 << 20;

#[derive(Deserialize)]
pub struct NextSrcQuery {
    from: u32,
    dir: i8,
}

#[derive(serde::Serialize)]
pub struct NextSrc {
    clk: u32,
    src_pc: usize,
}

/// The first cycle after (`dir=1`) or before (`dir=-1`) `from` whose source op is mapped and
/// differs from the one at `from`, within [`NEXT_SRC_SCAN`] cycles; 404 when there is none,
/// 409 when the build has no pc-level source map.
pub async fn next_src(
    State(s): State<AppState>,
    Path(rid): Path<String>,
    Query(q): Query<NextSrcQuery>,
) -> ApiResult<Json<NextSrc>> {
    if q.dir != 1 && q.dir != -1 {
        return Err(AppError::BadRequest(format!(
            "dir must be 1 or -1, not {}",
            q.dir
        )));
    }
    let (rdir, jdir) = runs::open(&s, &rid)?;
    let t: Trace = runs::trace_of(&rdir)?;
    if q.from >= t.total {
        return Err(AppError::BadRequest(format!(
            "from {} is past the last cycle ({})",
            q.from,
            t.total.saturating_sub(1)
        )));
    }
    let (map, op_map) = maps_of(&jdir);
    let Some(map) = map else {
        return Err(AppError::Conflict(
            "this build has no pc-level source map".into(),
        ));
    };
    let found = tokio::task::spawn_blocking(move || scan_src(&t, &map, &op_map, q.from, q.dir))
        .await
        .map_err(|e| AppError::Internal(e.to_string()))?
        .map_err(runs::trace_err)?;
    match found {
        Some((clk, src_pc)) => Ok(Json(NextSrc { clk, src_pc })),
        None => Err(AppError::NotFound(format!(
            "no {} source op within {NEXT_SRC_SCAN} cycles",
            if q.dir == 1 { "later" } else { "earlier" }
        ))),
    }
}

fn scan_src(
    t: &Trace,
    map: &SourceMap,
    op_map: &[OpLine],
    from: u32,
    dir: i8,
) -> std::io::Result<Option<(u32, usize)>> {
    let map = Some(map.clone());
    let mut memo: std::collections::HashMap<u32, Option<usize>> = std::collections::HashMap::new();
    let mut src_of = |pc: u32| {
        *memo
            .entry(pc)
            .or_insert_with(|| c_line_for(&map, pc).and_then(|l| src_pc_for_line(op_map, l)))
    };
    let here = src_of(t.cycle(from)?.pc);
    let page = terminal_core::trace::MAX_PAGE;
    let mut scanned = 0u32;
    if dir == 1 {
        let mut at = from + 1;
        while at < t.total && scanned < NEXT_SRC_SCAN {
            let n = page.min(NEXT_SRC_SCAN - scanned);
            let cycles = t.cycles(at, n)?;
            for c in &cycles {
                if let Some(p) = src_of(c.pc) {
                    if Some(p) != here {
                        return Ok(Some((c.clk, p)));
                    }
                }
            }
            scanned += cycles.len() as u32;
            at += cycles.len() as u32;
        }
    } else {
        let mut end = from; // exclusive
        while end > 0 && scanned < NEXT_SRC_SCAN {
            let n = page.min(NEXT_SRC_SCAN - scanned).min(end);
            let cycles = t.cycles(end - n, n)?;
            for c in cycles.iter().rev() {
                if let Some(p) = src_of(c.pc) {
                    if Some(p) != here {
                        return Ok(Some((c.clk, p)));
                    }
                }
            }
            scanned += n;
            end -= n;
        }
    }
    Ok(None)
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
