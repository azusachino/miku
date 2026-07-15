//! Read-only workspace API DTOs and handlers for `miku-web`.

use axum::{
    extract::{Path, Query, State},
    Json,
};
use miku_domain::{Backlink, SearchRequest, SearchScope};
use miku_vault::VaultDocument;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use utoipa::ToSchema;

use crate::{AppError, AppState};

/// Workspace bootstrap payload for the separate browser frontend.
#[derive(Debug, Serialize, ToSchema)]
pub struct WorkspaceResponse {
    /// Publicly displayed vault root label.
    pub root: String,
    /// This release exposes no mutation routes in this API group.
    pub readonly: bool,
    /// Number of source notes discovered in the vault.
    pub note_count: usize,
    /// Number of derived tree placements.
    pub placement_count: usize,
    /// Number of path-addressed legacy notes awaiting migration.
    pub legacy_count: usize,
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
    /// Whether this note still uses a path-derived legacy identity.
    pub legacy: bool,
}

/// One visible placement in the tree.
#[derive(Debug, Clone, Serialize, ToSchema)]
pub struct TreeNode {
    /// Stable placement identity derived from note and parent identities.
    pub placement_id: String,
    /// Stable note content identity.
    pub note_id: String,
    /// Parent note identity, absent for a root placement.
    pub parent_id: Option<String>,
    /// Note summary shown by the tree shell.
    pub note: NoteSummary,
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
    /// Whether migration has not yet written a stable ID.
    pub legacy: bool,
}

/// File revision exposed to the frontend for later conflict-safe writes.
#[derive(Debug, Serialize, ToSchema)]
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

/// Returns workspace bootstrap metadata.
#[utoipa::path(get, path = "/api/v1/workspace", responses((status = 200, body = WorkspaceResponse)))]
pub async fn workspace(State(state): State<AppState>) -> Result<Json<WorkspaceResponse>, AppError> {
    let documents = scan(&state)?;
    let placement_count = documents.iter().map(placement_count).sum();
    Ok(Json(WorkspaceResponse {
        root: state.vault.root().display().to_string(),
        readonly: true,
        note_count: documents.len(),
        placement_count,
        legacy_count: documents.iter().filter(|document| document.legacy).count(),
    }))
}

/// Returns root or parent-filtered tree placements.
#[utoipa::path(get, path = "/api/v1/tree", params(("parent_id" = Option<String>, Query)), responses((status = 200, body = TreeResponse)))]
pub async fn tree(
    Query(query): Query<TreeQuery>,
    State(state): State<AppState>,
) -> Result<Json<TreeResponse>, AppError> {
    let documents = scan(&state)?;
    Ok(Json(TreeResponse {
        parent_id: query.parent_id.clone(),
        nodes: tree_nodes(&documents, query.parent_id.as_deref()),
    }))
}

/// Returns one note by stable identity.
#[utoipa::path(get, path = "/api/v1/notes/{id}", params(("id" = String, Path)), responses((status = 200, body = NoteResponse), (status = 404)))]
pub async fn note(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<NoteResponse>, AppError> {
    let document = scan(&state)?
        .into_iter()
        .find(|document| document.note.id.as_str() == id)
        .ok_or_else(|| AppError::not_found(anyhow::anyhow!("note not found: {id}")))?;
    Ok(Json(note_response(&document)))
}

/// Returns note, parents, children, and indexed backlinks.
#[utoipa::path(get, path = "/api/v1/notes/{id}/context", params(("id" = String, Path)), responses((status = 200, body = ContextResponse), (status = 404)))]
pub async fn note_context(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<ContextResponse>, AppError> {
    let documents = scan(&state)?;
    let document = documents
        .iter()
        .find(|document| document.note.id.as_str() == id)
        .ok_or_else(|| AppError::not_found(anyhow::anyhow!("note not found: {id}")))?;
    let parents = document
        .note
        .parents
        .iter()
        .filter_map(|parent_id| {
            documents
                .iter()
                .find(|candidate| candidate.note.id == *parent_id)
        })
        .map(note_summary)
        .collect();
    let backlinks = state
        .index
        .backlinks(&document.note.source_path)
        .await
        .map_err(|error| anyhow::anyhow!(error))?
        .into_iter()
        .map(backlink_response)
        .collect();
    Ok(Json(ContextResponse {
        note: note_response(document),
        parents,
        children: tree_nodes(&documents, Some(document.note.id.as_str())),
        backlinks,
    }))
}

/// Returns direct children for one note.
#[utoipa::path(get, path = "/api/v1/notes/{id}/children", params(("id" = String, Path)), responses((status = 200, body = TreeResponse), (status = 404)))]
pub async fn note_children(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<Json<TreeResponse>, AppError> {
    let documents = scan(&state)?;
    if !documents
        .iter()
        .any(|document| document.note.id.as_str() == id)
    {
        return Err(AppError::not_found(anyhow::anyhow!("note not found: {id}")));
    }
    Ok(Json(TreeResponse {
        parent_id: Some(id.clone()),
        nodes: tree_nodes(&documents, Some(&id)),
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
    let hits = state
        .index
        .search(SearchRequest {
            query: query_text.clone(),
            scope: SearchScope::All,
            limit,
        })
        .await
        .map_err(|error| anyhow::anyhow!(error))?;
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

#[derive(Debug, Deserialize, ToSchema)]
pub struct TreeQuery {
    /// Parent note identity; omitted means root placements.
    pub parent_id: Option<String>,
}

fn scan(state: &AppState) -> Result<Vec<VaultDocument>, AppError> {
    state
        .vault
        .scan()
        .map_err(|error| anyhow::anyhow!(error))
        .map_err(AppError::from)
}

fn placement_count(document: &VaultDocument) -> usize {
    document.note.parents.len().max(1)
}

fn tree_nodes(documents: &[VaultDocument], parent_id: Option<&str>) -> Vec<TreeNode> {
    let mut nodes = documents
        .iter()
        .flat_map(|document| {
            let parents = if document.note.parents.is_empty() {
                vec![None]
            } else {
                document
                    .note
                    .parents
                    .iter()
                    .map(|parent| Some(parent.as_str()))
                    .collect()
            };
            parents.into_iter().filter_map(move |candidate_parent| {
                if candidate_parent != parent_id {
                    return None;
                }
                Some(TreeNode {
                    placement_id: placement_identity(document.note.id.as_str(), candidate_parent),
                    note_id: document.note.id.as_str().to_string(),
                    parent_id: candidate_parent.map(ToOwned::to_owned),
                    note: note_summary(document),
                })
            })
        })
        .collect::<Vec<_>>();
    nodes.sort_by(|left, right| {
        left.note
            .order
            .unwrap_or(i64::MAX)
            .cmp(&right.note.order.unwrap_or(i64::MAX))
            .then_with(|| left.note.path.cmp(&right.note.path))
    });
    nodes
}

fn note_summary(document: &VaultDocument) -> NoteSummary {
    NoteSummary {
        note_id: document.note.id.as_str().to_string(),
        path: document.note.source_path.clone(),
        title: document.note.title.clone(),
        order: document.note.order,
        legacy: document.legacy,
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
        legacy: document.legacy,
    }
}

fn backlink_response(backlink: Backlink) -> BacklinkResponse {
    BacklinkResponse {
        path: backlink.path,
        title: backlink.title,
    }
}

fn placement_identity(note_id: &str, parent_id: Option<&str>) -> String {
    let mut hasher = Sha256::new();
    hasher.update(parent_id.unwrap_or("root").as_bytes());
    hasher.update([0]);
    hasher.update(note_id.as_bytes());
    format!("placement-{:x}", hasher.finalize())
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
            legacy: false,
        }
    }

    #[test]
    fn tree_exposes_root_and_cloned_parent_placements() {
        let root = document("root", "Root.md", Vec::new(), Some(0));
        let child = document(
            "child",
            "Child.md",
            vec![root.note.id.clone(), NoteId::new("other").unwrap()],
            Some(1),
        );

        assert_eq!(tree_nodes(&[root.clone(), child.clone()], None).len(), 1);
        assert_eq!(tree_nodes(&[root, child], Some("root")).len(), 1);
        assert_ne!(
            placement_identity("child", Some("root")),
            placement_identity("child", Some("other"))
        );
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
