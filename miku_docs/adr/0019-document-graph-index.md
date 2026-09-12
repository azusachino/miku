---
id: ADR-0019
type: adr
title: ADR-0019 — In-memory document-graph index
slug: document-graph-index
status: Accepted
updated: 2026-07-29
date-proposed: 2026-07-28
date-accepted: 2026-07-28
deciders: [haru]
mirror: asobi:miku:decision:document-graph-index
supersedes: []
superseded-by:
relates-to: [ADR-0009, ADR-0015, ADR-0016, ADR-0018, ADR-0020]
impacts: [crates/miku-domain, crates/miku-index-memory, crates/miku-index-sqlite]
tags: [index, links, performance, architecture]
---

## ADR-0019 — In-memory document-graph index

### Current status

The in-memory graph decision remains active. ADR-0020 supersedes this record's historical FTS5/Tantivy search details and completed the memory-cap follow-up by moving body search to SQLite's plain
`TEXT` column and stripping bodies from MemoryIndex.

### Decision

Link, slug, alias, tag, and backlink resolution for `[[wikilink]]` graphs is owned entirely by the in-process `MemoryIndex` (`crates/miku-index-memory`) as plain Rust data structures, not by SQL:

- `slug_index: HashMap<Slug, Vec<Path>>` gives an O(1) point lookup for resolving `[[target]]`.
- `backlinks: HashMap<TargetPath, HashSet<SourcePath>>` gives an O(1) point lookup for backlinks, updated incrementally per changed page rather than rebuilt over the full corpus.

`crates/miku-index-sqlite` is demoted to a simple key/value and full-text cache: it stores raw page metadata (`tb_pages(path, json_doc)`) for offline/durable recovery and `tb_pages_fts` for FTS5 search. It no longer maintains `tb_links`, `tb_page_aliases`, or `tb_tags` as relational join targets, and it performs no cross-page `UPDATE ... JOIN` resolution sweep.

This keeps SQLite as the `DurableProjection` per ADR-0009 and ADR-0018, but narrows its responsibility to durable storage and search-candidate indexing; the page graph itself lives only in the hot `MemoryIndex` projection, consistent with ADR-0018's stated split between `DurableProjection` and `SearchProjection`.

This retires the remaining part of ADR-0016's design that survived ADR-0017: ADR-0017 already rejected SQLite as the domain/note-identity model, but ADR-0016's relational `tb_links`/`tb_tags`/`tb_page_aliases` schema and `UPDATE ... JOIN` resolve passes stayed in `crates/miku-index-sqlite` as an implementation detail. ADR-0016 has been updated to note this.

**The hot `MemoryIndex` projection must not hold the full corpus resident without bound.** Regardless of vault size, total resident memory attributable to the hot projection (page cache + search index) is capped at **100 MB**. Unbounded full residency of every page body and a fully in-RAM Tantivy index — the behavior inherited from ADR-0017/0018 and still in place today — is explicitly **not approved** and is superseded by this cap. Meeting it requires two follow-up changes, tracked as new work rather than assumed complete by this ADR:

1. A bounded page-body cache in front of `SqliteIndex`, evicting via a recency/frequency policy (e.g. MySQL InnoDB buffer-pool-style LRU with midpoint insertion, so a one-time full-corpus scan can't evict the real working set) instead of `MemoryIndex` holding every page's full body in `pages: BTreeMap<String, PageIndex>` forever.
2. Moving Tantivy from `Index::create_in_ram` to an on-disk directory, so its postings/term dictionary/positions are memory-mapped and paged in on demand instead of fully resident.

### Why

The current `SqliteIndex` still resolves wikilinks relationally: `tb_links.target_norm` is matched against `tb_pages.path`/`slug` with batched `UPDATE tb_links ... JOIN tb_pages` sweeps (`crates/miku-index-sqlite/src/lib.rs`), and `MemoryIndex::rebuild_backlinks` recomputes backlinks for every page on every single-page write (`crates/miku-index-memory/src/lib.rs`). Because wikilink resolution requires fuzzy stem, slug, and alias matching across the ~14,000-file vault, these sweeps compared on the order of 700 million rows per batch during startup reconcile, taking 23.8 seconds to 4 minutes and triggering SQLx slow-statement warnings.

Moving resolution into pure in-memory maps removes the relational join entirely: single-file edits resolve links in under 0.001 ms, and reconciling the full corpus drops from ~23.8 s to ~300 ms. The `slug_index`/`backlinks` HashMaps themselves are estimated to scale linearly and stay small: ~1.4 MB at 1,000 notes, ~20.1 MB at the current 14,337-note vault, ~137 MB at 100,000 notes.

That estimate describes the graph structures only. It does **not** cover, and was never meant to justify, the memory used by `MemoryIndex` overall. Measured against the live 15,910-file / 280 MB `miku_docs` corpus, `MemoryIndex` grows process RSS by **~2.27 GB — about 8x the raw corpus size** — because it holds every page's full body plus a complete in-memory Tantivy index. That was not a reviewed, approved trade-off: it is the pre-existing ADR-0017/0018 "hot projection" design, which assumed full residency was harmless but was never bounded or measured against a real vault until this ADR's own benchmark surfaced it. 8x amplification at only 16k files does not scale to the 100k-note case this ADR already extrapolates to, so this ADR now also caps total hot-projection memory (see Decision).

### Trade-offs / Rejected

- Rejected keeping relational resolution in SQLite for durability's sake: the join-based sweep does not scale with vault size and re-triggers on every reconcile, which is disproportionate to the value SQL joins add over a point lookup.
- Rejected making SQLite fully disposable (index-only cache with no independent recovery value): `tb_pages(path, json_doc)` and `tb_pages_fts` remain durable so a process can recover page content and search without replaying the full Markdown corpus.
- Accepted that the page graph becomes fully process-local and rebuilds from scratch on cold start; this is consistent with ADR-0018's existing hot-projection rebuild behavior for Tantivy and does not introduce a new consistency risk.
- Deferred alias- and tag-index restructuring beyond what is needed for slug/backlink resolution; `tb_tags`/`tb_page_aliases` removal from the relational schema follows once the memory-side structures cover the same queries.

### Historical implementation status before ADR-0020

**Partially implemented.** The link-graph resolution work is done: `crates/miku-index-memory` resolves links via `LinkGraph`'s `slug_index`/`path_index` with incremental `upsert_page`/`remove_page` (no full-corpus rebuild on single-page writes), and `crates/miku-index-sqlite` no longer performs relational link/tag/alias resolution; `tb_links`, `tb_tags`, and `tb_page_aliases` are removed from the schema. This part was tracked as `miku:document-graph-index` in asobi (tasks 1-5, all DONE).

**The 100 MB hot-projection memory cap in Decision is not met and is open, unstarted work**, not covered by tasks 1-5 above. It requires the bounded page-body cache and on-disk Tantivy directory described in Decision, and should be tracked as its own asobi epic before implementation begins.

Measured against the live `miku_docs` corpus (15,910 files, `make benchmark-real-vault`, release build, Apple Silicon):

- **Reconcile time:** 5.17-5.31 s end to end (walk + parse + index + graph build), down from the pre-ADR baseline of 23.8 s-4 min. This is a substantial, real improvement, but higher than the "~300 ms" figure in Why above: that number described only the graph-resolution step in isolation. At today's scale, Markdown parsing, Tantivy indexing, and disk I/O dominate the remaining ~5 s, not link/slug/backlink resolution, which the benchmarks in `crates/miku-index-memory` (task-2) confirm stays flat (~10 µs/update) regardless of corpus size.
- **Memory:** process RSS grew by ~2.27 GB reconciling the full corpus (280 MB of raw Markdown), ~23x over the 100 MB cap this ADR now sets. The graph structures (`slug_index`/`backlinks`) are a negligible fraction of that; the overwhelming majority is full page bodies/frontmatter in `MemoryIndex`'s `BTreeMap<String, PageIndex>` plus a complete in-memory Tantivy index — both pre-existing ADR-0017/0018 behavior, unchanged by tasks 1-5, and not yet remediated.
