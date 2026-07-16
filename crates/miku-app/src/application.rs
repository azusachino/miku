//! Concrete file-backed implementation of the universal application port.

use crate::{
    ApplicationError, FileNode, FileNodeKind, FileTree, FileTreeRequest, IndexApi, IndexPhase,
    NoteContext, NoteRef, RelativePath, SaveNoteCommand, SearchReader, TagReader, VaultInfo,
    VaultReader, VaultWriter, WorkspaceService, WorkspaceServiceError,
};
use async_trait::async_trait;
use miku_domain::{workspace::NoteId, SearchHit, SearchRequest};
use miku_vault::{Vault, VaultDocument};
use std::{collections::BTreeMap, sync::Arc};
use tokio::sync::RwLock;

/// File-backed application service composed from the vault, workspace policy,
/// and rebuildable index. It is the only concrete service transports need.
#[derive(Clone)]
pub struct FileMikuApplication {
    vault: Arc<Vault>,
    workspace: Arc<dyn WorkspaceService>,
    index: IndexApi,
    documents_cache: Arc<RwLock<Option<Vec<VaultDocument>>>>,
}

impl FileMikuApplication {
    pub fn new(vault: Arc<Vault>, workspace: Arc<dyn WorkspaceService>, index: IndexApi) -> Self {
        Self {
            vault,
            workspace,
            index,
            documents_cache: Arc::new(RwLock::new(None)),
        }
    }

    /// Discard the parsed projection after an external filesystem change.
    pub async fn invalidate_documents(&self) {
        *self.documents_cache.write().await = None;
    }

    async fn documents(&self) -> Result<Vec<VaultDocument>, ApplicationError> {
        if let Some(documents) = self.documents_cache.read().await.clone() {
            return Ok(documents);
        }
        let documents = self
            .workspace
            .workspace()
            .await
            .map_err(application_error)?;
        let mut cache = self.documents_cache.write().await;
        if let Some(existing) = cache.as_ref() {
            return Ok(existing.clone());
        }
        *cache = Some(documents.clone());
        Ok(documents)
    }

    async fn resolve_document(&self, note: NoteRef) -> Result<VaultDocument, ApplicationError> {
        match note {
            NoteRef::Path(path) => self
                .vault
                .read(path.as_str())
                .map_err(ApplicationError::from),
            NoteRef::Id(id) => self
                .documents()
                .await?
                .into_iter()
                .find(|document| document.note.id == id)
                .ok_or_else(|| ApplicationError::NotFound(id.as_str().to_string())),
        }
    }

    fn file_node(document: &VaultDocument) -> FileNode {
        let path = RelativePath::new(&document.note.source_path)
            .expect("VaultDocument source paths are canonical relative paths");
        let name = path
            .as_str()
            .rsplit('/')
            .next()
            .unwrap_or(path.as_str())
            .to_string();
        FileNode {
            kind: FileNodeKind::Markdown,
            path,
            note_id: Some(document.note.id.clone()),
            identity_generated: document.identity_generated,
            name,
            title: Some(document.note.title.clone()),
            has_children: false,
        }
    }

    fn folder_node(path: RelativePath, name: String, has_children: bool) -> FileNode {
        FileNode {
            kind: FileNodeKind::Folder,
            path,
            note_id: None,
            identity_generated: false,
            name,
            title: None,
            has_children,
        }
    }

    fn tree_nodes(documents: &[VaultDocument], folder: &RelativePath) -> Vec<FileNode> {
        let folder_path = folder.as_str();
        let mut folders = BTreeMap::<String, FileNode>::new();
        let mut files = BTreeMap::<String, FileNode>::new();

        for document in documents {
            let source = document.note.source_path.as_str();
            let relative = if folder_path.is_empty() {
                source
            } else if let Some(value) = source.strip_prefix(&format!("{folder_path}/")) {
                value
            } else {
                continue;
            };
            let Some(first) = relative.split('/').next() else {
                continue;
            };

            if !relative.contains('/') {
                files.insert(first.to_string(), Self::file_node(document));
                continue;
            }

            let child_path = if folder_path.is_empty() {
                first.to_string()
            } else {
                format!("{folder_path}/{first}")
            };
            if let Ok(path) = RelativePath::new(&child_path) {
                folders.insert(child_path, Self::folder_node(path, first.to_string(), true));
            }
        }

        folders.into_values().chain(files.into_values()).collect()
    }

    fn summary_node(document: &VaultDocument) -> FileNode {
        Self::file_node(document)
    }
}

#[async_trait]
impl VaultReader for FileMikuApplication {
    async fn vault_info(&self) -> Result<VaultInfo, ApplicationError> {
        let documents = self.documents().await?;
        Ok(VaultInfo {
            root: self.workspace.root(),
            readonly: self.workspace.readonly(),
            index_phase: IndexPhase::Ready,
            capabilities: self.index.capabilities().await?,
            note_count: documents.len(),
            generated_identity_count: documents
                .iter()
                .filter(|document| document.identity_generated)
                .count(),
            first_note: documents
                .first()
                .and_then(|document| crate::NotePath::new(&document.note.source_path).ok()),
        })
    }

    async fn file_tree(&self, request: FileTreeRequest) -> Result<FileTree, ApplicationError> {
        let documents = self.documents().await?;
        Ok(FileTree {
            folder: request.folder.clone(),
            nodes: Self::tree_nodes(&documents, &request.folder),
        })
    }

    async fn read_note(&self, note: NoteRef) -> Result<VaultDocument, ApplicationError> {
        self.resolve_document(note).await
    }

    async fn note_context(&self, note: NoteRef) -> Result<NoteContext, ApplicationError> {
        let document = self.resolve_document(note).await?;
        let documents = self.documents().await?;
        let parents = document
            .note
            .parents
            .iter()
            .filter_map(|id| documents.iter().find(|candidate| candidate.note.id == *id))
            .map(Self::summary_node)
            .collect();
        let children = documents
            .iter()
            .filter(|candidate| candidate.note.parents.contains(&document.note.id))
            .map(Self::summary_node)
            .collect();
        let backlinks = self.index.backlinks(&document.note.source_path).await?;
        Ok(NoteContext {
            note: document,
            parents,
            children,
            backlinks,
        })
    }
}

#[async_trait]
impl VaultWriter for FileMikuApplication {
    async fn save_note(&self, command: SaveNoteCommand) -> Result<VaultDocument, ApplicationError> {
        let document = self.resolve_document(command.note).await?;
        let id: NoteId = document.note.id;
        let saved = self
            .workspace
            .save_note(
                id.as_str(),
                command.title,
                command.body,
                command.expected_revision,
            )
            .await
            .map_err(application_error)?;
        let mut cache = self.documents_cache.write().await;
        if let Some(documents) = cache.as_mut() {
            if let Some(existing) = documents
                .iter_mut()
                .find(|existing| existing.note.id == saved.note.id)
            {
                *existing = saved.clone();
            }
        }
        Ok(saved)
    }
}

#[async_trait]
impl SearchReader for FileMikuApplication {
    async fn search(&self, request: SearchRequest) -> Result<Vec<SearchHit>, ApplicationError> {
        Ok(self.index.search(request).await?)
    }
}

#[async_trait]
impl TagReader for FileMikuApplication {
    async fn tags(&self) -> Result<Vec<miku_domain::TagCount>, ApplicationError> {
        Ok(self.index.tags().await?)
    }

    async fn notes_with_tag(
        &self,
        tag: String,
    ) -> Result<Vec<miku_domain::PageSummary>, ApplicationError> {
        Ok(self.index.pages_with_tag(&tag).await?)
    }
}

fn application_error(error: WorkspaceServiceError) -> ApplicationError {
    match error {
        WorkspaceServiceError::Readonly => ApplicationError::Readonly,
        WorkspaceServiceError::NotFound(note) => ApplicationError::NotFound(note),
        WorkspaceServiceError::Conflict => ApplicationError::Conflict,
        WorkspaceServiceError::Vault(error) => ApplicationError::Vault(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::FileWorkspaceService;
    use miku_index_memory::MemoryIndex;
    use tempfile::tempdir;

    #[tokio::test]
    async fn file_application_exposes_real_folders_and_path_notes() {
        let root = tempdir().expect("temporary vault");
        let vault = Arc::new(Vault::new(root.path()));
        vault
            .create("Projects/Alpha.md", "Alpha", "# Alpha", Default::default())
            .expect("create nested note");
        vault
            .create("Inbox.md", "Inbox", "capture", Default::default())
            .expect("create root note");
        let workspace: Arc<dyn WorkspaceService> =
            Arc::new(FileWorkspaceService::new(Arc::clone(&vault), false));
        let index = IndexApi::from_store(Arc::new(MemoryIndex::new()));
        let application = FileMikuApplication::new(vault, workspace, index);

        let tree = application
            .file_tree(FileTreeRequest {
                folder: RelativePath::root(),
            })
            .await
            .expect("root tree");
        assert_eq!(tree.nodes.len(), 2);
        assert_eq!(tree.nodes[0].kind, FileNodeKind::Folder);
        assert_eq!(tree.nodes[0].path.as_str(), "Projects");
        assert_eq!(tree.nodes[1].path.as_str(), "Inbox.md");

        let note = application
            .read_note(NoteRef::Path(
                crate::NotePath::new("Projects/Alpha.md").unwrap(),
            ))
            .await
            .expect("path note");
        assert_eq!(note.note.title, "Alpha");
    }
}
