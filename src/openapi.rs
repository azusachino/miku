//! Generated `OpenAPI` contract for the read-only workspace API.

use utoipa::OpenApi;

use crate::workspace_api;

/// `OpenAPI` document consumed by `miku-web` type generation.
#[derive(OpenApi)]
#[openapi(
    info(
        title = "Miku Workspace API",
        version = "0.0.3",
        description = "Read-only workspace contract for the file-backed Markdown UI"
    ),
    paths(
        workspace_api::workspace,
        workspace_api::tree,
        workspace_api::note,
        workspace_api::note_context,
        workspace_api::note_children,
        workspace_api::search
    ),
    components(schemas(
        workspace_api::WorkspaceResponse,
        workspace_api::NoteSummary,
        workspace_api::TreeNode,
        workspace_api::TreeResponse,
        workspace_api::NoteResponse,
        workspace_api::RevisionResponse,
        workspace_api::ContextResponse,
        workspace_api::BacklinkResponse,
        workspace_api::SearchQuery,
        workspace_api::SearchResult,
        workspace_api::SearchResponse,
        workspace_api::TreeQuery
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
        assert!(paths.contains_key("/api/v1/notes/{id}/context"));
        assert!(paths.contains_key("/api/v1/search"));
        assert!(paths.values().all(|path| path.post.is_none()));
    }
}
