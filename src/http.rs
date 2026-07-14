use super::AppState;
use axum::{
    extract::Request,
    middleware::{self, Next},
    response::Response,
    routing::{get, post},
    Router,
};
use std::sync::atomic::{AtomicU64, Ordering};
use tower_http::{services::ServeDir, trace::TraceLayer};

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
        .route("/", get(super::redirect_to_index))
        .route("/search", get(super::search))
        .route("/tags", get(super::tags_index))
        .route("/tags/{tag}", get(super::tag_filter))
        .route("/folders/{*path}", get(super::folder_view))
        .route("/preview", post(super::preview))
        .route("/api/v1/pages/{*path}", get(super::reader_page_api))
        .route(
            "/p/{*path}",
            get(super::page_handler).post(super::page_save),
        )
        .route("/events", get(super::events))
        .route("/healthz", get(super::healthz))
        .route("/readyz", get(super::readyz))
        .route("/metrics", get(super::metrics))
        .route("/api/v1/move", post(super::page_move))
        .route(
            "/api/v1/trash",
            post(super::page_trash).get(super::trash_list),
        )
        .route("/api/v1/trash/restore", post(super::trash_restore))
        .route("/api/v1/trash/purge", post(super::trash_purge))
        .route("/api/v1/promote-mention", post(super::promote_mention))
        .route("/api/v1/nav/children", get(super::nav_children_handler))
        .route("/api/v1/tags", get(super::tags_api))
        .route("/api/v1/quickswitch", get(super::quickswitch))
        .nest_service("/static", ServeDir::new("static"))
        .nest_service("/assets", ServeDir::new("miku_docs/assets"))
        .layer(TraceLayer::new_for_http().on_response(super::observe_http_response))
        .layer(middleware::from_fn(request_context))
        .with_state(state)
}
