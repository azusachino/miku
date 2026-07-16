//! Stable application ports shared by HTTP, CLI, and future frontends.
//!
//! These traits are intentionally unversioned. `/api/v1` is an HTTP adapter
//! version; it must not leak into the application contract.

use async_trait::async_trait;
use miku_domain::{
    workspace::{NoteId, RevisionToken, WorkspaceError},
    Backlink, IndexCapabilities, PageSummary, SearchHit, SearchRequest, StoreError, TagCount,
};
use miku_vault::{VaultDocument, VaultError};
use serde::{Deserialize, Serialize};
use std::fmt;
use thiserror::Error;

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct RelativePath(String);

impl RelativePath {
    pub fn new(path: impl Into<String>) -> Result<Self, ApplicationError> {
        let path = path.into().replace('\\', "/");
        if path.starts_with('/')
            || path.trim().is_empty()
            || path
                .split('/')
                .next()
                .is_some_and(|component| component.ends_with(':'))
        {
            return Err(ApplicationError::InvalidPath(path));
        }
        let normalized = path.trim_end_matches('/');
        if normalized.is_empty()
            || normalized
                .split('/')
                .any(|part| part.is_empty() || part == "." || part == "..")
        {
            return Err(ApplicationError::InvalidPath(path));
        }
        Ok(Self(normalized.to_string()))
    }

    pub fn root() -> Self {
        Self(String::new())
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    #[must_use]
    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Display for RelativePath {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub struct NotePath(RelativePath);

impl NotePath {
    pub fn new(path: impl Into<String>) -> Result<Self, ApplicationError> {
        let path = RelativePath::new(path)?;
        if !path.as_str().ends_with(".md") {
            return Err(ApplicationError::InvalidPath(path.to_string()));
        }
        Ok(Self(path))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

#[non_exhaustive]
#[derive(Debug, Clone, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum NoteRef {
    /// Canonical relative Markdown path. This is the default web identity.
    Path(NotePath),
    /// Explicit frontmatter identity used by integrations and migrations.
    Id(NoteId),
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum IndexPhase {
    Starting,
    Indexing,
    Ready,
    Degraded,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct VaultInfo {
    pub root: String,
    pub readonly: bool,
    pub index_phase: IndexPhase,
    pub capabilities: IndexCapabilities,
    pub note_count: usize,
    pub generated_identity_count: usize,
    pub first_note: Option<NotePath>,
}

#[non_exhaustive]
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum FileNodeKind {
    Folder,
    Markdown,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FileNode {
    pub kind: FileNodeKind,
    pub path: RelativePath,
    pub note_id: Option<NoteId>,
    pub identity_generated: bool,
    pub name: String,
    pub title: Option<String>,
    pub has_children: bool,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FileTreeRequest {
    pub folder: RelativePath,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct FileTree {
    pub folder: RelativePath,
    pub nodes: Vec<FileNode>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct SaveNoteCommand {
    pub note: NoteRef,
    pub title: String,
    pub body: String,
    pub expected_revision: RevisionToken,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct NoteContext {
    pub note: VaultDocument,
    pub parents: Vec<FileNode>,
    pub children: Vec<FileNode>,
    pub backlinks: Vec<Backlink>,
}

#[non_exhaustive]
#[derive(Debug, Error)]
pub enum ApplicationError {
    #[error("invalid relative path: {0}")]
    InvalidPath(String),
    #[error("workspace operation failed: {0}")]
    Workspace(#[from] WorkspaceError),
    #[error("index operation failed: {0}")]
    Index(#[from] StoreError),
    #[error("vault operation failed: {0}")]
    Vault(#[from] VaultError),
    #[error("note not found: {0}")]
    NotFound(String),
    #[error("workspace is readonly")]
    Readonly,
    #[error("note changed on disk")]
    Conflict,
}

#[async_trait]
pub trait VaultReader: Send + Sync {
    async fn vault_info(&self) -> Result<VaultInfo, ApplicationError>;
    async fn file_tree(&self, request: FileTreeRequest) -> Result<FileTree, ApplicationError>;
    async fn read_note(&self, note: NoteRef) -> Result<VaultDocument, ApplicationError>;
    async fn note_context(&self, note: NoteRef) -> Result<NoteContext, ApplicationError>;
}

#[async_trait]
pub trait VaultWriter: Send + Sync {
    async fn save_note(&self, command: SaveNoteCommand) -> Result<VaultDocument, ApplicationError>;
}

/// Filesystem-owned document source rooted at `miku_docs`.
pub trait DocumentSource: VaultReader + VaultWriter {}

impl<T> DocumentSource for T where T: VaultReader + VaultWriter {}

#[async_trait]
pub trait SearchReader: Send + Sync {
    async fn search(&self, request: SearchRequest) -> Result<Vec<SearchHit>, ApplicationError>;
}

#[async_trait]
pub trait TagReader: Send + Sync {
    async fn tags(&self) -> Result<Vec<TagCount>, ApplicationError>;
    async fn notes_with_tag(&self, tag: String) -> Result<Vec<PageSummary>, ApplicationError>;
}

/// Universal Miku application port. Concrete storage and indexing remain
/// behind this trait; transports adapt this port to their own versioned API.
pub trait MikuApplication: VaultReader + VaultWriter + SearchReader + TagReader {}

impl<T> MikuApplication for T where T: VaultReader + VaultWriter + SearchReader + TagReader {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn note_path(path: &str) -> NotePath {
        NotePath::new(path).expect("valid note path")
    }

    #[test]
    fn paths_are_safe_and_canonical() {
        assert_eq!(
            RelativePath::new("Projects\\Miku/").unwrap().as_str(),
            "Projects/Miku"
        );
        assert!(RelativePath::new("/Projects/Miku").is_err());
        assert!(RelativePath::new("C:/Projects/Miku").is_err());
        assert!(RelativePath::new("../escape").is_err());
        assert!(RelativePath::new("Projects/./Miku").is_err());
        assert!(NotePath::new("Projects/Miku").is_err());
        assert_eq!(note_path("Projects/Miku.md").as_str(), "Projects/Miku.md");
    }

    #[test]
    fn application_port_is_object_safe() {
        fn accept_port(_application: &dyn MikuApplication) {}
        fn accept_shared_port(_application: Arc<dyn MikuApplication>) {}

        struct Fake;
        #[async_trait]
        impl VaultReader for Fake {
            async fn vault_info(&self) -> Result<VaultInfo, ApplicationError> {
                Ok(VaultInfo {
                    root: "fake".into(),
                    readonly: true,
                    index_phase: IndexPhase::Ready,
                    capabilities: IndexCapabilities {
                        durable: false,
                        full_text_search: true,
                        fuzzy_page_search: false,
                        transactions: true,
                        remote_sync: false,
                    },
                    note_count: 1,
                    generated_identity_count: 0,
                    first_note: Some(note_path("Index.md")),
                })
            }

            async fn file_tree(
                &self,
                _request: FileTreeRequest,
            ) -> Result<FileTree, ApplicationError> {
                Ok(FileTree {
                    folder: RelativePath::root(),
                    nodes: Vec::new(),
                })
            }

            async fn read_note(&self, _note: NoteRef) -> Result<VaultDocument, ApplicationError> {
                Err(ApplicationError::NotFound("fake".into()))
            }

            async fn note_context(&self, _note: NoteRef) -> Result<NoteContext, ApplicationError> {
                Err(ApplicationError::NotFound("fake".into()))
            }
        }

        #[async_trait]
        impl VaultWriter for Fake {
            async fn save_note(
                &self,
                _command: SaveNoteCommand,
            ) -> Result<VaultDocument, ApplicationError> {
                Err(ApplicationError::Readonly)
            }
        }

        #[async_trait]
        impl SearchReader for Fake {
            async fn search(
                &self,
                _request: SearchRequest,
            ) -> Result<Vec<SearchHit>, ApplicationError> {
                Ok(Vec::new())
            }
        }

        #[async_trait]
        impl TagReader for Fake {
            async fn tags(&self) -> Result<Vec<TagCount>, ApplicationError> {
                Ok(Vec::new())
            }

            async fn notes_with_tag(
                &self,
                _tag: String,
            ) -> Result<Vec<PageSummary>, ApplicationError> {
                Ok(Vec::new())
            }
        }

        accept_port(&Fake);
        accept_shared_port(Arc::new(Fake));
    }

    #[test]
    fn write_failures_are_distinguishable() {
        assert_eq!(
            ApplicationError::Readonly.to_string(),
            "workspace is readonly"
        );
        assert_eq!(
            ApplicationError::Conflict.to_string(),
            "note changed on disk"
        );
        assert!(matches!(
            ApplicationError::Workspace(WorkspaceError::EmptyRevisionHash),
            ApplicationError::Workspace(WorkspaceError::EmptyRevisionHash)
        ));
    }
}
