//! Concrete file-backed implementation of the universal application port.

use crate::{
    ApplicationError, FileNode, FileNodeKind, FileTree, FileTreeRequest, IndexApi, IndexPhase,
    NoteContext, NoteRef, RelativePath, SaveNoteCommand, SearchReader, TagReader, VaultInfo,
    VaultReader, VaultWriter, WorkspaceService, WorkspaceServiceError,
};
use async_trait::async_trait;
use miku_domain::{workspace::NoteId, PageSummary, SearchHit, SearchRequest};
use miku_vault::{Vault, VaultDocument};
use std::{
    collections::{BTreeMap, HashMap, VecDeque},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use tokio::sync::RwLock;

const DOCUMENT_CACHE_CAPACITY: usize = 128;

struct DocumentCache {
    entries: HashMap<String, VaultDocument>,
    order: VecDeque<String>,
}

impl DocumentCache {
    fn new() -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    fn get(&mut self, path: &str) -> Option<VaultDocument> {
        let document = self.entries.get(path).cloned()?;
        self.touch(path);
        Some(document)
    }

    fn insert(&mut self, path: String, document: VaultDocument) {
        self.entries.insert(path.clone(), document);
        self.touch(&path);
        while self.order.len() > DOCUMENT_CACHE_CAPACITY {
            if let Some(evicted) = self.order.pop_front() {
                self.entries.remove(&evicted);
            }
        }
    }

    fn clear(&mut self) {
        self.entries.clear();
        self.order.clear();
    }

    fn touch(&mut self, path: &str) {
        self.order.retain(|entry| entry != path);
        self.order.push_back(path.to_string());
    }
}

/// File-backed application service composed from the vault, workspace policy,
/// and rebuildable index. It is the only concrete service transports need.
#[derive(Clone)]
pub struct FileMikuApplication {
    vault: Arc<Vault>,
    workspace: Arc<dyn WorkspaceService>,
    index: IndexApi,
    documents_cache: Arc<RwLock<DocumentCache>>,
    index_ready: Arc<AtomicBool>,
}

impl FileMikuApplication {
    pub fn new(vault: Arc<Vault>, workspace: Arc<dyn WorkspaceService>, index: IndexApi) -> Self {
        Self::with_index_readiness(vault, workspace, index, Arc::new(AtomicBool::new(true)))
    }

    /// Construct the application with the indexer's live readiness state.
    pub fn with_index_readiness(
        vault: Arc<Vault>,
        workspace: Arc<dyn WorkspaceService>,
        index: IndexApi,
        index_ready: Arc<AtomicBool>,
    ) -> Self {
        Self {
            vault,
            workspace,
            index,
            documents_cache: Arc::new(RwLock::new(DocumentCache::new())),
            index_ready,
        }
    }

    /// Discard the parsed projection after an external filesystem change.
    pub async fn invalidate_documents(&self) {
        self.documents_cache.write().await.clear();
    }

    async fn read_document_path(&self, path: &str) -> Result<VaultDocument, ApplicationError> {
        if let Some(document) = self.documents_cache.write().await.get(path) {
            return Ok(document);
        }
        let document = self.vault.read(path).map_err(ApplicationError::from)?;
        self.documents_cache
            .write()
            .await
            .insert(path.to_string(), document.clone());
        Ok(document)
    }

    async fn resolve_document(&self, note: NoteRef) -> Result<VaultDocument, ApplicationError> {
        match note {
            NoteRef::Path(path) => self.read_document_path(path.as_str()).await,
            NoteRef::Id(id) => {
                let pages = self.index.list_pages().await?;
                let path = pages
                    .into_iter()
                    .find(|page| {
                        page.frontmatter
                            .get("id")
                            .and_then(serde_json::Value::as_str)
                            == Some(id.as_str())
                    })
                    .map(|page| page.path)
                    .ok_or_else(|| ApplicationError::NotFound(id.as_str().to_string()))?;
                self.read_document_path(&path).await
            }
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

    fn summary_file_node(page: &PageSummary) -> FileNode {
        let path = RelativePath::new(&page.path).expect("indexed paths are canonical");
        let name = path
            .as_str()
            .rsplit('/')
            .next()
            .unwrap_or(path.as_str())
            .to_string();
        let explicit_id = page
            .frontmatter
            .get("id")
            .and_then(serde_json::Value::as_str)
            .and_then(|id| NoteId::new(id.to_string()).ok());
        FileNode {
            kind: FileNodeKind::Markdown,
            path,
            note_id: explicit_id,
            identity_generated: page.frontmatter.get("id").is_none(),
            name,
            title: Some(page.title.clone()),
            has_children: false,
        }
    }

    fn snapshot_tree_nodes(pages: &[PageSummary], folder: &RelativePath) -> Vec<FileNode> {
        let folder_path = folder.as_str();
        let mut folders = BTreeMap::<String, FileNode>::new();
        let mut files = BTreeMap::<String, FileNode>::new();

        for page in pages {
            let relative = if folder_path.is_empty() {
                page.path.as_str()
            } else if let Some(value) = page.path.strip_prefix(&format!("{folder_path}/")) {
                value
            } else {
                continue;
            };
            let Some(first) = relative.split('/').next() else {
                continue;
            };
            if !relative.contains('/') {
                files.insert(first.to_string(), Self::summary_file_node(page));
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
}

#[async_trait]
impl VaultReader for FileMikuApplication {
    async fn vault_info(&self) -> Result<VaultInfo, ApplicationError> {
        let pages = self
            .index
            .list_pages()
            .await
            .map_err(ApplicationError::from)?;
        Ok(VaultInfo {
            root: self.workspace.root(),
            readonly: self.workspace.readonly(),
            index_phase: if self.index_ready.load(Ordering::Acquire) {
                IndexPhase::Ready
            } else {
                IndexPhase::Indexing
            },
            capabilities: self.index.capabilities().await?,
            note_count: pages.len(),
            generated_identity_count: pages
                .iter()
                .filter(|page| page.frontmatter.get("id").is_none())
                .count(),
            first_note: pages
                .first()
                .and_then(|page| crate::NotePath::new(&page.path).ok()),
        })
    }

    async fn file_tree(&self, request: FileTreeRequest) -> Result<FileTree, ApplicationError> {
        let pages = self
            .index
            .list_pages()
            .await
            .map_err(ApplicationError::from)?;
        Ok(FileTree {
            folder: request.folder.clone(),
            nodes: Self::snapshot_tree_nodes(&pages, &request.folder),
        })
    }

    async fn read_note(&self, note: NoteRef) -> Result<VaultDocument, ApplicationError> {
        self.resolve_document(note).await
    }

    async fn note_context(&self, note: NoteRef) -> Result<NoteContext, ApplicationError> {
        let document = self.resolve_document(note).await?;
        let pages = self
            .index
            .list_pages()
            .await
            .map_err(ApplicationError::from)?;
        let parents = document
            .note
            .parents
            .iter()
            .filter_map(|id| {
                pages.iter().find(|page| {
                    page.frontmatter
                        .get("id")
                        .and_then(serde_json::Value::as_str)
                        == Some(id.as_str())
                })
            })
            .map(Self::summary_file_node)
            .collect();
        let children = pages
            .iter()
            .filter(|page| {
                page.frontmatter
                    .get("parents")
                    .and_then(serde_json::Value::as_array)
                    .is_some_and(|parents| {
                        parents
                            .iter()
                            .any(|parent| parent.as_str() == Some(document.note.id.as_str()))
                    })
            })
            .map(Self::summary_file_node)
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
        self.documents_cache
            .write()
            .await
            .insert(saved.note.source_path.clone(), saved.clone());
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
    use miku_domain::{DocumentSignals, IndexWriter, PageIndex, PageSummary};
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
        let memory = Arc::new(MemoryIndex::new());
        memory
            .replace_pages(vec![
                PageIndex {
                    summary: PageSummary {
                        path: "Projects/Alpha.md".to_string(),
                        title: "Alpha".to_string(),
                        frontmatter: serde_json::json!({}),
                        mtime: 1,
                    },
                    body: "# Alpha".to_string(),
                    links: Vec::new(),
                    tags: Vec::new(),
                    aliases: Vec::new(),
                    has_mermaid: false,
                    signals: DocumentSignals::default(),
                },
                PageIndex {
                    summary: PageSummary {
                        path: "Inbox.md".to_string(),
                        title: "Inbox".to_string(),
                        frontmatter: serde_json::json!({}),
                        mtime: 1,
                    },
                    body: "capture".to_string(),
                    links: Vec::new(),
                    tags: Vec::new(),
                    aliases: Vec::new(),
                    has_mermaid: false,
                    signals: DocumentSignals::default(),
                },
            ])
            .await
            .expect("seed snapshot");
        let index = IndexApi::from_store(memory);
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
        assert_eq!(application.documents_cache.read().await.entries.len(), 1);

        application.invalidate_documents().await;
        assert!(application.documents_cache.read().await.entries.is_empty());
    }

    #[tokio::test]
    async fn document_cache_is_bounded() {
        let root = tempdir().expect("temporary vault");
        let vault = Arc::new(Vault::new(root.path()));
        for index in 0..=DOCUMENT_CACHE_CAPACITY {
            vault
                .create(
                    &format!("Note-{index}.md"),
                    format!("Note {index}"),
                    "body",
                    Default::default(),
                )
                .expect("create note");
        }
        let workspace: Arc<dyn WorkspaceService> =
            Arc::new(FileWorkspaceService::new(Arc::clone(&vault), false));
        let application = FileMikuApplication::new(
            vault,
            workspace,
            IndexApi::from_store(Arc::new(MemoryIndex::new())),
        );

        for index in 0..=DOCUMENT_CACHE_CAPACITY {
            application
                .read_note(NoteRef::Path(
                    crate::NotePath::new(format!("Note-{index}.md")).unwrap(),
                ))
                .await
                .expect("read note");
        }

        assert_eq!(
            application.documents_cache.read().await.entries.len(),
            DOCUMENT_CACHE_CAPACITY
        );
    }
}
