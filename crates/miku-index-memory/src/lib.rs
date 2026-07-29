//! Deterministic in-memory graph [`miku_domain::IndexStore`] implementation.
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

mod graph;

use graph::LinkGraph;

type MentionKey = (String, String, String);
type MentionMap = BTreeMap<MentionKey, MentionRecord>;

/// An in-memory index keyed by source-relative page path.
#[derive(Clone)]
pub struct MemoryIndex {
    pages: Arc<RwLock<BTreeMap<String, PageIndex>>>,
    graph: Arc<RwLock<LinkGraph>>,
    mentions: Arc<RwLock<MentionMap>>,
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
            graph: Arc::new(RwLock::new(LinkGraph::default())),
            mentions: Arc::new(RwLock::new(BTreeMap::new())),
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

    fn write_graph(&self) -> StoreResult<std::sync::RwLockWriteGuard<'_, LinkGraph>> {
        self.graph
            .write()
            .map_err(|_| StoreError::Operation("memory graph lock poisoned".to_string()))
    }

    fn read_graph(&self) -> StoreResult<std::sync::RwLockReadGuard<'_, LinkGraph>> {
        self.graph
            .read()
            .map_err(|_| StoreError::Operation("memory graph lock poisoned".to_string()))
    }

    fn rebuild_graph(&self) -> StoreResult<()> {
        let pages = self.read_pages()?;
        self.write_graph()?.rebuild_all(&pages);
        Ok(())
    }
}

#[async_trait]
impl IndexReader for MemoryIndex {
    async fn capabilities(&self) -> StoreResult<IndexCapabilities> {
        Ok(IndexCapabilities {
            durable: false,
            full_text_search: false,
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

    async fn search(&self, _request: SearchRequest) -> StoreResult<Vec<SearchHit>> {
        Ok(Vec::new())
    }

    async fn backlinks(&self, path: &str) -> StoreResult<Vec<Backlink>> {
        let pages = self.read_pages()?;
        let index = self.read_graph()?;
        Ok(graph::backlinks_for(&index, path, &pages))
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
        let normalized = miku_markdown::normalize_tag(tag);

        Ok(self
            .read_pages()?
            .values()
            .filter(|page| page.tags.iter().any(|candidate| candidate == &normalized))
            .map(|page| page.summary.clone())
            .collect())
    }
}

#[async_trait]
impl IndexWriter for MemoryIndex {
    async fn replace_page(&self, mut page: PageIndex) -> StoreResult<IndexEvent> {
        let path = page.summary.path.clone();
        let links = page.links.clone();
        page.body.clear();
        page.body.shrink_to_fit();
        page.summary.frontmatter = serde_json::Value::Null;
        let mut pages = self.write_pages()?;
        let previous = pages.insert(path.clone(), page);
        self.write_graph()?.upsert_page(
            &path,
            &links,
            previous.as_ref().map(|old| old.links.as_slice()),
            &pages,
        );
        drop(pages);
        Ok(IndexEvent::PageIndexed { path })
    }

    async fn rebuild_search_index(&self) -> StoreResult<()> {
        self.rebuild_graph()
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
        for mut page in pages {
            page.body.clear();
            page.body.shrink_to_fit();
            page.summary.frontmatter = serde_json::Value::Null;
            indexed.insert(page.summary.path.clone(), page);
        }
        drop(indexed);
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
        let mut pages = self.write_pages()?;
        let removed = pages.remove(path);
        if let Some(old_page) = &removed {
            self.write_graph()?
                .remove_page(path, &old_page.links, &pages);
        }
        drop(pages);
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
        page_with_aliases(path, title, body, Vec::new())
    }

    fn page_with_aliases(path: &str, title: &str, body: &str, aliases: Vec<&str>) -> PageIndex {
        let aliases: Vec<String> = aliases.into_iter().map(str::to_string).collect();
        PageIndex {
            summary: PageSummary {
                path: path.to_string(),
                title: title.to_string(),
                frontmatter: serde_json::json!({}),
                mtime: 1,
                aliases: aliases.clone(),
            },
            body: body.to_string(),
            links: Vec::new(),
            tags: vec!["notes".to_string()],
            aliases,
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
        assert!(hits.is_empty());
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

    fn link(target: &str, target_norm: &str) -> LinkRecord {
        LinkRecord {
            target: target.to_string(),
            target_norm: target_norm.to_string(),
            alias: None,
            kind: LinkKind::Page,
            is_embed: false,
        }
    }

    #[tokio::test]
    async fn backlinks_resolve_standard_markdown_links_not_just_wikilinks() {
        // ADR-README-style source: only standard [text](path) links, no
        // wikilinks. Runs real extraction (miku_indexer::build_page_index)
        // rather than hand-building LinkRecords, so it exercises the actual
        // Markdown-link regex, not just the graph's generic link handling.
        let index = MemoryIndex::new();
        let mut source = page(
            "adr/README.md",
            "ADR Index",
            "- [0001](0001-fts-english.md)\n- [0002](0002-other.md)",
        );
        source.links = miku_indexer::build_page_index(
            &source.summary.path,
            source.body.as_bytes(),
            source.summary.mtime,
        )
        .links;
        index.replace_page(source).await.expect("source indexed");
        index
            .replace_page(page("adr/0001-fts-english.md", "0001", "content"))
            .await
            .expect("target indexed");

        let backlinks = index.backlinks("adr/0001-fts-english.md").await.unwrap();
        assert_eq!(backlinks.len(), 1);
        assert_eq!(backlinks[0].path, "adr/README.md");
    }

    #[tokio::test]
    async fn delete_restores_ambiguous_uniqueness_for_pending_source() {
        let index = MemoryIndex::new();
        let mut source = page("Source.md", "Source", "[[Target]]");
        source.links.push(link("Target", "target"));
        index.replace_page(source).await.expect("source indexed");
        index
            .replace_page(page("same/Target.md", "Target", "t"))
            .await
            .expect("unique target indexed");
        assert_eq!(
            index.backlinks("same/Target.md").await.unwrap().len(),
            1,
            "unique slug must resolve"
        );

        index
            .replace_page(page("other/Target.md", "Target", "t"))
            .await
            .expect("ambiguous duplicate indexed");
        assert!(
            index.backlinks("same/Target.md").await.unwrap().is_empty(),
            "ambiguous slug must retract the prior backlink"
        );

        index
            .delete_page("other/Target.md")
            .await
            .expect("delete duplicate");
        assert_eq!(
            index.backlinks("same/Target.md").await.unwrap().len(),
            1,
            "removing the duplicate must restore the unique resolution"
        );
    }

    #[tokio::test]
    async fn updating_a_pages_links_moves_its_backlink_contribution() {
        let index = MemoryIndex::new();
        index
            .replace_page(page("A.md", "A", "a"))
            .await
            .expect("A indexed");
        index
            .replace_page(page("B.md", "B", "b"))
            .await
            .expect("B indexed");

        let mut source = page("Source.md", "Source", "[[A]]");
        source.links.push(link("A", "a"));
        index.replace_page(source).await.expect("source -> A");
        assert_eq!(index.backlinks("A.md").await.unwrap().len(), 1);
        assert!(index.backlinks("B.md").await.unwrap().is_empty());

        let mut retargeted = page("Source.md", "Source", "[[B]]");
        retargeted.links.push(link("B", "b"));
        index.replace_page(retargeted).await.expect("source -> B");
        assert!(
            index.backlinks("A.md").await.unwrap().is_empty(),
            "stale pending registration must not keep A's backlink alive"
        );
        assert_eq!(index.backlinks("B.md").await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn incremental_updates_match_full_rebuild() {
        let mut ambiguous_source = page("same/Source.md", "Source", "[[Target]]");
        ambiguous_source.links.push(link("Target", "target"));
        let mut explicit_source = page("same/Explicit.md", "Explicit", "[[other/Target]]");
        explicit_source
            .links
            .push(link("other/Target", "other/target"));
        let mut self_referencing = page("Self.md", "Self", "[[Self]]");
        self_referencing.links.push(link("Self", "self"));
        let mut alias_source = page("Alias-Source.md", "Alias Source", "[[boss log]]");
        alias_source.links.push(link("boss log", "bosslog"));

        let pages = vec![
            ambiguous_source,
            explicit_source,
            page("same/Target.md", "Target", "t"),
            page("other/Target.md", "Target", "t"),
            self_referencing,
            alias_source,
            page_with_aliases("aliased/Note.md", "Aliased Note", "n", vec!["Boss-Log"]),
        ];

        let incremental = MemoryIndex::new();
        for page in pages.clone() {
            incremental
                .replace_page(page)
                .await
                .expect("incremental insert");
        }

        let bulk = MemoryIndex::new();
        bulk.replace_pages(pages.clone())
            .await
            .expect("bulk insert");
        bulk.rebuild_search_index()
            .await
            .expect("full graph rebuild");

        for page in &pages {
            let path = &page.summary.path;
            let mut via_incremental = incremental.backlinks(path).await.unwrap();
            let mut via_full_rebuild = bulk.backlinks(path).await.unwrap();
            via_incremental.sort_by(|left, right| left.path.cmp(&right.path));
            via_full_rebuild.sort_by(|left, right| left.path.cmp(&right.path));
            assert_eq!(
                via_incremental, via_full_rebuild,
                "incremental and full-rebuild backlinks diverged for {path}"
            );
        }
    }

    #[tokio::test]
    async fn backlinks_resolve_wikilinks_by_title_and_alias() {
        let index = MemoryIndex::new();
        index
            .replace_page(page_with_aliases(
                "elden-ring.md",
                "Elden Ring",
                "notes",
                vec!["ER Boss Log"],
            ))
            .await
            .expect("target indexed");

        let mut by_title = page("BySlug.md", "By Title", "[[Elden Ring]]");
        by_title.links.push(link("Elden Ring", "eldenring"));
        index.replace_page(by_title).await.expect("title source");

        let mut by_alias = page("ByAlias.md", "By Alias", "[[ER Boss Log]]");
        by_alias.links.push(link("ER Boss Log", "erbosslog"));
        index.replace_page(by_alias).await.expect("alias source");

        let mut by_folded_slug = page("ByFoldedSlug.md", "By Folded Slug", "[[elden ring]]");
        by_folded_slug.links.push(link("elden ring", "eldenring"));
        index
            .replace_page(by_folded_slug)
            .await
            .expect("folded slug source");

        let mut backlinks = index.backlinks("elden-ring.md").await.unwrap();
        backlinks.sort_by(|left, right| left.path.cmp(&right.path));
        assert_eq!(
            backlinks
                .iter()
                .map(|b| b.path.as_str())
                .collect::<Vec<_>>(),
            vec!["ByAlias.md", "ByFoldedSlug.md", "BySlug.md"]
        );
    }

    #[tokio::test]
    async fn editing_an_alias_incrementally_moves_its_backlink_contribution() {
        let index = MemoryIndex::new();
        index
            .replace_page(page_with_aliases(
                "target.md",
                "Target",
                "notes",
                vec!["Old Alias"],
            ))
            .await
            .expect("target indexed");

        let mut source = page("Source.md", "Source", "[[Old Alias]]");
        source.links.push(link("Old Alias", "oldalias"));
        index.replace_page(source).await.expect("source indexed");
        assert_eq!(index.backlinks("target.md").await.unwrap().len(), 1);

        // Renaming the alias without touching links must retract the stale
        // resolution: "Old Alias" no longer belongs to any page.
        index
            .replace_page(page_with_aliases(
                "target.md",
                "Target",
                "notes",
                vec!["New Alias"],
            ))
            .await
            .expect("alias renamed");
        assert!(
            index.backlinks("target.md").await.unwrap().is_empty(),
            "stale alias registration must not keep the backlink alive"
        );

        let mut retargeted = page("Source.md", "Source", "[[New Alias]]");
        retargeted.links.push(link("New Alias", "newalias"));
        index
            .replace_page(retargeted)
            .await
            .expect("source -> new alias");
        assert_eq!(index.backlinks("target.md").await.unwrap().len(), 1);
    }

    #[tokio::test]
    async fn ambiguous_titles_do_not_auto_resolve_backlinks() {
        let index = MemoryIndex::new();
        let mut source = page("Source.md", "Source", "[[Shared Title]]");
        source.links.push(link("Shared Title", "sharedtitle"));
        index.replace_page(source).await.expect("source indexed");
        index
            .replace_page(page("one/Note.md", "Shared Title", "a"))
            .await
            .expect("first candidate indexed");
        assert_eq!(index.backlinks("one/Note.md").await.unwrap().len(), 1);

        index
            .replace_page(page("two/Note.md", "Shared Title", "b"))
            .await
            .expect("second candidate indexed");
        assert!(
            index.backlinks("one/Note.md").await.unwrap().is_empty(),
            "ambiguous title must retract the prior backlink"
        );
        assert!(index.backlinks("two/Note.md").await.unwrap().is_empty());
    }

    /// Exercises `LinkGraph` directly (not the full `MemoryIndex` API) so the
    /// measurement isolates graph-resolution cost from unrelated `RwLock`
    /// and page-storage overhead, which would otherwise swamp the signal
    /// this test is checking.
    #[test]
    fn single_page_update_cost_does_not_scale_with_corpus_size() {
        fn seed_and_time_one_update(corpus_size: usize) -> std::time::Duration {
            let mut graph = LinkGraph::default();
            let mut all_pages = BTreeMap::new();
            for i in 0..corpus_size {
                let target = (i + 1) % corpus_size;
                let mut note = page(
                    &format!("Note{i}.md"),
                    &format!("Note {i}"),
                    "body text linking onward",
                );
                note.links
                    .push(link(&format!("Note{target}"), &format!("note{target}")));
                let path = note.summary.path.clone();
                let links = note.links.clone();
                all_pages.insert(path.clone(), note);
                graph.upsert_page(&path, &links, None, &all_pages);
            }

            let previous = all_pages.get("Note0.md").map(|p| p.links.clone());
            let mut updated = page("Note0.md", "Note 0", "updated body, same links");
            updated.links.push(link("Note1", "note1"));
            all_pages.insert("Note0.md".to_string(), updated.clone());
            let started = std::time::Instant::now();
            graph.upsert_page("Note0.md", &updated.links, previous.as_deref(), &all_pages);
            started.elapsed()
        }

        let small = seed_and_time_one_update(200);
        let large = seed_and_time_one_update(4_000);

        println!(
            "benchmark=single-page-update small_pages=200 small_us={:.1} large_pages=4000 large_us={:.1}",
            small.as_secs_f64() * 1e6,
            large.as_secs_f64() * 1e6,
        );

        // A full-corpus rescan would grow roughly linearly with page count
        // (20x here); an O(1) incremental update stays flat. Generous
        // headroom and an absolute floor keep this stable under CI noise.
        assert!(
            large.as_secs_f64() < small.as_secs_f64() * 10.0 + 0.010,
            "single-page update cost scaled with corpus size: small={small:?} large={large:?}"
        );
    }
}
