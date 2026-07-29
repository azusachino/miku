//! Concrete file-backed implementation of the universal application port.

use crate::{
    ApplicationError, FileNode, FileNodeKind, FileTree, FileTreeRequest, IndexApi, IndexPhase,
    NoteContext, NoteRef, OutgoingLinkRecord, RelativePath, SaveNoteCommand, SearchReader,
    TagReader, VaultInfo, VaultReader, VaultWriter, WorkspaceService, WorkspaceServiceError,
};
use async_trait::async_trait;
use miku_domain::{fold_name, workspace::NoteId, PageSummary, SearchHit, SearchRequest};
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

    pub fn read_raw_asset(&self, path: &str) -> Result<Vec<u8>, ApplicationError> {
        let rel = RelativePath::new(path)?;
        self.vault
            .read_raw_bytes(rel.as_str())
            .map_err(ApplicationError::from)
    }

    async fn resolve_document(&self, note: NoteRef) -> Result<VaultDocument, ApplicationError> {
        let requested_str = match &note {
            NoteRef::Path(path) => path.as_str().to_string(),
            NoteRef::Id(id) => id.as_str().to_string(),
        };

        let direct_path = if requested_str.ends_with(".md") {
            requested_str.clone()
        } else {
            format!("{requested_str}.md")
        };

        if let Ok(doc) = self.read_document_path(&direct_path).await {
            return Ok(doc);
        }
        if direct_path != requested_str {
            if let Ok(doc) = self.read_document_path(&requested_str).await {
                return Ok(doc);
            }
        }

        let target_slug = requested_str
            .split('/')
            .next_back()
            .unwrap_or(&requested_str)
            .trim_end_matches(".md");

        let pages = self.index.list_pages().await?;
        if let Some(matched_path) = Self::resolve_named_path(target_slug, &pages) {
            if let Ok(doc) = self.read_document_path(&matched_path).await {
                return Ok(doc);
            }
        }

        let matched_page = pages.into_iter().find(|page| {
            page.path == requested_str
                || page.path == direct_path
                || page
                    .frontmatter
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    == Some(requested_str.as_str())
        });

        if let Some(page) = matched_page {
            return self.read_document_path(&page.path).await;
        }

        Err(ApplicationError::NotFound(requested_str))
    }

    /// Build a folded-name (filename, title, alias) -> page index once, so
    /// resolving many wikilink targets against the same page list (e.g.
    /// every outgoing link in one note) doesn't re-scan the whole vault
    /// per link. A single `resolve_named_path` call still pays the same
    /// O(pages) cost either way, but the per-note-context loop over every
    /// outgoing link previously paid O(links * pages) -- 2.5s+ on a real
    /// vault for a hub note with dozens of links, against every page's
    /// name set re-derived and re-folded from scratch on every link.
    fn build_name_index(pages: &[PageSummary]) -> HashMap<String, Vec<usize>> {
        let mut index: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, page) in pages.iter().enumerate() {
            for name in miku_domain::page_names(&page.path, &page.title, &page.aliases) {
                let folded = fold_name(&name);
                if folded.is_empty() {
                    continue;
                }
                let entries = index.entry(folded).or_default();
                if !entries.contains(&i) {
                    entries.push(i);
                }
            }
        }
        index
    }

    /// Match a wikilink target against a prebuilt name index, folding
    /// whitespace/hyphen/underscore so that "elden ring" and "elden-ring"
    /// resolve to the same page. Ambiguous matches fall back to the
    /// shortest, then lexicographically first, path.
    fn resolve_from_index<'a>(
        target: &str,
        pages: &'a [PageSummary],
        index: &HashMap<String, Vec<usize>>,
    ) -> Option<&'a PageSummary> {
        let target_folded = fold_name(target);
        if target_folded.is_empty() {
            return None;
        }
        let matches = index.get(&target_folded)?;
        match matches.as_slice() {
            [] => None,
            [only] => Some(&pages[*only]),
            many => many.iter().map(|&i| &pages[i]).min_by(|a, b| {
                a.path
                    .len()
                    .cmp(&b.path.len())
                    .then_with(|| a.path.cmp(&b.path))
            }),
        }
    }

    /// Match a wikilink target against each page's filename, title, and
    /// frontmatter aliases. See `resolve_from_index` for matching
    /// semantics; prefer that directly (with a shared `build_name_index`)
    /// when resolving more than one target against the same page list.
    fn resolve_named_path(target: &str, pages: &[PageSummary]) -> Option<String> {
        let index = Self::build_name_index(pages);
        Self::resolve_from_index(target, pages, &index).map(|page| page.path.clone())
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
            aliases: Vec::new(),
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
            aliases: page.aliases.clone(),
        }
    }

    fn snapshot_tree_nodes(pages: &[PageSummary], folder: &RelativePath) -> Vec<FileNode> {
        let folder_path = folder.as_str();
        let folder_prefix = if folder_path.is_empty() {
            String::new()
        } else {
            format!("{folder_path}/")
        };
        let mut folders = BTreeMap::<String, FileNode>::new();
        let mut files = BTreeMap::<String, FileNode>::new();

        for page in pages {
            let relative = if folder_path.is_empty() {
                page.path.as_str()
            } else if let Some(value) = page.path.strip_prefix(&folder_prefix) {
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

    async fn list_pages(&self) -> Result<Vec<PageSummary>, ApplicationError> {
        self.index
            .list_pages()
            .await
            .map_err(ApplicationError::from)
    }

    async fn read_note(&self, note: NoteRef) -> Result<VaultDocument, ApplicationError> {
        self.resolve_document(note).await
    }

    async fn read_raw_asset(&self, path: &str) -> Result<Vec<u8>, ApplicationError> {
        self.read_raw_asset(path)
    }

    async fn note_context(&self, note: NoteRef) -> Result<NoteContext, ApplicationError> {
        let document = self.resolve_document(note).await?;
        let pages = self
            .index
            .list_pages()
            .await
            .map_err(ApplicationError::from)?;
        let id_map = pages
            .iter()
            .filter_map(|page| {
                page.frontmatter
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .map(|id| (id, page))
            })
            .collect::<std::collections::HashMap<_, _>>();
        let parents = document
            .note
            .parents
            .iter()
            .filter_map(|id| id_map.get(id.as_str()).copied())
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
        let page_projection = miku_indexer::build_page_index(
            &document.note.source_path,
            document.body.as_bytes(),
            document.revision.mtime,
        );
        let current_dir = document.note.source_path.split('/').collect::<Vec<_>>();
        let folder_prefix = if current_dir.len() > 1 {
            current_dir[..current_dir.len() - 1].join("/")
        } else {
            String::new()
        };

        let mut outgoing = Vec::new();
        let mut seen = std::collections::HashSet::new();
        let name_index = Self::build_name_index(&pages);

        for link in page_projection.links {
            if link.kind != miku_domain::LinkKind::Page {
                continue;
            }
            let resolved = Self::resolve_from_index(&link.target, &pages, &name_index);
            let (target_path, title, is_missing) = match resolved {
                Some(page) => (page.path.clone(), page.title.clone(), false),
                None => {
                    let raw_target = link.target.trim();
                    let path = if !folder_prefix.is_empty() && !raw_target.contains('/') {
                        let sub = if raw_target.ends_with(".md") {
                            raw_target.to_string()
                        } else {
                            format!("{raw_target}.md")
                        };
                        format!("{folder_prefix}/{sub}")
                    } else if raw_target.ends_with(".md") {
                        raw_target.to_string()
                    } else {
                        format!("{raw_target}.md")
                    };
                    let title = link.alias.clone().unwrap_or_else(|| {
                        raw_target
                            .split('/')
                            .next_back()
                            .unwrap_or(raw_target)
                            .trim_end_matches(".md")
                            .to_string()
                    });
                    (path, title, true)
                }
            };
            // Dedupe by folded target text, not resolved path: two
            // differently-written links (e.g. a title and an alias) that
            // resolve to the same file must each keep their own entry so
            // the frontend can look up either raw form and render a
            // correct href for it.
            if seen.insert(fold_name(&link.target)) {
                outgoing.push(OutgoingLinkRecord {
                    target: link.target.clone(),
                    title,
                    path: target_path,
                    is_missing,
                });
            }
        }

        Ok(NoteContext {
            note: document,
            parents,
            children,
            backlinks,
            outgoing,
        })
    }
}

#[async_trait]
impl VaultWriter for FileMikuApplication {
    async fn save_note(&self, command: SaveNoteCommand) -> Result<VaultDocument, ApplicationError> {
        let document = self.resolve_document(command.note).await?;
        let saved = self
            .workspace
            .save_note(
                &document.note.source_path,
                command.title,
                command.body,
                command.expected_revision,
            )
            .await
            .map_err(application_error)?;
        let mut cache = self.documents_cache.write().await;
        cache.insert(saved.note.source_path.clone(), saved.clone());
        cache.insert(saved.note.id.as_str().to_string(), saved.clone());
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
                        aliases: Vec::new(),
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
                        aliases: Vec::new(),
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
        let application = FileMikuApplication::with_index_readiness(
            vault,
            workspace,
            IndexApi::from_store(Arc::new(MemoryIndex::new())),
            Arc::new(AtomicBool::new(false)),
        );

        for index in 0..=DOCUMENT_CACHE_CAPACITY {
            application
                .read_note(NoteRef::Path(
                    crate::NotePath::new(format!("Note-{index}.md")).unwrap(),
                ))
                .await
                .expect("read note");
        }

        application
            .read_note(NoteRef::Path(crate::NotePath::new("Note-0.md").unwrap()))
            .await
            .expect("promote note");
        application
            .read_note(NoteRef::Path(crate::NotePath::new("Note-128.md").unwrap()))
            .await
            .expect("read note beyond cache bound");
        assert_eq!(
            application.documents_cache.read().await.entries.len(),
            DOCUMENT_CACHE_CAPACITY
        );
        assert!(application
            .documents_cache
            .read()
            .await
            .entries
            .contains_key("Note-0.md"));
        assert!(!application
            .documents_cache
            .read()
            .await
            .entries
            .contains_key("Note-1.md"));
    }

    #[tokio::test]
    async fn resolve_document_matches_folded_filename_title_and_alias() {
        let root = tempdir().expect("temporary vault");
        let vault = Arc::new(Vault::new(root.path()));
        vault
            .create(
                "elden-ring.md",
                "Elden Ring",
                "# Elden Ring",
                Default::default(),
            )
            .expect("create note");
        vault
            .create(
                "Games/Boss-Log.md",
                "Boss Log",
                "# Boss Log",
                Default::default(),
            )
            .expect("create aliased note");
        let workspace: Arc<dyn WorkspaceService> =
            Arc::new(FileWorkspaceService::new(Arc::clone(&vault), false));
        let memory = Arc::new(MemoryIndex::new());
        memory
            .replace_pages(vec![
                PageIndex {
                    summary: PageSummary {
                        path: "elden-ring.md".to_string(),
                        title: "Elden Ring".to_string(),
                        frontmatter: serde_json::json!({}),
                        mtime: 1,
                        aliases: Vec::new(),
                    },
                    body: "# Elden Ring".to_string(),
                    links: Vec::new(),
                    tags: Vec::new(),
                    aliases: Vec::new(),
                    has_mermaid: false,
                    signals: DocumentSignals::default(),
                },
                PageIndex {
                    summary: PageSummary {
                        path: "Games/Boss-Log.md".to_string(),
                        title: "Elden Ring Boss Log".to_string(),
                        frontmatter: serde_json::json!({ "aliases": ["ER Boss Log"] }),
                        mtime: 1,
                        aliases: vec!["ER Boss Log".to_string()],
                    },
                    body: "# Boss Log".to_string(),
                    links: Vec::new(),
                    tags: Vec::new(),
                    aliases: vec!["ER Boss Log".to_string()],
                    has_mermaid: false,
                    signals: DocumentSignals::default(),
                },
            ])
            .await
            .expect("seed snapshot");
        let index = IndexApi::from_store(memory);
        let application = FileMikuApplication::new(vault, workspace, index);

        // Filename slug, folded: a space-separated target still finds a
        // hyphenated filename.
        let by_filename = application
            .read_note(NoteRef::Id(NoteId::new("Elden Ring").unwrap()))
            .await
            .expect("resolve by folded filename");
        assert_eq!(by_filename.note.source_path, "elden-ring.md");

        // Title match: the target has no relation to the filename at all.
        let by_title = application
            .read_note(NoteRef::Id(NoteId::new("elden-ring boss_log").unwrap()))
            .await
            .expect("resolve by folded title");
        assert_eq!(by_title.note.source_path, "Games/Boss-Log.md");

        // Alias match: a frontmatter alias distinct from filename and title.
        let by_alias = application
            .read_note(NoteRef::Id(NoteId::new("er-boss-log").unwrap()))
            .await
            .expect("resolve by folded alias");
        assert_eq!(by_alias.note.source_path, "Games/Boss-Log.md");
    }

    #[tokio::test]
    async fn test_note_context_returns_resolved_outgoing_links() {
        let root = tempdir().expect("temporary vault");
        let vault = Arc::new(Vault::new(root.path()));
        vault
            .create(
                "vault/maps/topic-map.md",
                "Topics",
                "- [[reference-note]]\n- [[uncreated-note]]",
                Default::default(),
            )
            .expect("create topic-map");
        vault
            .create(
                "vault/maps/reference-note.md",
                "Reference",
                "# Reference",
                Default::default(),
            )
            .expect("create reference-note");

        let workspace: Arc<dyn WorkspaceService> =
            Arc::new(FileWorkspaceService::new(Arc::clone(&vault), false));
        let memory = Arc::new(MemoryIndex::new());
        memory
            .replace_pages(vec![
                PageIndex {
                    summary: PageSummary {
                        path: "vault/maps/topic-map.md".to_string(),
                        title: "Topics".to_string(),
                        frontmatter: serde_json::json!({}),
                        mtime: 1,
                        aliases: Vec::new(),
                    },
                    body: "- [[reference-note]]\n- [[uncreated-note]]".to_string(),
                    links: Vec::new(),
                    tags: Vec::new(),
                    aliases: Vec::new(),
                    has_mermaid: false,
                    signals: DocumentSignals::default(),
                },
                PageIndex {
                    summary: PageSummary {
                        path: "vault/maps/reference-note.md".to_string(),
                        title: "Reference".to_string(),
                        frontmatter: serde_json::json!({}),
                        mtime: 1,
                        aliases: Vec::new(),
                    },
                    body: "# Reference".to_string(),
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
        let context = application
            .note_context(NoteRef::Path(
                crate::NotePath::new("vault/maps/topic-map.md").unwrap(),
            ))
            .await
            .expect("fetch note context");

        assert_eq!(context.outgoing.len(), 2);
        assert_eq!(context.outgoing[0].target, "reference-note");
        assert_eq!(context.outgoing[0].title, "Reference");
        assert_eq!(context.outgoing[0].path, "vault/maps/reference-note.md");
        assert!(!context.outgoing[0].is_missing);

        assert_eq!(context.outgoing[1].target, "uncreated-note");
        assert_eq!(context.outgoing[1].title, "uncreated-note");
        assert_eq!(context.outgoing[1].path, "vault/maps/uncreated-note.md");
        assert!(context.outgoing[1].is_missing);
    }

    #[tokio::test]
    async fn test_note_context_keeps_separate_outgoing_entries_for_each_distinct_link_spelling() {
        // Two differently-written links to the same file (its filename and
        // its title) must each get their own `outgoing` entry: the frontend
        // looks up a raw wikilink target's resolved href by folded target
        // text, so deduping by resolved path instead would silently drop
        // the lookup entry for whichever spelling appeared second.
        let root = tempdir().expect("temporary vault");
        let vault = Arc::new(Vault::new(root.path()));
        vault
            .create(
                "source.md",
                "Source",
                "- [[target-note]]\n- [[Target]]",
                Default::default(),
            )
            .expect("create source");
        vault
            .create("target-note.md", "Target", "# Target", Default::default())
            .expect("create target");

        let workspace: Arc<dyn WorkspaceService> =
            Arc::new(FileWorkspaceService::new(Arc::clone(&vault), false));
        let memory = Arc::new(MemoryIndex::new());
        memory
            .replace_pages(vec![
                PageIndex {
                    summary: PageSummary {
                        path: "source.md".to_string(),
                        title: "Source".to_string(),
                        frontmatter: serde_json::json!({}),
                        mtime: 1,
                        aliases: Vec::new(),
                    },
                    body: "- [[target-note]]\n- [[Target]]".to_string(),
                    links: Vec::new(),
                    tags: Vec::new(),
                    aliases: Vec::new(),
                    has_mermaid: false,
                    signals: DocumentSignals::default(),
                },
                PageIndex {
                    summary: PageSummary {
                        path: "target-note.md".to_string(),
                        title: "Target".to_string(),
                        frontmatter: serde_json::json!({}),
                        mtime: 1,
                        aliases: Vec::new(),
                    },
                    body: "# Target".to_string(),
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
        let context = application
            .note_context(NoteRef::Path(crate::NotePath::new("source.md").unwrap()))
            .await
            .expect("fetch note context");

        assert_eq!(context.outgoing.len(), 2);
        assert_eq!(context.outgoing[0].target, "target-note");
        assert_eq!(context.outgoing[0].path, "target-note.md");
        assert_eq!(context.outgoing[1].target, "Target");
        assert_eq!(context.outgoing[1].path, "target-note.md");
    }

    #[tokio::test]
    async fn test_note_context_resolves_many_links_without_rescanning_the_vault_per_link() {
        // Regression guard: note_context used to call resolve_named_path
        // (an O(pages) linear scan) once per outgoing link, so a hub note
        // with many links paid O(links * pages) -- 2.5s+ on the real
        // ~15,900-file vault for a note with just 47 links. Reproduces
        // the same shape (many links, many unrelated pages) at a size
        // that would make the old behavior obviously slow in CI (the old
        // code took ~150ms+ here; the fix takes low single-digit ms) while
        // staying fast enough to run on every `cargo test`.
        let root = tempdir().expect("temporary vault");
        let vault = Arc::new(Vault::new(root.path()));
        const LINK_COUNT: usize = 60;
        const UNRELATED_PAGE_COUNT: usize = 3_000;

        let body: String = (0..LINK_COUNT)
            .map(|i| format!("- [[target-{i}]]\n"))
            .collect();
        vault
            .create("hub.md", "Hub", &body, Default::default())
            .expect("create hub");

        let workspace: Arc<dyn WorkspaceService> =
            Arc::new(FileWorkspaceService::new(Arc::clone(&vault), false));
        let memory = Arc::new(MemoryIndex::new());
        let mut pages = vec![PageIndex {
            summary: PageSummary {
                path: "hub.md".to_string(),
                title: "Hub".to_string(),
                frontmatter: serde_json::json!({}),
                mtime: 1,
                aliases: Vec::new(),
            },
            body,
            links: Vec::new(),
            tags: Vec::new(),
            aliases: Vec::new(),
            has_mermaid: false,
            signals: DocumentSignals::default(),
        }];
        for i in 0..LINK_COUNT {
            pages.push(PageIndex {
                summary: PageSummary {
                    path: format!("target-{i}.md"),
                    title: format!("Target {i}"),
                    frontmatter: serde_json::json!({}),
                    mtime: 1,
                    aliases: Vec::new(),
                },
                body: String::new(),
                links: Vec::new(),
                tags: Vec::new(),
                aliases: Vec::new(),
                has_mermaid: false,
                signals: DocumentSignals::default(),
            });
        }
        for i in 0..UNRELATED_PAGE_COUNT {
            pages.push(PageIndex {
                summary: PageSummary {
                    path: format!("unrelated-{i}.md"),
                    title: format!("Unrelated Page {i} With A Longer Title"),
                    frontmatter: serde_json::json!({}),
                    mtime: 1,
                    aliases: vec![format!("alias-{i}")],
                },
                body: String::new(),
                links: Vec::new(),
                tags: Vec::new(),
                aliases: vec![format!("alias-{i}")],
                has_mermaid: false,
                signals: DocumentSignals::default(),
            });
        }
        memory.replace_pages(pages).await.expect("seed snapshot");

        let index = IndexApi::from_store(memory);
        let application = FileMikuApplication::new(vault, workspace, index);
        let started = std::time::Instant::now();
        let context = application
            .note_context(NoteRef::Path(crate::NotePath::new("hub.md").unwrap()))
            .await
            .expect("fetch note context");
        let elapsed = started.elapsed();

        assert_eq!(context.outgoing.len(), LINK_COUNT);
        assert!(
            elapsed < std::time::Duration::from_millis(150),
            "note_context took {elapsed:?} for {LINK_COUNT} links against {} pages -- \
             expected O(links + pages), not O(links * pages)",
            UNRELATED_PAGE_COUNT + LINK_COUNT + 1
        );
    }
}
