//! Deterministic in-memory [`miku_domain::IndexStore`] implementation.
//!
//! This is the reference behavior for contract tests and disposable
//! development. It is not a durable deployment backend.

use async_trait::async_trait;
use miku_domain::{
    Backlink, IndexCapabilities, IndexEvent, IndexReader, IndexWriter, MentionRecord, PageIndex,
    PageSummary, SearchHit, SearchRequest, StoreError, StoreResult, TagCount,
};
use std::collections::BTreeMap;
use std::sync::{Arc, RwLock};

/// An in-memory index keyed by source-relative page path.
#[derive(Clone, Default)]
pub struct MemoryIndex {
    pages: Arc<RwLock<BTreeMap<String, PageIndex>>>,
    mentions: Arc<RwLock<BTreeMap<(String, String, String), MentionRecord>>>,
}

impl MemoryIndex {
    /// Create an empty reference index.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
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
        let query = request.query.trim().to_lowercase();
        if query.is_empty() || request.limit == 0 {
            return Ok(Vec::new());
        }

        let terms: Vec<&str> = query.split_whitespace().collect();
        let mut hits = self
            .read_pages()?
            .values()
            .filter_map(|page| {
                let path = page.summary.path.to_lowercase();
                let title = page.summary.title.to_lowercase();
                let body = page.body.to_lowercase();
                let haystack = match request.scope {
                    miku_domain::SearchScope::All => format!("{path} {title} {body}"),
                    miku_domain::SearchScope::Title => format!("{path} {title}"),
                    miku_domain::SearchScope::Body => body.clone(),
                };
                terms
                    .iter()
                    .all(|term| haystack.contains(term))
                    .then(|| SearchHit {
                        path: page.summary.path.clone(),
                        title: page.summary.title.clone(),
                        snippet: snippet(&page.body, &query),
                    })
            })
            .collect::<Vec<_>>();
        hits.truncate(request.limit);
        Ok(hits)
    }

    async fn backlinks(&self, path: &str) -> StoreResult<Vec<Backlink>> {
        let target = miku_indexer::page_slug(path);
        Ok(self
            .read_pages()?
            .values()
            .filter(|page| {
                page.summary.path != path
                    && page.links.iter().any(|link| {
                        link.kind == miku_domain::LinkKind::Page && link.target_norm == target
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
        Ok(IndexEvent::PageIndexed { path })
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
        Ok(events)
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

    async fn mark_mentions_ready(&self) -> StoreResult<()> {
        Ok(())
    }

    async fn delete_page(&self, path: &str) -> StoreResult<IndexEvent> {
        self.write_pages()?.remove(path);
        Ok(IndexEvent::PageDeleted {
            path: path.to_string(),
        })
    }
}

fn snippet(body: &str, query: &str) -> String {
    let lower = body.to_lowercase();
    let start = query
        .split_whitespace()
        .find_map(|term| lower.find(term))
        .unwrap_or(0);
    let start_chars = lower[..start].chars().count();
    body.chars().skip(start_chars).take(160).collect()
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
}
