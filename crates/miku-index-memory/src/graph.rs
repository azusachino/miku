//! In-process document-graph projection: O(1) slug/path lookup and
//! incrementally maintained backlinks, replacing full-corpus rescans.
//!
//! See ADR-0019.

use std::collections::{HashMap, HashSet};

use miku_domain::{Backlink, LinkKind, LinkRecord, PageIndex};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum LinkCandidate {
    /// An explicit cross-folder path link (may cross folders, never ambiguous).
    Path(String),
    /// A basename-only link; ambiguous when more than one page shares the slug.
    Slug(String),
}

fn link_candidate(target_norm: &str) -> LinkCandidate {
    let lowered = target_norm.to_lowercase();
    if lowered.contains('/') {
        LinkCandidate::Path(lowered)
    } else {
        LinkCandidate::Slug(lowered)
    }
}

fn path_key(path: &str) -> String {
    path.strip_suffix(".md").unwrap_or(path).to_lowercase()
}

fn page_links(links: &[LinkRecord]) -> impl Iterator<Item = &LinkRecord> {
    links.iter().filter(|link| link.kind == LinkKind::Page)
}

/// Incrementally maintained slug/path resolution and backlink graph.
///
/// `slug_index` and `path_index` give O(1) point lookups for `[[target]]`
/// resolution (mirroring `miku_indexer::resolve_link_path`'s semantics
/// exactly: explicit `folder/name` links never guess across ambiguity,
/// basename-only links require a globally unique candidate). `pending`
/// tracks, for every link candidate a page currently references, which
/// source paths must be re-resolved when that candidate's cardinality
/// changes (a page with that slug/path is added or removed) — bounding
/// cascading recomputation to the affected candidate's fan-in rather than
/// rescanning the whole corpus.
#[derive(Default)]
pub struct LinkGraph {
    slug_index: HashMap<String, Vec<String>>,
    path_index: HashMap<String, String>,
    resolved_links: HashMap<String, HashSet<String>>,
    pending: HashMap<LinkCandidate, HashSet<String>>,
    backlinks: HashMap<String, HashSet<String>>,
}

impl LinkGraph {
    pub fn backlink_sources(&self, path: &str) -> Vec<String> {
        self.backlinks
            .get(path)
            .map(|sources| sources.iter().cloned().collect())
            .unwrap_or_default()
    }

    fn resolve(&self, candidate: &LinkCandidate) -> Option<String> {
        match candidate {
            LinkCandidate::Path(key) => self.path_index.get(key).cloned(),
            LinkCandidate::Slug(slug) => {
                let matches = self.slug_index.get(slug)?;
                (matches.len() == 1).then(|| matches[0].clone())
            }
        }
    }

    fn unregister_pending(&mut self, source_path: &str, links: &[LinkRecord]) {
        let mut seen = HashSet::new();
        for link in page_links(links) {
            let candidate = link_candidate(&link.target_norm);
            if !seen.insert(candidate.clone()) {
                continue;
            }
            if let Some(sources) = self.pending.get_mut(&candidate) {
                sources.remove(source_path);
                if sources.is_empty() {
                    self.pending.remove(&candidate);
                }
            }
        }
    }

    fn register_pending(&mut self, source_path: &str, links: &[LinkRecord]) {
        let mut seen = HashSet::new();
        for link in page_links(links) {
            let candidate = link_candidate(&link.target_norm);
            if !seen.insert(candidate.clone()) {
                continue;
            }
            self.pending
                .entry(candidate)
                .or_default()
                .insert(source_path.to_string());
        }
    }

    /// Re-resolve one source page's outgoing links against the current
    /// slug/path indexes and update `backlinks` with only the difference.
    fn recompute_source(&mut self, source_path: &str, links: &[LinkRecord]) {
        let mut new_targets = HashSet::new();
        for link in page_links(links) {
            let candidate = link_candidate(&link.target_norm);
            if let Some(target) = self.resolve(&candidate) {
                if target != source_path {
                    new_targets.insert(target);
                }
            }
        }

        let old_targets = self.resolved_links.remove(source_path).unwrap_or_default();
        for removed in old_targets.difference(&new_targets) {
            if let Some(sources) = self.backlinks.get_mut(removed) {
                sources.remove(source_path);
                if sources.is_empty() {
                    self.backlinks.remove(removed);
                }
            }
        }
        for added in new_targets.difference(&old_targets) {
            self.backlinks
                .entry(added.clone())
                .or_default()
                .insert(source_path.to_string());
        }
        if !new_targets.is_empty() {
            self.resolved_links
                .insert(source_path.to_string(), new_targets);
        }
    }

    /// Candidate sources that must be re-resolved because a page was just
    /// added at or removed from `path` (bounded by that slug/path's fan-in).
    fn cascaded_sources(&self, path: &str) -> HashSet<String> {
        let slug = miku_indexer::page_slug(path);
        let mut affected = self
            .pending
            .get(&LinkCandidate::Slug(slug))
            .cloned()
            .unwrap_or_default();
        if let Some(sources) = self.pending.get(&LinkCandidate::Path(path_key(path))) {
            affected.extend(sources.iter().cloned());
        }
        affected.remove(path);
        affected
    }

    /// Insert or update `path`'s links. `previous_links` is the page's prior
    /// link list (`None` for a brand-new page), used to unregister stale
    /// pending entries before re-registering the current ones.
    pub fn upsert_page(
        &mut self,
        path: &str,
        links: &[LinkRecord],
        previous_links: Option<&[LinkRecord]>,
        all_pages: &std::collections::BTreeMap<String, PageIndex>,
    ) {
        let is_new_path = previous_links.is_none();
        if let Some(old_links) = previous_links {
            self.unregister_pending(path, old_links);
        }

        let cascaded = if is_new_path {
            let slug = miku_indexer::page_slug(path);
            self.slug_index
                .entry(slug)
                .or_default()
                .push(path.to_string());
            self.path_index.insert(path_key(path), path.to_string());
            self.cascaded_sources(path)
        } else {
            HashSet::new()
        };

        self.register_pending(path, links);
        self.recompute_source(path, links);

        for source in cascaded {
            if let Some(page) = all_pages.get(&source) {
                self.recompute_source(&source, &page.links);
            }
        }
    }

    /// Rebuild every slug/path/backlink entry from scratch. Still O(1) per
    /// link (via the slug/path indexes built in the first pass) rather than
    /// the O(pages) per-link scan `miku_indexer::resolve_link_path` performs
    /// against a raw page list, so a full-corpus rebuild stays cheap.
    pub fn rebuild_all(&mut self, all_pages: &std::collections::BTreeMap<String, PageIndex>) {
        self.slug_index.clear();
        self.path_index.clear();
        self.resolved_links.clear();
        self.pending.clear();
        self.backlinks.clear();

        for path in all_pages.keys() {
            let slug = miku_indexer::page_slug(path);
            self.slug_index.entry(slug).or_default().push(path.clone());
            self.path_index.insert(path_key(path), path.clone());
        }
        for (path, page) in all_pages {
            self.register_pending(path, &page.links);
            self.recompute_source(path, &page.links);
        }
    }

    /// Remove `path` entirely: drop it as a resolution candidate, drop its
    /// own outgoing/incoming contributions, and re-resolve any source that
    /// depended on its slug or path.
    pub fn remove_page(
        &mut self,
        path: &str,
        links: &[LinkRecord],
        all_pages: &std::collections::BTreeMap<String, PageIndex>,
    ) {
        self.unregister_pending(path, links);
        self.resolved_links.remove(path);
        self.backlinks.remove(path);
        for sources in self.backlinks.values_mut() {
            sources.remove(path);
        }

        let slug = miku_indexer::page_slug(path);
        if let Some(paths) = self.slug_index.get_mut(&slug) {
            paths.retain(|candidate| candidate != path);
            if paths.is_empty() {
                self.slug_index.remove(&slug);
            }
        }
        self.path_index.remove(&path_key(path));

        for source in self.cascaded_sources(path) {
            if let Some(page) = all_pages.get(&source) {
                self.recompute_source(&source, &page.links);
            }
        }
    }
}

/// Build `Backlink` entries for `path`, sorted by title then path to match
/// prior full-rebuild ordering.
pub fn backlinks_for(
    graph: &LinkGraph,
    path: &str,
    all_pages: &std::collections::BTreeMap<String, PageIndex>,
) -> Vec<Backlink> {
    let mut entries: Vec<Backlink> = graph
        .backlink_sources(path)
        .into_iter()
        .filter_map(|source_path| {
            all_pages.get(&source_path).map(|page| Backlink {
                path: page.summary.path.clone(),
                title: page.summary.title.clone(),
            })
        })
        .collect();
    entries.sort_by(|left, right| {
        left.title
            .cmp(&right.title)
            .then(left.path.cmp(&right.path))
    });
    entries
}
