//! Backend-neutral domain types and index-store contract for Miku.
//!
//! Markdown files remain the source of truth. An [`IndexStore`] stores only a
//! rebuildable projection used by the HTTP read path and background indexer.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Result type used by the index-store contract.
pub type StoreResult<T> = Result<T, StoreError>;

/// Errors exposed by an index store without leaking driver-specific types.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize, thiserror::Error)]
pub enum StoreError {
    /// The backend is unavailable or failed to establish its required state.
    #[error("index store unavailable: {0}")]
    Unavailable(String),
    /// The requested operation is not supported by this backend.
    #[error("index store operation unsupported: {0}")]
    Unsupported(String),
    /// The caller supplied an invalid domain value.
    #[error("invalid index store input: {0}")]
    InvalidInput(String),
    /// A backend operation failed without a more specific contract error.
    #[error("index store operation failed: {0}")]
    Operation(String),
}

/// A page's searchable and navigable summary.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PageSummary {
    /// Path relative to the Markdown content root, including no leading slash.
    pub path: String,
    /// Display title derived from frontmatter, heading, or filename.
    pub title: String,
    /// Opaque user-defined frontmatter retained by the index.
    pub frontmatter: serde_json::Value,
    /// Source file modification time as Unix seconds.
    pub mtime: i64,
}

/// The complete index projection produced for one Markdown page.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PageIndex {
    /// Page summary fields used by navigation and search.
    pub summary: PageSummary,
    /// Markdown body used to build full-text search and mentions.
    pub body: String,
    /// Outgoing links and embeds parsed from the page.
    pub links: Vec<LinkRecord>,
    /// Inline and frontmatter tags merged into one set by the parser.
    pub tags: Vec<String>,
    /// Frontmatter aliases used during wikilink resolution.
    pub aliases: Vec<String>,
    /// Whether the rendered page needs the Mermaid client asset.
    pub has_mermaid: bool,
    /// Deterministic signals extracted from the same Markdown parse.
    #[serde(default)]
    pub signals: DocumentSignals,
}

/// Lightweight, deterministic signals used by navigation and future discovery surfaces.
#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct DocumentSignals {
    /// First meaningful paragraph from the Markdown body, if present.
    pub lead: String,
    /// Heading text and nesting level in document order.
    pub headings: Vec<HeadingSummary>,
    /// Count of visible, whitespace-separated words in the body.
    pub word_count: usize,
}

/// A heading extracted from a Markdown document.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct HeadingSummary {
    /// ATX/setext heading level.
    pub level: u8,
    /// Plain visible heading text.
    pub text: String,
}

/// An outgoing page or asset link in a page projection.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct LinkRecord {
    /// Page or asset target as written by the author.
    pub target: String,
    /// Normalized resolver key used for matching.
    pub target_norm: String,
    /// Optional display alias from `[[target|alias]]`.
    pub alias: Option<String>,
    /// Whether this link targets a page or an asset.
    pub kind: LinkKind,
    /// Whether the link is an embed rather than a normal navigation link.
    pub is_embed: bool,
}

/// The two link classes supported by the index projection.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum LinkKind {
    /// A wikilink to another Markdown page.
    Page,
    /// An embed or reference to a file under the assets root.
    Asset,
}

/// Search scope accepted by all backends.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
pub enum SearchScope {
    /// Search title and body text.
    All,
    /// Search only page title/path metadata.
    Title,
    /// Search only indexed body text.
    Body,
}

/// Backend-neutral search input.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct SearchRequest {
    /// User-entered search text.
    pub query: String,
    /// Requested search scope.
    pub scope: SearchScope,
    /// Maximum number of results to return.
    pub limit: usize,
}

/// One search result with an optional backend-generated snippet.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct SearchHit {
    /// Page path relative to the content root.
    pub path: String,
    /// Display title.
    pub title: String,
    /// Context excerpt; empty when the backend cannot provide one.
    pub snippet: String,
}

/// A page that links to another page.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct Backlink {
    /// Source page path.
    pub path: String,
    /// Source page title.
    pub title: String,
}

/// A plain-text mention that has not been converted into a wikilink.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct UnlinkedMention {
    /// Source page path.
    pub path: String,
    /// Source page title.
    pub title: String,
    /// Short context around the mention.
    pub snippet: String,
}

/// A persisted plain-text occurrence that may be promoted to a wikilink.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct MentionRecord {
    /// Target page path whose title or alias was mentioned.
    pub target_path: String,
    /// Source page path containing the occurrence.
    pub source_path: String,
    /// Source page display title.
    pub source_title: String,
    /// Matched title or alias text as it appeared in the source.
    pub matched_text: String,
    /// Short context around the occurrence.
    pub snippet: String,
}

/// A tag and its indexed page count.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct TagCount {
    /// Normalized tag name without the leading `#`.
    pub tag: String,
    /// Number of indexed pages containing the tag.
    pub count: i64,
}

/// Capabilities advertised by a concrete index store.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct IndexCapabilities {
    /// Whether the store survives process restart.
    pub durable: bool,
    /// Whether the store provides ranked full-text search.
    pub full_text_search: bool,
    /// Whether the store provides fuzzy page matching.
    pub fuzzy_page_search: bool,
    /// Whether page replacement/deletion is atomic at the store boundary.
    pub transactions: bool,
    /// Whether the store synchronizes with a remote database service.
    pub remote_sync: bool,
}

/// A committed index change emitted to cache and event layers.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub enum IndexEvent {
    /// A page projection was inserted or replaced.
    PageIndexed { path: String },
    /// A page projection was removed.
    PageDeleted { path: String },
    /// A full reconcile changed enough pages to require a broad refresh.
    Reconciled,
}

/// Read operations shared by memory, SQLite, and Postgres indexes.
#[async_trait]
pub trait IndexReader: Send + Sync {
    /// Return the capabilities of this concrete store.
    async fn capabilities(&self) -> StoreResult<IndexCapabilities>;

    /// List all indexed pages in deterministic order.
    async fn list_pages(&self) -> StoreResult<Vec<PageSummary>>;

    /// Load one indexed page summary, if it exists.
    async fn page(&self, path: &str) -> StoreResult<Option<PageSummary>>;

    /// Search indexed pages according to the shared request semantics.
    async fn search(&self, request: SearchRequest) -> StoreResult<Vec<SearchHit>>;

    /// Return explicit backlinks for a page path.
    async fn backlinks(&self, path: &str) -> StoreResult<Vec<Backlink>>;

    /// Return derived unlinked mentions targeting a page path.
    async fn mentions_for_target(&self, path: &str) -> StoreResult<Vec<MentionRecord>>;

    /// Whether the derived mention projection has completed at least one full build.
    async fn mentions_ready(&self) -> StoreResult<bool> {
        Ok(false)
    }

    /// Return all tags and their page counts.
    async fn tags(&self) -> StoreResult<Vec<TagCount>>;

    /// List pages carrying one exact normalized tag.
    async fn pages_with_tag(&self, tag: &str) -> StoreResult<Vec<PageSummary>>;
}

/// Mutation operations shared by durable and in-memory index stores.
#[async_trait]
pub trait IndexWriter: Send + Sync {
    /// Replace one complete page projection atomically.
    async fn replace_page(&self, page: PageIndex) -> StoreResult<IndexEvent>;

    /// Replace a batch of complete page projections.
    ///
    /// Backends may override this to commit the batch in one transaction;
    /// the default keeps the contract compatible with simple writers.
    async fn replace_pages(&self, pages: Vec<PageIndex>) -> StoreResult<Vec<IndexEvent>> {
        let mut events = Vec::with_capacity(pages.len());
        for page in pages {
            events.push(self.replace_page(page).await?);
        }
        Ok(events)
    }

    /// Rebuild any derived full-text structures after a bulk projection load.
    ///
    /// Stores without a separate derived search structure can keep the default
    /// no-op implementation.
    async fn rebuild_search_index(&self) -> StoreResult<()> {
        Ok(())
    }

    /// Replace every derived mention emitted by one source page.
    async fn replace_mentions_for_source(
        &self,
        _source_path: &str,
        _mentions: Vec<MentionRecord>,
    ) -> StoreResult<()> {
        Err(StoreError::Unsupported(
            "derived unlinked mentions".to_string(),
        ))
    }

    /// Replace derived mentions for several source pages in one backend operation.
    async fn replace_mentions_for_sources(
        &self,
        entries: Vec<(String, Vec<MentionRecord>)>,
    ) -> StoreResult<()> {
        for (source_path, mentions) in entries {
            self.replace_mentions_for_source(&source_path, mentions)
                .await?;
        }
        Ok(())
    }

    /// Remove every derived mention emitted by one source page.
    async fn delete_mentions_for_source(&self, _source_path: &str) -> StoreResult<()> {
        Err(StoreError::Unsupported(
            "derived unlinked mentions".to_string(),
        ))
    }

    /// Remove every derived mention targeting one page.
    async fn delete_mentions_for_target(&self, _target_path: &str) -> StoreResult<()> {
        Err(StoreError::Unsupported(
            "derived unlinked mentions".to_string(),
        ))
    }

    /// Remove derived mentions targeting several pages in one backend operation.
    async fn delete_mentions_for_targets(&self, target_paths: Vec<String>) -> StoreResult<()> {
        for target_path in target_paths {
            self.delete_mentions_for_target(&target_path).await?;
        }
        Ok(())
    }

    /// Mark the derived mention projection complete for the current page set.
    async fn mark_mentions_ready(&self) -> StoreResult<()> {
        Err(StoreError::Unsupported(
            "derived unlinked mentions".to_string(),
        ))
    }

    /// Delete one page projection and return the resulting event.
    async fn delete_page(&self, path: &str) -> StoreResult<IndexEvent>;
}

/// The complete storage contract used when the application owns one backend.
pub trait IndexStore: IndexReader + IndexWriter {}

impl<T> IndexStore for T where T: IndexReader + IndexWriter {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn page_index_round_trips_through_json() {
        let page = PageIndex {
            summary: PageSummary {
                path: "Notes/Today.md".to_string(),
                title: "Today".to_string(),
                frontmatter: serde_json::json!({"status": "draft"}),
                mtime: 42,
            },
            body: "# Today".to_string(),
            links: vec![LinkRecord {
                target: "Index".to_string(),
                target_norm: "index".to_string(),
                alias: None,
                kind: LinkKind::Page,
                is_embed: false,
            }],
            tags: vec!["daily".to_string()],
            aliases: Vec::new(),
            has_mermaid: false,
            signals: DocumentSignals::default(),
        };

        let encoded = serde_json::to_string(&page).expect("encode page index");
        let decoded: PageIndex = serde_json::from_str(&encoded).expect("decode page index");

        assert_eq!(decoded, page);
    }

    #[test]
    fn capabilities_describe_a_local_durable_store() {
        let capabilities = IndexCapabilities {
            durable: true,
            full_text_search: true,
            fuzzy_page_search: false,
            transactions: true,
            remote_sync: false,
        };

        assert!(capabilities.durable);
        assert!(capabilities.transactions);
        assert!(!capabilities.remote_sync);
    }
}
