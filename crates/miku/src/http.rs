use super::{healthz, http_api, metrics, openapi, readyz, AppState};
use axum::{
    extract::{Request, State},
    middleware,
    response::{
        sse::{self, KeepAlive, Sse},
        Response,
    },
    routing::get,
    Router,
};
use std::sync::atomic::{AtomicU64, Ordering};
use tokio_stream::{wrappers::BroadcastStream, Stream, StreamExt};
use tower_http::trace::TraceLayer;

static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

async fn request_context(mut request: Request, next: middleware::Next) -> Response {
    let request_id = request
        .headers()
        .get("x-miku-request-id")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| format!("miku-{}", REQUEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)));
    request.headers_mut().insert(
        "x-miku-request-id",
        request_id.parse().expect("request id is valid ASCII"),
    );
    let span = tracing::info_span!("http_request", request_id = %request_id, method = %request.method(), uri = %request.uri());
    let _entered = span.enter();
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        "x-miku-request-id",
        request_id.parse().expect("request id is valid ASCII"),
    );
    tracing::info!(status = %response.status(), "request completed");
    response
}

async fn events(
    State(state): axum::extract::State<AppState>,
) -> Sse<impl Stream<Item = Result<sse::Event, std::convert::Infallible>>> {
    let stream = BroadcastStream::new(state.events.subscribe())
        .filter_map(|item| item.ok().map(|path| Ok(sse::Event::default().data(path))));
    Sse::new(stream).keep_alive(KeepAlive::default())
}

async fn root() -> axum::Json<serde_json::Value> {
    axum::Json(serde_json::json!({
        "service": "miku-api",
        "frontend": "http://127.0.0.1:5173/",
        "api": "/api/v1",
        "health": "/healthz",
    }))
}

pub(super) fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(root))
        .route("/events", get(events))
        .route("/healthz", get(healthz))
        .route("/readyz", get(readyz))
        .route("/metrics", get(metrics))
        .route("/api/v1/workspace", get(http_api::workspace))
        .route("/api/v1/tree", get(http_api::tree))
        .route(
            "/api/v1/notes/{*id}",
            get(http_api::note).put(http_api::save_note),
        )
        .route("/api/v1/note-context/{*id}", get(http_api::note_context))
        .route("/api/v1/note-children/{*id}", get(http_api::note_children))
        .route("/api/v1/search", get(http_api::search))
        .route("/api/v1/tags", get(http_api::tags))
        .route("/api/v1/tags/{tag}/notes", get(http_api::tag_notes))
        .route("/api/openapi.json", get(openapi::json))
        .layer(TraceLayer::new_for_http().on_response(super::observe_http_response))
        .layer(middleware::from_fn(request_context))
        .with_state(state)
}
