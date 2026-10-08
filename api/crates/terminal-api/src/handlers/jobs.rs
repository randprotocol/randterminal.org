use crate::{error::ApiResult, jobs::JobEvent, state::AppState};
use axum::{
    extract::{Path, State},
    http::{header, StatusCode},
    response::{
        sse::{Event, KeepAlive, Sse},
        IntoResponse, Response,
    },
    Json,
};
use futures_util::stream::{self, Stream, StreamExt};
use serde_json::{json, Value};
use std::convert::Infallible;
use terminal_core::views::{JobView, NewJob};
use tokio::sync::broadcast::error::RecvError;

pub async fn create(
    State(s): State<AppState>,
    Json(req): Json<NewJob>,
) -> ApiResult<(StatusCode, Json<Value>)> {
    let id = s.store.create(req)?;
    Ok((StatusCode::CREATED, Json(json!({ "id": id }))))
}

pub async fn get(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<JobView>> {
    Ok(Json(s.store.view(&id)?))
}

pub async fn compile(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    Ok(Json(s.store.stage_view(&id, "compile")?))
}
pub async fn transpile(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Json<Value>> {
    Ok(Json(s.store.stage_view(&id, "transpile")?))
}
pub async fn image(State(s): State<AppState>, Path(id): Path<String>) -> ApiResult<Json<Value>> {
    Ok(Json(s.store.stage_view(&id, "image")?))
}

pub async fn file(
    State(s): State<AppState>,
    Path((id, name)): Path<(String, String)>,
) -> ApiResult<Response> {
    let (bytes, ty) = s.store.file(&id, &name)?;
    Ok((
        [
            (header::CONTENT_TYPE, ty),
            (header::CONTENT_DISPOSITION, "inline"),
        ],
        bytes,
    )
        .into_response())
}

fn event_name(e: &JobEvent) -> &'static str {
    match e {
        JobEvent::Stage { .. } => "stage",
        JobEvent::Console(_) => "console",
        JobEvent::Done => "done",
    }
}

fn done_event() -> Event {
    Event::default().event("done").data("{}")
}

/// An event carrying `v` as JSON; `None` when it cannot be serialized (the stream then ends).
fn json_event<T: serde::Serialize>(name: &str, v: &T) -> Option<Event> {
    Event::default().event(name).json_data(v).ok()
}

/// `job` snapshot, then `stage`/`console` events, then `done`; the stream ends after `done`.
pub async fn events(
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Sse<impl Stream<Item = Result<Event, Infallible>>>> {
    // Subscribe before reading the snapshot, so no change falls between the two.
    let rx = s.store.subscribe(&id);
    let view = s.store.view(&id)?;
    // A job with no live channel (finished, or from before a restart) gets the snapshot and `done`.
    let live = !view.finished && rx.is_some();
    let head = stream::once(async move { Ok(json_event("job", &view).unwrap_or_else(done_event)) });
    let tail = stream::unfold(
        (if live { rx } else { None }, s, id),
        |(rx, s, id)| async move {
            let mut rx = rx?;
            let ev = match rx.recv().await {
                Ok(JobEvent::Done) | Err(RecvError::Closed) => {
                    return Some((Ok(done_event()), (None, s, id)))
                }
                Ok(e) => json_event(event_name(&e), &e),
                // Events were missed: replace them with a fresh snapshot.
                Err(RecvError::Lagged(_)) => {
                    s.store.view(&id).ok().and_then(|v| json_event("job", &v))
                }
            };
            match ev {
                Some(ev) => Some((Ok(ev), (Some(rx), s, id))),
                None => Some((Ok(done_event()), (None, s, id))),
            }
        },
    );
    let done = stream::once(async { Ok(done_event()) });
    let body = if live {
        head.chain(tail).boxed()
    } else {
        head.chain(done).boxed()
    };
    Ok(Sse::new(body).keep_alive(KeepAlive::default()))
}
