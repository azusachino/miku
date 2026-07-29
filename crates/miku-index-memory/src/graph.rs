//! In-process document-graph projection: O(1) slug/path lookup and
//! incrementally maintained backlinks, replacing full-corpus rescans.
//!
//! See ADR-0019.

use std::collections::{HashMap, HashSet};

use miku_domain::{fold_name, page_names, Backlink, LinkKind, LinkRecord, PageIndex};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
enum LinkCandidate {
    /// An explicit cross-folder path link (may cross folders, never ambiguous).
    Path(String),
    /// A basename/title/alias link; ambiguous when more than one page shares
    /// the folded name.
    Slug(String),
}

fn link_candidate(target_norm: &str) -> LinkCandidate {
    let lowered = target_norm.to_lowercase();
    if lowered.contains('/') {
        LinkCandidate::Path(lowered)
    } else {
        LinkCandidate::Slug(fold_name(&lowered))
    }
}

fn path_key(path: &str) -> String {
    path.strip_suffix(".md").unwrap_or(path).to_lowercase()
}

/// Every folded name (filename, title, aliases) that should resolve to
/// `path`, deduplicated so a page whose title equals its filename only
/// registers one slug-index entry.
fn folded_names_for(path: &str, page: &PageIndex) -> HashSet<String> {
    page_names(path, &page.summary.title, &page.aliases)
        .into_iter()
        .map(|name| fold_name(&name))
        .filter(|folded| !folded.is_empty())
        .collect()
}

fn page_links(links: &[LinkRecord]) -> impl Iterator<Item = &LinkRecord> {
    links.iter().filter(|link| link.kind == LinkKind::Page)
}

/// Incrementally maintained slug/path resolution and backlink graph.
///
/// `slug_index` and `path_index` give O(1) point lookups for `[[target]]`
/// resolution: explicit `folder/name` links never guess across ambiguity,
/// basename/title/alias links require a globally unique candidate (folded
/// on whitespace/hyphen/underscore, so "elden ring" and "elden-ring" share
/// a slot). `registered_names` tracks exactly which folded names are
/// currently registered for each path, so a title or alias edit can be
/// diffed and reconciled without rescanning the corpus. `pending` tracks,
/// for every link candidate a page currently references, which source
/// paths must be re-resolved when that candidate's cardinality changes (a
/// page with that name is added, removed, or renamed) — bounding cascading
/// recomputation to the affected candidate's fan-in rather than rescanning
/// the whole corpus.
#[derive(Default)]
pub struct LinkGraph {
    slug_index: HashMap<String, Vec<String>>,
    path_index: HashMap<String, String>,
    registered_names: HashMap<String, HashSet<String>>,
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

    /// Reconcile `slug_index`/`registered_names` for `path` against
    /// `new_names`, returning the set of folded names that were added or
    /// removed (used to bound cascaded re-resolution of other pages'
    /// pending links). A no-op diff (title/aliases unchanged) returns an
    /// empty set and touches nothing.
    fn reconcile_names(&mut self, path: &str, new_names: HashSet<String>) -> HashSet<String> {
        let old_names = self.registered_names.remove(path).unwrap_or_default();
        let mut changed = HashSet::new();
        for removed in old_names.difference(&new_names) {
            changed.insert(removed.clone());
            if let Some(entries) = self.slug_index.get_mut(removed) {
                entries.retain(|candidate| candidate != path);
                if entries.is_empty() {
                    self.slug_index.remove(removed);
                }
            }
        }
        for added in new_names.difference(&old_names) {
            changed.insert(added.clone());
            let entries = self.slug_index.entry(added.clone()).or_default();
            if !entries.iter().any(|candidate| candidate == path) {
                entries.push(path.to_string());
            }
        }
        if !new_names.is_empty() {
            self.registered_names.insert(path.to_string(), new_names);
        }
        changed
    }

    /// Candidate sources that must be re-resolved because `path` just
    /// gained or lost one of `changed_names`, or was added/removed at
    /// `path` itself (bounded by those candidates' fan-in).
    fn cascaded_sources_for(&self, path: &str, changed_names: &HashSet<String>) -> HashSet<String> {
        let mut affected = HashSet::new();
        for name in changed_names {
            if let Some(sources) = self.pending.get(&LinkCandidate::Slug(name.clone())) {
                affected.extend(sources.iter().cloned());
            }
        }
        if let Some(sources) = self.pending.get(&LinkCandidate::Path(path_key(path))) {
            affected.extend(sources.iter().cloned());
        }
        affected.remove(path);
        affected
    }

    /// Insert or update `path`'s links. `previous_links` is the page's prior
    /// link list (`None` for a brand-new page), used to unregister stale
    /// pending entries before re-registering the current ones. Title/alias
    /// changes on an already-registered path are diffed against
    /// `registered_names` and reconciled the same way a new or removed page
    /// would be.
    pub fn upsert_page(
        &mut self,
        path: &str,
        links: &[LinkRecord],
        previous_links: Option<&[LinkRecord]>,
        all_pages: &std::collections::BTreeMap<String, PageIndex>,
    ) {
        if let Some(old_links) = previous_links {
            self.unregister_pending(path, old_links);
        }

        let new_names = all_pages
            .get(path)
            .map(|page| folded_names_for(path, page))
            .unwrap_or_default();
        let changed_names = self.reconcile_names(path, new_names);
        self.path_index.insert(path_key(path), path.to_string());
        let cascaded = if changed_names.is_empty() {
            HashSet::new()
        } else {
            self.cascaded_sources_for(path, &changed_names)
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
        self.registered_names.clear();
        self.resolved_links.clear();
        self.pending.clear();
        self.backlinks.clear();

        for (path, page) in all_pages {
            let names = folded_names_for(path, page);
            for name in &names {
                self.slug_index
                    .entry(name.clone())
                    .or_default()
                    .push(path.clone());
            }
            self.registered_names.insert(path.clone(), names);
            self.path_index.insert(path_key(path), path.clone());
        }
        for (path, page) in all_pages {
            self.register_pending(path, &page.links);
            self.recompute_source(path, &page.links);
        }
    }

    /// Remove `path` entirely: drop it as a resolution candidate, drop its
    /// own outgoing/incoming contributions, and re-resolve any source that
    /// depended on its filename, title, aliases, or path.
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

        let changed_names = self.reconcile_names(path, HashSet::new());
        let cascaded = self.cascaded_sources_for(path, &changed_names);
        self.path_index.remove(&path_key(path));

        for source in cascaded {
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
