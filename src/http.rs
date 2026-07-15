use super::routes;
use super::{AppState, StaticAssets};
use axum::{
    extract::{Path, Request},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Response},
    routing::{get, post},
    Router,
};
use std::sync::atomic::{AtomicU64, Ordering};
use tower_http::{services::ServeDir, trace::TraceLayer};

// Serve a static asset baked into the binary (see StaticAssets in main.rs). The
// /assets route stays a filesystem ServeDir — those are vault content, not
// shipped assets.
async fn static_asset(Path(path): Path<String>) -> Response {
    match StaticAssets::get(&path) {
        Some(file) => {
            let mime = mime_guess::from_path(&path).first_or_octet_stream();
            ([(header::CONTENT_TYPE, mime.as_ref())], file.data).into_response()
        }
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

static REQUEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

async fn request_context(mut request: Request, next: Next) -> Response {
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
    let span = tracing::info_span!(
        "http_request",
        request_id = %request_id,
        method = %request.method(),
        uri = %request.uri()
    );
    let _entered = span.enter();
    let mut response = next.run(request).await;
    response.headers_mut().insert(
        "x-miku-request-id",
        request_id.parse().expect("request id is valid ASCII"),
    );
    tracing::info!(status = %response.status(), "request completed");
    response
}

pub(super) fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(routes::redirect_to_index))
        .route("/search", get(routes::search))
        .route("/tags", get(routes::tags_index))
        .route("/tags/{tag}", get(routes::tag_filter))
        .route("/folders/{*path}", get(routes::folder_view))
        .route("/preview", post(routes::preview))
        .route("/api/v1/pages/{*path}", get(routes::reader_page_api))
        .route(
            "/p/{*path}",
            get(routes::page_handler).post(routes::page_save),
        )
        .route("/events", get(routes::events))
        .route("/healthz", get(super::healthz))
        .route("/readyz", get(super::readyz))
        .route("/metrics", get(super::metrics))
        .route("/api/v1/promote-mention", post(routes::promote_mention))
        .route("/api/v1/nav/children", get(routes::nav_children_handler))
        .route("/api/v1/tags", get(routes::tags_api))
        .route("/api/v1/tags/{tag}/pages", get(routes::tag_pages_api))
        .route("/api/v1/quickswitch", get(routes::quickswitch))
        .route("/api/v1/content-search", get(routes::content_search_api))
        .route("/static/{*path}", get(static_asset))
        .nest_service("/assets", ServeDir::new("miku_docs/assets"))
        .layer(TraceLayer::new_for_http().on_response(super::observe_http_response))
        .layer(middleware::from_fn(request_context))
        .with_state(state)
}
