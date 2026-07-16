//! Generated `OpenAPI` contract for the read-only workspace API.

use utoipa::OpenApi;

use crate::http_api;

/// `OpenAPI` document consumed by `miku-web` type generation.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Miku Workspace API",
        version = "0.0.3",
        description = "Read-only workspace contract for the file-backed Markdown UI"
    ),
    paths(
        http_api::workspace,
        http_api::tree,
        http_api::note,
        http_api::save_note,
        http_api::note_context,
        http_api::note_children,
        http_api::search,
        http_api::tags,
        http_api::tag_notes
    ),
    components(schemas(
        http_api::WorkspaceResponse,
        http_api::NoteSummary,
        http_api::TreeNode,
        http_api::TreeResponse,
        http_api::NoteResponse,
        http_api::RevisionResponse,
        http_api::SaveNoteRequest,
        http_api::ContextResponse,
        http_api::BacklinkResponse,
        http_api::SearchQuery,
        http_api::SearchResult,
        http_api::SearchResponse,
        http_api::TagResponse,
        http_api::TagNoteResponse,
        http_api::TreeQuery
    ))
)]
pub struct ApiDoc;

/// Serves the generated `OpenAPI` JSON document.
pub async fn json() -> impl axum::response::IntoResponse {
    (
        [(axum::http::header::CONTENT_TYPE, "application/json")],
        ApiDoc::openapi()
            .to_json()
            .expect("generated OpenAPI document must serialize"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn contract_contains_only_read_workspace_surface() {
        let document = ApiDoc::openapi();
        let paths = &document.paths.paths;
        assert!(paths.contains_key("/api/v1/workspace"));
        assert!(paths.contains_key("/api/v1/tree"));
        assert!(paths.contains_key("/api/v1/note-context/{id}"));
        assert!(paths.contains_key("/api/v1/search"));
        assert!(paths.values().all(|path| path.post.is_none()));
    }
}
