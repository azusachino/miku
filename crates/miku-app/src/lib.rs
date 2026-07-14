//! Application-facing index API.
//!
//! HTTP handlers should depend on [`IndexApi`] rather than a database driver.
//! The indexer can retain the writer half while read-only routes retain the
//! reader half, which keeps backend selection and cache composition outside
//! the request layer.

use miku_domain::{
    Backlink, IndexCapabilities, IndexEvent, IndexReader, IndexStore, IndexWriter, PageIndex,
    PageSummary, SearchHit, SearchRequest, StoreResult, TagCount, UnlinkedMention,
};
use std::sync::Arc;

/// Backend-neutral operations consumed by Miku's HTTP and indexing layers.
#[derive(Clone)]
pub struct IndexApi {
    reader: Arc<dyn IndexReader>,
    writer: Arc<dyn IndexWriter>,
}

impl IndexApi {
    /// Compose the API from one concrete backend.
    pub fn from_store<S>(store: Arc<S>) -> Self
    where
        S: IndexStore + 'static,
    {
        Self {
            reader: store.clone(),
            writer: store,
        }
    }

    /// Compose a read path and write path independently.
    pub fn from_parts(reader: Arc<dyn IndexReader>, writer: Arc<dyn IndexWriter>) -> Self {
        Self { reader, writer }
    }

    /// Return backend capabilities for health/configuration reporting.
    pub async fn capabilities(&self) -> StoreResult<IndexCapabilities> {
        self.reader.capabilities().await
    }

    /// List indexed pages in backend-defined deterministic order.
    pub async fn list_pages(&self) -> StoreResult<Vec<PageSummary>> {
        self.reader.list_pages().await
    }

    /// Load one page summary.
    pub async fn page(&self, path: &str) -> StoreResult<Option<PageSummary>> {
        self.reader.page(path).await
    }

    /// Search indexed content.
    pub async fn search(&self, request: SearchRequest) -> StoreResult<Vec<SearchHit>> {
        self.reader.search(request).await
    }

    /// Find pages linking to the requested page.
    pub async fn backlinks(&self, path: &str) -> StoreResult<Vec<Backlink>> {
        self.reader.backlinks(path).await
    }

    /// Find plain-text mentions of the requested page.
    pub async fn unlinked_mentions(&self, path: &str) -> StoreResult<Vec<UnlinkedMention>> {
        self.reader.unlinked_mentions(path).await
    }

    /// Return tag counts.
    pub async fn tags(&self) -> StoreResult<Vec<TagCount>> {
        self.reader.tags().await
    }

    /// Atomically replace one indexed page projection.
    pub async fn replace_page(&self, page: PageIndex) -> StoreResult<IndexEvent> {
        self.writer.replace_page(page).await
    }

    /// Delete one indexed page projection.
    pub async fn delete_page(&self, path: &str) -> StoreResult<IndexEvent> {
        self.writer.delete_page(path).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use miku_domain::{PageIndex, PageSummary, SearchScope};
    use miku_index_memory::MemoryIndex;

    #[tokio::test]
    async fn facade_composes_reader_and_writer() {
        let store = Arc::new(MemoryIndex::new());
        let api = IndexApi::from_store(store);
        api.replace_page(PageIndex {
            summary: PageSummary {
                path: "Today.md".to_string(),
                title: "Today".to_string(),
                frontmatter: serde_json::json!({}),
                mtime: 1,
            },
            body: "A note".to_string(),
            links: Vec::new(),
            tags: Vec::new(),
            aliases: Vec::new(),
            has_mermaid: false,
        })
        .await
        .expect("write page");

        let hits = api
            .search(SearchRequest {
                query: "note".to_string(),
                scope: SearchScope::Body,
                limit: 10,
            })
            .await
            .expect("search pages");
        assert_eq!(hits.len(), 1);
    }
}
