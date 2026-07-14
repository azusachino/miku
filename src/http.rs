use super::AppState;
use axum::{
    routing::{get, post},
    Router,
};
use tower_http::{services::ServeDir, trace::TraceLayer};

pub(super) fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(super::redirect_to_index))
        .route("/search", get(super::search))
        .route("/tags", get(super::tags_index))
        .route("/tags/{tag}", get(super::tag_filter))
        .route("/folders/{*path}", get(super::folder_view))
        .route("/preview", post(super::preview))
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
        .route("/api/v1/quickswitch", get(super::quickswitch))
        .nest_service("/static", ServeDir::new("static"))
        .nest_service("/assets", ServeDir::new("miku_docs/assets"))
        .layer(TraceLayer::new_for_http().on_response(super::observe_http_response))
        .with_state(state)
}
