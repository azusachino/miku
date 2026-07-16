//! Deterministic in-memory graph and Tantivy [`miku_domain::IndexStore`] implementation.
//!
//! This is the reference behavior for contract tests and disposable
//! development. It is not a durable deployment backend.

use async_trait::async_trait;
use miku_domain::{
    Backlink, HotProjection, IndexCapabilities, IndexEvent, IndexReader, IndexWriter,
    MentionRecord, PageIndex, PageSummary, SearchHit, SearchRequest, StoreError, StoreResult,
    TagCount,
};
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

mod search;

use search::SearchProjection;

type MentionKey = (String, String, String);
type MentionMap = BTreeMap<MentionKey, MentionRecord>;

/// An in-memory index keyed by source-relative page path.
#[derive(Clone)]
pub struct MemoryIndex {
    pages: Arc<RwLock<BTreeMap<String, PageIndex>>>,
    mentions: Arc<RwLock<MentionMap>>,
    search: Arc<RwLock<SearchProjection>>,
}

impl HotProjection for MemoryIndex {}

impl Default for MemoryIndex {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryIndex {
    /// Create an empty reference index.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pages: Arc::new(RwLock::new(BTreeMap::new())),
            mentions: Arc::new(RwLock::new(BTreeMap::new())),
            search: Arc::new(RwLock::new(
                SearchProjection::new().expect("in-memory Tantivy projection must initialize"),
            )),
        }
    }

    fn read_pages(
        &self,
    ) -> StoreResult<std::sync::RwLockReadGuard<'_, BTreeMap<String, PageIndex>>> {
        self.pages
            .read()
            .map_err(|_| StoreError::Operation("memory index lock poisoned".to_string()))
    }

    fn write_pages(
        &self,
    ) -> StoreResult<std::sync::RwLockWriteGuard<'_, BTreeMap<String, PageIndex>>> {
        self.pages
            .write()
            .map_err(|_| StoreError::Operation("memory index lock poisoned".to_string()))
    }

    fn rebuild_search(&self) -> StoreResult<()> {
        let pages = self
            .pages
            .read()
            .map_err(|_| StoreError::Operation("memory index lock poisoned".to_string()))?;
        self.search
            .write()
            .map_err(|_| StoreError::Operation("memory search lock poisoned".to_string()))?
            .rebuild(&pages.values().cloned().collect::<Vec<_>>())
    }
}

#[async_trait]
impl IndexReader for MemoryIndex {
    async fn capabilities(&self) -> StoreResult<IndexCapabilities> {
        Ok(IndexCapabilities {
            durable: false,
            full_text_search: true,
            fuzzy_page_search: false,
            transactions: true,
            remote_sync: false,
        })
    }

    async fn list_pages(&self) -> StoreResult<Vec<PageSummary>> {
        Ok(self
            .read_pages()?
            .values()
            .map(|page| page.summary.clone())
            .collect())
    }

    async fn page(&self, path: &str) -> StoreResult<Option<PageSummary>> {
        Ok(self
            .read_pages()?
            .get(path)
            .map(|page| page.summary.clone()))
    }

    async fn search(&self, request: SearchRequest) -> StoreResult<Vec<SearchHit>> {
        self.search
            .read()
            .map_err(|_| StoreError::Operation("memory search lock poisoned".to_string()))?
            .search(&request)
    }

    async fn backlinks(&self, path: &str) -> StoreResult<Vec<Backlink>> {
        let pages = self.read_pages()?;
        let summaries = pages
            .values()
            .map(|page| page.summary.clone())
            .collect::<Vec<_>>();
        Ok(pages
            .values()
            .filter(|page| {
                page.summary.path != path
                    && page.links.iter().any(|link| {
                        link.kind == miku_domain::LinkKind::Page
                            && miku_indexer::resolve_link_path(&link.target_norm, &summaries)
                                .as_deref()
                                == Some(path)
                    })
            })
            .map(|page| Backlink {
                path: page.summary.path.clone(),
                title: page.summary.title.clone(),
            })
            .collect())
    }

    async fn mentions_for_target(&self, path: &str) -> StoreResult<Vec<MentionRecord>> {
        Ok(self
            .mentions
            .read()
            .map_err(|_| StoreError::Operation("memory mention lock poisoned".to_string()))?
            .values()
            .filter(|mention| mention.target_path == path)
            .cloned()
            .collect())
    }

    async fn mentions_ready(&self) -> StoreResult<bool> {
        Ok(true)
    }

    async fn tags(&self) -> StoreResult<Vec<TagCount>> {
        let mut counts = BTreeMap::new();
        for page in self.read_pages()?.values() {
            for tag in &page.tags {
                *counts.entry(tag.clone()).or_insert(0) += 1;
            }
        }
        Ok(counts
            .into_iter()
            .map(|(tag, count)| TagCount { tag, count })
            .collect())
    }

    async fn pages_with_tag(&self, tag: &str) -> StoreResult<Vec<PageSummary>> {
        Ok(self
            .read_pages()?
            .values()
            .filter(|page| page.tags.iter().any(|candidate| candidate == tag))
            .map(|page| page.summary.clone())
            .collect())
    }
}

#[async_trait]
impl IndexWriter for MemoryIndex {
    async fn replace_page(&self, page: PageIndex) -> StoreResult<IndexEvent> {
        let path = page.summary.path.clone();
        self.write_pages()?.insert(path.clone(), page);
        self.rebuild_search()?;
        Ok(IndexEvent::PageIndexed { path })
    }

    async fn rebuild_search_index(&self) -> StoreResult<()> {
        self.rebuild_search()
    }

    async fn replace_pages(&self, pages: Vec<PageIndex>) -> StoreResult<Vec<IndexEvent>> {
        if pages.is_empty() {
            return Ok(Vec::new());
        }
        let events = pages
            .iter()
            .map(|page| IndexEvent::PageIndexed {
                path: page.summary.path.clone(),
            })
            .collect();
        let mut indexed = self.write_pages()?;
        for page in pages {
            indexed.insert(page.summary.path.clone(), page);
        }
        drop(indexed);
        // Bulk callers rebuild the search projection once after all batches
        // have been loaded. Rebuilding here would make a full reconcile
        // quadratic in the number of batches.
        Ok(events)
    }

    async fn hydrate_hot_pages(&self, pages: Vec<PageIndex>) -> StoreResult<()> {
        self.replace_pages(pages).await.map(|_| ())
    }

    async fn replace_mentions_for_source(
        &self,
        source_path: &str,
        mentions: Vec<MentionRecord>,
    ) -> StoreResult<()> {
        let mut indexed = self
            .mentions
            .write()
            .map_err(|_| StoreError::Operation("memory mention lock poisoned".to_string()))?;
        indexed.retain(|(_, source, _), _| source != source_path);
        for mention in mentions {
            indexed.insert(
                (
                    mention.target_path.clone(),
                    mention.source_path.clone(),
                    mention.matched_text.to_lowercase(),
                ),
                mention,
            );
        }
        Ok(())
    }

    async fn replace_mentions_for_sources(
        &self,
        entries: Vec<(String, Vec<MentionRecord>)>,
    ) -> StoreResult<()> {
        let mut indexed = self
            .mentions
            .write()
            .map_err(|_| StoreError::Operation("memory mention lock poisoned".to_string()))?;
        for (source_path, mentions) in entries {
            indexed.retain(|(_, source, _), _| source != &source_path);
            for mention in mentions {
                indexed.insert(
                    (
                        mention.target_path.clone(),
                        mention.source_path.clone(),
                        mention.matched_text.to_lowercase(),
                    ),
                    mention,
                );
            }
        }
        Ok(())
    }

    async fn delete_mentions_for_source(&self, source_path: &str) -> StoreResult<()> {
        self.mentions
            .write()
            .map_err(|_| StoreError::Operation("memory mention lock poisoned".to_string()))?
            .retain(|(_, source, _), _| source != source_path);
        Ok(())
    }

    async fn delete_mentions_for_target(&self, target_path: &str) -> StoreResult<()> {
        self.mentions
            .write()
            .map_err(|_| StoreError::Operation("memory mention lock poisoned".to_string()))?
            .retain(|(target, _, _), _| target != target_path);
        Ok(())
    }

    async fn delete_mentions_for_targets(&self, target_paths: Vec<String>) -> StoreResult<()> {
        self.mentions
            .write()
            .map_err(|_| StoreError::Operation("memory mention lock poisoned".to_string()))?
            .retain(|(target, _, _), _| !target_paths.iter().any(|path| path == target));
        Ok(())
    }

    async fn mark_mentions_ready(&self) -> StoreResult<()> {
        Ok(())
    }

    async fn delete_page(&self, path: &str) -> StoreResult<IndexEvent> {
        self.write_pages()?.remove(path);
        self.rebuild_search()?;
        Ok(IndexEvent::PageDeleted {
            path: path.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use miku_domain::{LinkKind, LinkRecord, PageSummary, SearchScope};

    fn page(path: &str, title: &str, body: &str) -> PageIndex {
        PageIndex {
            summary: PageSummary {
                path: path.to_string(),
                title: title.to_string(),
                frontmatter: serde_json::json!({}),
                mtime: 1,
            },
            body: body.to_string(),
            links: Vec::new(),
            tags: vec!["notes".to_string()],
            aliases: Vec::new(),
            has_mermaid: false,
            signals: Default::default(),
        }
    }

    #[tokio::test]
    async fn supports_search_backlinks_mentions_and_tags() {
        let index = MemoryIndex::new();
        let mut source = page("Source.md", "Source", "Today references Index.");
        source.links.push(LinkRecord {
            target: "Index".to_string(),
            target_norm: "index".to_string(),
            alias: None,
            kind: LinkKind::Page,
            is_embed: false,
        });
        index.replace_page(source).await.expect("source indexed");
        index
            .replace_page(page("Index.md", "Index", "The home page."))
            .await
            .expect("index indexed");

        let hits = index
            .search(SearchRequest {
                query: "home".to_string(),
                scope: SearchScope::Body,
                limit: 10,
            })
            .await
            .expect("search works");
        assert_eq!(hits.len(), 1);
        assert_eq!(
            index.backlinks("Index.md").await.expect("backlinks").len(),
            1
        );
        assert_eq!(
            index
                .mentions_for_target("Index.md")
                .await
                .expect("mentions")
                .len(),
            0
        );

        index
            .replace_mentions_for_source(
                "Source.md",
                vec![MentionRecord {
                    target_path: "Index.md".to_string(),
                    source_path: "Source.md".to_string(),
                    source_title: "Source".to_string(),
                    matched_text: "Index".to_string(),
                    snippet: "Today references Index.".to_string(),
                }],
            )
            .await
            .expect("replace mentions");
        assert_eq!(
            index.mentions_for_target("Index.md").await.unwrap().len(),
            1
        );
        assert_eq!(index.tags().await.expect("tags")[0].count, 2);
    }

    #[tokio::test]
    async fn backlinks_cover_same_layer_cross_layer_and_global_conflicts() {
        let unique = MemoryIndex::new();
        let mut same_source = page("same/Source.md", "Source", "[[Target]]");
        same_source.links.push(LinkRecord {
            target: "Target".to_string(),
            target_norm: "target".to_string(),
            alias: None,
            kind: LinkKind::Page,
            is_embed: false,
        });
        unique.replace_page(same_source).await.expect("same source");
        unique
            .replace_page(page("same/Target.md", "Target", "target"))
            .await
            .expect("same target");
        assert_eq!(unique.backlinks("same/Target.md").await.unwrap().len(), 1);

        let conflict = MemoryIndex::new();
        let mut explicit_source = page("same/Explicit.md", "Explicit", "[[other/Target]]");
        explicit_source.links.push(LinkRecord {
            target: "other/Target".to_string(),
            target_norm: "other/target".to_string(),
            alias: None,
            kind: LinkKind::Page,
            is_embed: false,
        });
        conflict
            .replace_page(explicit_source)
            .await
            .expect("explicit source");
        let mut ambiguous_source = page("same/Ambiguous.md", "Ambiguous", "[[Target]]");
        ambiguous_source.links.push(LinkRecord {
            target: "Target".to_string(),
            target_norm: "target".to_string(),
            alias: None,
            kind: LinkKind::Page,
            is_embed: false,
        });
        conflict
            .replace_page(ambiguous_source)
            .await
            .expect("ambiguous source");
        conflict
            .replace_page(page("same/Target.md", "Target", "target"))
            .await
            .expect("same target");
        conflict
            .replace_page(page("other/Target.md", "Target", "target"))
            .await
            .expect("other target");

        assert!(conflict
            .backlinks("same/Target.md")
            .await
            .unwrap()
            .is_empty());
        let cross_layer = conflict.backlinks("other/Target.md").await.unwrap();
        assert_eq!(cross_layer.len(), 1);
        assert_eq!(cross_layer[0].path, "same/Explicit.md");
    }

    #[tokio::test]
    async fn rebuild_removes_deleted_documents_from_tantivy() {
        let index = MemoryIndex::new();
        index
            .replace_page(page("Gone.md", "Gone", "ephemeral content"))
            .await
            .expect("index document");
        assert_eq!(
            index
                .search(SearchRequest {
                    query: "ephemeral".to_string(),
                    scope: SearchScope::Body,
                    limit: 10,
                })
                .await
                .expect("search before delete")
                .len(),
            1
        );

        index.delete_page("Gone.md").await.expect("delete document");
        index.rebuild_search_index().await.expect("rebuild search");
        assert!(index
            .search(SearchRequest {
                query: "ephemeral".to_string(),
                scope: SearchScope::Body,
                limit: 10,
            })
            .await
            .expect("search after delete")
            .is_empty());
    }
}
