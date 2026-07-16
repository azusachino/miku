//! Read-only workspace API DTOs and handlers for `miku-web`.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use miku_app::{
    ApplicationError, FileNode, FileNodeKind, FileTreeRequest, NotePath, NoteRef, RelativePath,
    SaveNoteCommand,
};
use miku_domain::{workspace::NoteId, Backlink, SearchRequest, SearchScope};
use miku_vault::VaultDocument;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{AppError, AppState};

/// Workspace bootstrap payload for the separate browser frontend.
#[derive(Debug, Serialize, ToSchema)]
pub struct WorkspaceResponse {
    /// Publicly displayed vault root label.
    pub root: String,
    /// This release exposes no mutation routes in this API group.
    pub readonly: bool,
    /// Current durable and hot projection lifecycle state.
    pub index_phase: String,
    /// Number of source notes discovered in the vault.
    pub note_count: usize,
    /// Number of derived tree placements.
    pub placement_count: usize,
    /// Number of notes whose identity is currently derived from its path.
    pub generated_identity_count: usize,
}

/// Compact note identity used in tree, parent, and child responses.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct NoteSummary {
    /// Stable note identity.
    pub note_id: String,
    /// Canonical Markdown source path.
    pub path: String,
    /// Display title.
    pub title: String,
    /// Sibling ordering from frontmatter.
    pub order: Option<i64>,
    /// Whether this note still uses a path-derived generated identity.
    pub identity_generated: bool,
}

/// One visible placement in the tree.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TreeNode {
    /// Whether this node is a filesystem folder or Markdown document.
    pub kind: String,
    /// Stable placement identity derived from note and parent identities.
    pub placement_id: String,
    /// Stable note content identity.
    pub note_id: String,
    /// Parent note identity, absent for a root placement.
    pub parent_id: Option<String>,
    /// Note summary shown by the tree shell.
    pub note: NoteSummary,
    /// Whether a folder contains another level of entries.
    pub has_children: bool,
}

/// Tree response filtered to one parent, or root placements when absent.
#[derive(Debug, Serialize, ToSchema)]
pub struct TreeResponse {
    /// Requested parent filter.
    pub parent_id: Option<String>,
    /// Ordered visible placements.
    pub nodes: Vec<TreeNode>,
}

/// Full note payload returned to the editor/context panel.
#[derive(Debug, Serialize, ToSchema)]
pub struct NoteResponse {
    /// Stable note identity.
    pub note_id: String,
    /// Canonical Markdown source path.
    pub path: String,
    /// Display title.
    pub title: String,
    /// Markdown body without frontmatter delimiters.
    pub body: String,
    /// Workspace and user frontmatter as written by the source file.
    pub frontmatter: serde_json::Value,
    /// Optimistic read revision.
    pub revision: RevisionResponse,
    /// Whether the source still lacks an explicit identity.
    pub identity_generated: bool,
}

/// File revision exposed to the frontend for later conflict-safe writes.
#[derive(Debug, Deserialize, Serialize, ToSchema)]
pub struct RevisionResponse {
    /// SHA-256 digest of the complete source file.
    pub content_hash: String,
    /// Source modification time as Unix seconds.
    pub mtime: i64,
}

/// Context assembled for the Trilium-like note workspace.
#[derive(Debug, Serialize, ToSchema)]
pub struct ContextResponse {
    /// Selected note.
    pub note: NoteResponse,
    /// Parent notes declared by the selected note.
    pub parents: Vec<NoteSummary>,
    /// Direct child placements.
    pub children: Vec<TreeNode>,
    /// Indexed backlinks.
    pub backlinks: Vec<BacklinkResponse>,
}

/// A source note that links to the selected note.
#[derive(Debug, Serialize, ToSchema)]
pub struct BacklinkResponse {
    /// Source path.
    pub path: String,
    /// Source title.
    pub title: String,
}

/// Search query accepted by the workspace frontend.
#[derive(Debug, Deserialize, ToSchema)]
pub struct SearchQuery {
    /// Full-text query.
    pub q: String,
    /// Maximum result count, capped by the server.
    pub limit: Option<usize>,
    /// Search scope: all, title, or content.
    pub scope: Option<String>,
}

/// Search result returned by the read API.
#[derive(Debug, Serialize, ToSchema)]
pub struct SearchResult {
    /// Canonical Markdown source path.
    pub path: String,
    /// Display title.
    pub title: String,
    /// Backend-provided context excerpt.
    pub snippet: String,
}

/// Search response envelope.
#[derive(Debug, Serialize, ToSchema)]
pub struct SearchResponse {
    /// Normalized query sent to the index.
    pub query: String,
    /// Ranked results.
    pub results: Vec<SearchResult>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TagResponse {
    pub tag: String,
    pub count: i64,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TagNoteResponse {
    pub path: String,
    pub title: String,
    pub mtime: i64,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SaveNoteRequest {
    pub body: String,
    pub title: String,
    pub expected_revision: RevisionResponse,
}

/// Returns workspace bootstrap metadata.
#[utoipa::path(get, path = "/api/v1/workspace", responses((status = 200, body = WorkspaceResponse)))]
pub async fn workspace(State(state): State<AppState>) -> Result<Json<WorkspaceResponse>, AppError> {
    let info = state
        .application
        .vault_info()
        .await
        .map_err(application_error)?;
    Ok(Json(WorkspaceResponse {
        root: info.root,
        readonly: info.readonly,
        index_phase: format!("{:?}", info.index_phase),
        note_count: info.note_count,
        placement_count: info.note_count,
        generated_identity_count: info.generated_identity_count,
    }))
}

/// Returns root or parent-filtered tree placements.
#[utoipa::path(get, path = "/api/v1/tree", params(("parent_id" = Option<String>, Query)), responses((status = 200, body = TreeResponse)))]
pub async fn tree(
    Query(query): Query<TreeQuery>,
    State(state): State<AppState>,
) -> Result<Json<TreeResponse>, AppError> {
    let folder = query
        .folder
        .as_deref()
        .map(RelativePath::new)
        .transpose()
        .map_err(application_error)?
        .unwrap_or_else(RelativePath::root);
    let tree = state
        .application
        .file_tree(FileTreeRequest { folder })
        .await
        .map_err(application_error)?;
    Ok(Json(TreeResponse {
        parent_id: query.parent_id.clone(),
        nodes: tree.nodes.into_iter().map(tree_node).collect(),
    }))
}

/// Returns one note by stable identity.
#[utoipa::path(get, path = "/api/v1/notes/{id}", params(("id" = String, Path)), responses((status = 200, body = NoteResponse), (status = 404)))]
pub async fn note(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<NoteResponse>, AppError> {
    let document = state
        .application
        .read_note(note_ref(&id).map_err(application_error)?)
        .await
        .map_err(application_error)?;
    Ok(Json(note_response(&document)))
}

#[utoipa::path(put, path = "/api/v1/notes/{id}", params(("id" = String, Path)), request_body = SaveNoteRequest, responses((status = 200, body = NoteResponse), (status = 403), (status = 404), (status = 409)))]
pub async fn save_note(
    Path(id): Path<String>,
    State(state): State<AppState>,
    Json(request): Json<SaveNoteRequest>,
) -> Result<Json<NoteResponse>, AppError> {
    let expected_revision = miku_domain::workspace::RevisionToken::new(
        request.expected_revision.content_hash,
        request.expected_revision.mtime,
    )?;
    let document = state
        .application
        .save_note(SaveNoteCommand {
            note: note_ref(&id).map_err(application_error)?,
            title: request.title,
            body: request.body,
            expected_revision,
        })
        .await
        .map_err(application_error)?;
    Ok(Json(note_response(&document)))
}

/// Returns note, parents, children, and indexed backlinks.
#[utoipa::path(get, path = "/api/v1/note-context/{id}", params(("id" = String, Path)), responses((status = 200, body = ContextResponse), (status = 404)))]
pub async fn note_context(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ContextResponse>, AppError> {
    let context = state
        .application
        .note_context(note_ref(&id).map_err(application_error)?)
        .await
        .map_err(application_error)?;
    Ok(Json(ContextResponse {
        note: note_response(&context.note),
        parents: context.parents.into_iter().map(note_summary_node).collect(),
        children: context.children.into_iter().map(tree_node).collect(),
        backlinks: context
            .backlinks
            .into_iter()
            .map(backlink_response)
            .collect(),
    }))
}

/// Returns direct children for one note.
#[utoipa::path(get, path = "/api/v1/note-children/{id}", params(("id" = String, Path)), responses((status = 200, body = TreeResponse), (status = 404)))]
pub async fn note_children(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<TreeResponse>, AppError> {
    let context = state
        .application
        .note_context(note_ref(&id).map_err(application_error)?)
        .await
        .map_err(application_error)?;
    Ok(Json(TreeResponse {
        parent_id: Some(id.clone()),
        nodes: context.children.into_iter().map(tree_node).collect(),
    }))
}

/// Searches the current read projection.
#[utoipa::path(get, path = "/api/v1/search", params(("q" = String, Query), ("limit" = Option<usize>, Query)), responses((status = 200, body = SearchResponse)))]
pub async fn search(
    Query(query): Query<SearchQuery>,
    State(state): State<AppState>,
) -> Result<Json<SearchResponse>, AppError> {
    let query_text = query.q.trim().to_string();
    let limit = query.limit.unwrap_or(50).clamp(1, 100);
    let scope = match query.scope.as_deref() {
        Some("title") => SearchScope::Title,
        Some("content" | "body") => SearchScope::Body,
        _ => SearchScope::All,
    };
    let hits = state
        .application
        .search(SearchRequest {
            query: query_text.clone(),
            scope,
            limit,
        })
        .await
        .map_err(application_error)?;
    Ok(Json(SearchResponse {
        query: query_text,
        results: hits
            .into_iter()
            .map(|hit| SearchResult {
                path: hit.path,
                title: hit.title,
                snippet: hit.snippet,
            })
            .collect(),
    }))
}

#[utoipa::path(get, path = "/api/v1/tags", responses((status = 200, body = [TagResponse])))]
pub async fn tags(State(state): State<AppState>) -> Result<Json<Vec<TagResponse>>, AppError> {
    let tags = state.application.tags().await.map_err(application_error)?;
    Ok(Json(
        tags.into_iter()
            .map(|tag| TagResponse {
                tag: tag.tag,
                count: tag.count,
            })
            .collect(),
    ))
}

#[utoipa::path(get, path = "/api/v1/tags/{tag}/notes", params(("tag" = String, Path)), responses((status = 200, body = [TagNoteResponse])))]
pub async fn tag_notes(
    Path(tag): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<Vec<TagNoteResponse>>, AppError> {
    let tag = tag.trim_start_matches('#').to_string();
    let notes = state
        .application
        .notes_with_tag(tag)
        .await
        .map_err(application_error)?;
    Ok(Json(
        notes
            .into_iter()
            .map(|note| TagNoteResponse {
                path: note.path,
                title: note.title,
                mtime: note.mtime,
            })
            .collect(),
    ))
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct TreeQuery {
    /// Relative folder path; omitted means the vault root.
    pub folder: Option<String>,
    /// Deprecated note-parent filter retained for compatibility.
    pub parent_id: Option<String>,
}

fn application_error(error: ApplicationError) -> AppError {
    match error {
        ApplicationError::Readonly => AppError::forbidden(anyhow::anyhow!(error)),
        ApplicationError::Conflict => AppError::conflict(anyhow::anyhow!(error)),
        ApplicationError::NotFound(_) | ApplicationError::InvalidPath(_) => {
            AppError::not_found(anyhow::anyhow!(error))
        }
        error => AppError::from(anyhow::anyhow!(error)),
    }
}

fn tree_node(node: FileNode) -> TreeNode {
    let note_id = node
        .note_id
        .as_ref()
        .map(|id| id.as_str().to_string())
        .unwrap_or_else(|| node.path.as_str().to_string());
    let title = node.title.clone().unwrap_or_else(|| node.name.clone());
    TreeNode {
        kind: match node.kind {
            FileNodeKind::Folder => "folder".to_string(),
            FileNodeKind::Markdown => "markdown".to_string(),
            _ => "unknown".to_string(),
        },
        placement_id: format!("path:{}", node.path),
        note_id,
        parent_id: None,
        note: NoteSummary {
            note_id: node_id(&node),
            path: node.path.as_str().to_string(),
            title,
            order: None,
            identity_generated: node.identity_generated,
        },
        has_children: node.has_children,
    }
}

fn node_id(node: &FileNode) -> String {
    node.note_id
        .as_ref()
        .map(|id| id.as_str().to_string())
        .unwrap_or_else(|| node.path.as_str().to_string())
}

fn note_summary_node(node: FileNode) -> NoteSummary {
    NoteSummary {
        note_id: node_id(&node),
        path: node.path.as_str().to_string(),
        title: node.title.unwrap_or(node.name),
        order: None,
        identity_generated: node.identity_generated,
    }
}

fn note_ref(id: &str) -> Result<NoteRef, ApplicationError> {
    if id.ends_with(".md") {
        Ok(NoteRef::Path(NotePath::new(id.to_string())?))
    } else {
        Ok(NoteRef::Id(
            NoteId::new(id.to_string()).map_err(ApplicationError::Workspace)?,
        ))
    }
}

fn note_response(document: &VaultDocument) -> NoteResponse {
    let mut frontmatter = serde_json::Map::new();
    for (key, value) in &document.note.properties {
        frontmatter.insert(key.clone(), value.clone());
    }
    frontmatter.insert(
        "id".to_string(),
        serde_json::Value::String(document.note.id.as_str().to_string()),
    );
    frontmatter.insert(
        "parents".to_string(),
        serde_json::Value::Array(
            document
                .note
                .parents
                .iter()
                .map(|parent| serde_json::Value::String(parent.as_str().to_string()))
                .collect(),
        ),
    );
    if let Some(order) = document.note.order {
        frontmatter.insert("order".to_string(), serde_json::Value::Number(order.into()));
    }
    NoteResponse {
        note_id: document.note.id.as_str().to_string(),
        path: document.note.source_path.clone(),
        title: document.note.title.clone(),
        body: document.body.clone(),
        frontmatter: serde_json::Value::Object(frontmatter),
        revision: RevisionResponse {
            content_hash: document.revision.content_hash.clone(),
            mtime: document.revision.mtime,
        },
        identity_generated: document.identity_generated,
    }
}

fn backlink_response(backlink: Backlink) -> BacklinkResponse {
    BacklinkResponse {
        path: backlink.path,
        title: backlink.title,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use miku_domain::workspace::{Note, NoteId, RevisionToken};
    use std::collections::BTreeMap;

    fn document(id: &str, path: &str, parents: Vec<NoteId>, order: Option<i64>) -> VaultDocument {
        VaultDocument {
            note: Note::new(
                NoteId::new(id).unwrap(),
                path,
                id,
                parents,
                order,
                BTreeMap::new(),
            )
            .unwrap(),
            body: String::new(),
            revision: RevisionToken::new("hash", 1).unwrap(),
            identity_generated: false,
        }
    }

    #[test]
    fn note_response_contains_workspace_frontmatter_and_revision() {
        let document = document("n1", "Notes/N1.md", Vec::new(), Some(3));
        let response = note_response(&document);
        assert_eq!(response.frontmatter["id"], "n1");
        assert_eq!(response.frontmatter["order"], 3);
        assert_eq!(response.revision.content_hash, "hash");
    }
}
