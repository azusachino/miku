---
id: ADR-0020
type: adr
title: ADR-0020 — SQLite plain-content search, no FTS5, no Tantivy
slug: sqlite-only-search
status: Accepted
updated: 2026-07-28
date-proposed: 2026-07-28
date-accepted: 2026-07-28
deciders: [haru]
mirror: asobi:miku:decision:sqlite-only-search
supersedes: [ADR-0018]
superseded-by:
relates-to: [ADR-0009, ADR-0016, ADR-0019]
impacts: [crates/miku-domain, crates/miku-app, crates/miku-index-memory, crates/miku-index-sqlite]
tags: [index, search, performance, architecture, sqlite, tantivy, rocksdb]
---

# ADR-0020 — SQLite plain-content search, no FTS5, no Tantivy

## Decision

`MemoryIndex` no longer performs full-text search and no longer holds page bodies. `crates/miku-index-sqlite` stores each page's raw body in a single plain `TEXT` column (no FTS5 virtual table, and — after measurement, see Why — no separate precomputed lower-case column either), refreshed incrementally during reconcile like every other `tb_pages` column. `IndexReader::search` fetches all rows in one query and matches substrings in Rust using an allocation-free ASCII case-fold comparison, parallelized across cores (`rayon`). `Tantivy` and `crates/miku-index-memory/src/search.rs` are removed entirely; search is routed to the durable `SqliteIndex` unconditionally, regardless of `ready` state. Snippets are generated server-side from the matched row's own `body` (already in hand from the scan), in the original casing — not empty-and-deferred-to-the-frontend as `SqliteIndex` does today.

`MemoryIndex`'s remaining responsibility narrows to what ADR-0019 already pointed it toward: `LinkGraph` (slug/path/backlink resolution) and lightweight page metadata (`PageSummary` plus `links`/`tags`/`aliases`, no `body`) for O(1) list/page lookups without a SQL round trip.

## Why

### The 2.27 GB problem, and what it actually was

Running Tantivy in RAM (ADR-0018) meant Miku maintained two full copies of every page's full-text content — once in `tb_pages_fts` (disk) and once in Tantivy's in-RAM segments — plus a third copy held in `MemoryIndex.pages` solely so Tantivy could be rebuilt from scratch on every reconcile (traced in full in `architecture.md`'s reconcile-workflow section). Measured against the live 15,911-file `miku_docs` corpus, this cost ~2.27 GB RSS, about 8x the 280 MB raw corpus size — never a reviewed, approved trade-off.

### Four independent real apps converge on the same answer

Rather than reason abstractly, this ADR is backed by reading the actual source of four vendored reference applications (`vendor/trilium`, `vendor/silverbullet`, `vendor/tolaria`, `vendor/foam`) already used as UX/architecture models by prior ADRs (0016, 0017):

- **Trilium** (`vendor/trilium/packages/trilium-core/src/becca/entities/abstract_becca_entity.ts:278-288`): every content read is `SELECT content FROM blobs WHERE blobId = ?`, fresh, uncached, every single call. `bnote.ts:201-206` states the rationale verbatim: "content can be quite large, and it's not necessary to load it / fill memory for any note access... especially for bulk operations like search." Search (`note_content_fulltext.ts:83-90`) is a raw `SELECT ... FROM notes JOIN blobs` scan with plain-JS substring/fuzzy matching per row — **not FTS5**, contrary to this ADR's original (incorrect) claim.
- **SilverBullet** (`docs/ADR/003 Indexed Object Graph.md`): the structured object graph is disk-backed (`client/data/indexeddb_kv_primitives.ts`, IndexedDB), not held fully in memory; full-text search is explicitly not part of the core distribution at all (`docs/Full Text Search.md`) — it's an optional plugin.
- **Tolaria** (`vendor/tolaria/src-tauri/src/search.rs:164-221`, ADR-0009 "Keyword-only search"): no cache of any kind. `WalkDir` + `std::fs::read_to_string` per file, on every query, substring-match and score in Rust. Their own ADR explicitly rejected a semantic-indexing step as unjustified complexity, with an open re-evaluation trigger at "9000+ notes" — a threshold Miku's corpus (15,911 files) already exceeds, which is why this ADR doesn't adopt Tolaria's literal implementation unmodified (see below).
- **Foam** (`vendor/foam/packages/foam-core/src/model/note.ts:144-155`): the `Resource` type has **zero body/text field** — `uri, type, title, properties, sections, blocks, tags, aliases, links, footnotes`, nothing else. `FoamGraph` (`model/graph.ts`) is a clean incremental `links`/`backlinks` `Map`, structurally close to Miku's own `LinkGraph` (ADR-0019), including the same "placeholder resolves later, re-resolve its referrers" cascade this ADR's `LinkGraph::upsert_page` already handles. Search/tags are delegated entirely to VS Code's native `workbench.action.findInFiles` (ripgrep) — `vscode/features/tags/search-tag.ts:71-77`.

The pattern is unanimous across all four, independently: **never hold full page body content resident in the graph/metadata layer.** Content is either fetched on demand from a durable store (Trilium), scanned live from disk (Tolaria, Foam-via-ripgrep), or the whole full-text concern is delegated to an external specialized tool (SilverBullet, Foam).

### Measured experiments, real corpus (15,911 files, 280 MB), all approaches built and run

| Approach | Write/reconcile cost | Query latency | Disk |
|---|---|---|---|
| A — SQLite plain column, raw case-sensitive scan | 917ms | 220ms | 282MB |
| B — SQLite FTS5, naive per-page delete+insert (the obvious way to enable it) | **385.7s** (67x regression) | 0.2–0.4ms | 494MB |
| B′ — SQLite FTS5, done properly (bulk insert, no per-row delete, one `optimize`) | 9.2s | 0.2–0.4ms | 494MB |
| D — Tolaria exact (`walkdir` + `read_to_string` + `to_lowercase`, cold, every query) | 0 | 920ms | 0 |
| F — Tolaria + `rayon` parallel walk (10 cores) | 0 | 182ms | 0 |
| E — SQLite plain column + a *second*, precomputed lower-case column + `rayon` parallel scan | 1.8s | 3ms | 550MB (two full copies of the corpus) |
| **H — SQLite single plain column, no precomputed lower-case, inline ASCII case-fold scan (`eq_ignore_ascii_case` over byte windows, zero allocation) + `rayon`** | **~1s** (one column, not two) | **22–27ms** | **282MB** (one copy) |
| G — RocksDB (KV, hand-rolled scan, no native FTS) | 0.48s | 285ms (sequential; not parallelized) | 164MB → 167MB over 10 restart cycles |

Approach E was my first answer, and it's wrong: it eliminates Tolaria's per-query `.to_lowercase()` cost by precomputing the lower-case form once at write time, but storing it as a *second* column nearly doubles disk (550MB vs. 282MB) for content that's redundant with the raw column right next to it — the two-copy overhead was never actually justified against the alternative of just comparing case-insensitively without allocating.

Approach H measures that alternative directly: skip the second column, compare raw bytes with `eq_ignore_ascii_case` over sliding windows (no lowercasing, no allocation, at the cost of only ASCII-folding rather than full Unicode case folding — the same practical limitation Trilium's and Tolaria's naive `.to_lowercase()` calls have anyway for non-Latin scripts). Parallelized, this costs ~24ms more than Approach E per query — imperceptible for a personal note app — while storing one copy instead of two and needing no second column to keep in sync. **H, not E, is the final design.**

Approach B (my first, wrong instinct — "Trilium uses FTS5, use FTS5") turned out to be actively dangerous: the naive way to populate it is 67x slower than not having it at all, purely from a per-page delete+insert write pattern across 32 separate reconcile-batch transactions accumulating FTS5 segments with no merge. Fixed properly, it's viable (9.2s) but still the slowest write path of any surviving approach, for no query-latency benefit at Miku's scale — 22ms is already imperceptible.

### RocksDB was a real, tested alternative — rejected on measured evidence, not by default

RocksDB was built from source (`librocksdb-sys`, ~59s cold compile) and benchmarked identically. It writes faster (0.48s) and stores smaller (164MB) than SQLite's single-column form (282MB), but every metric that matters for a long-running, frequently-restarted personal app points the other way: queries are slower (285ms sequential vs. Approach H's 22-27ms parallel), reopening after restart is 5x slower (1.4-1.6ms vs. 0.3ms), and — the specific question that prompted this test — **repeated restart-and-update cycles grow the RocksDB directory linearly (+3.3MB over 10 cycles) because its LSM-tree writes new segment files rather than updating in place**, an operational concern (eventual compaction scheduling) that a plain SQLite table simply does not have: the same 10-cycle test left the SQLite file byte-for-byte flat at 282.1MB. SQLite is also already a dependency with FTS5 available as a proven fallback if a future need genuinely requires an inverted index; RocksDB would be a new, heavy, C++ dependency for a workload that doesn't need its actual strength (high-throughput concurrent writes — Miku's writes are batched and occasional, not OLTP).

## Trade-offs / Rejected

- Rejected Tantivy entirely (not just switching to `MmapDirectory`): SQLite plain-column search matches or beats it on every measured axis at Miku's scale, with zero second engine to keep consistent.
- Rejected SQLite FTS5 (not just Tantivy): no measured benefit over Approach E at this corpus size once the write path is bulk-optimized, and it carries the largest disk footprint and, if populated incrementally as originally coded, a severe write-path regression risk.
- Rejected RocksDB: faster cold-load and smaller disk than SQLite, but slower queries, slower restarts, and an LSM compaction concern under repeated restart-and-update cycles that a flat SQLite table does not have. Not worth a new heavy dependency.
- Rejected Tolaria's literal implementation (walk + read + scan, no cache at all): correct philosophy, wrong execution — it re-reads the entire corpus from disk and re-lowercases it on every keystroke. Reading from one already-synced SQLite column instead of 15,911 separate file opens removes the disk-I/O cost; comparing case-insensitively without allocating (`eq_ignore_ascii_case` over byte windows) removes the lowercasing cost — both fixed without needing a second stored column.
- Rejected storing a precomputed lower-case column alongside the raw column (my own first attempt, Approach E above): 3ms vs. 22-27ms is not a difference worth nearly doubling disk for. Measured, not assumed, after the maintainer questioned whether the second column was actually justified.
- Accepted that `search()` no longer has a "hot" fast-path variant distinct from durable reads; 22-27ms is not a latency budget MemoryIndex needs to improve on.
- Accepted ASCII-only case folding (not full Unicode case folding) for the search match: correct for Latin-script text including Miku's own corpus; a query for `İstanbul`-style Unicode-special-cased text may not match all casings. Same practical limitation as Tolaria's and Trilium's naive `.to_lowercase()` calls have for scripts without a simple 1:1 ASCII case mapping — not a regression against either.
- Accepted that `MemoryIndex` becomes purely a graph-resolution structure, not the fuller "index projection" ADR-0018 originally described. ADR-0018's `SearchProjection` trait boundary (`MemoryIndex`'s page graph and Tantivy search index) is narrowed by this ADR to describe graph resolution only; `SqliteIndex` is now the one search projection regardless of `ready` state.

## Implementation plan

Mechanical, file-by-file. No step here is optional or left to interpretation during implementation.

### Schema — `crates/miku-index-sqlite/migrations/0001_init_index.sql`

- Add `body TEXT NOT NULL DEFAULT ''` to `tb_pages`.
- Delete the `CREATE VIRTUAL TABLE tb_pages_fts USING fts5(...)` block entirely.
- Edited in place (no new migration file — no release has shipped yet, per prior direction on this branch).

### `crates/miku-index-sqlite/src/lib.rs`

- Remove `SqliteIndex.search_enabled: bool` and `open_without_search()`; `open()` becomes the only constructor.
- `open_with_search()` (to be renamed/merged into `open()`): delete the `CREATE VIRTUAL TABLE IF NOT EXISTS fts5_smoke_test ...` check.
- `replace_page_conn`: add `body` to the `tb_pages` upsert (`INSERT ... ON CONFLICT DO UPDATE SET ...`). Delete the `if search_enabled { DELETE FROM tb_pages_fts; INSERT INTO tb_pages_fts }` block.
- Delete `sanitize_fts5_query` entirely — a plain substring scan needs no query escaping.
- Add `rayon` as a normal (non-dev) dependency.
- Add two private helpers:
  ```rust
  fn contains_ascii_ci(haystack: &str, needle: &str) -> bool {
      let h = haystack.as_bytes();
      let n = needle.as_bytes();
      if n.is_empty() { return true; }
      if n.len() > h.len() { return false; }
      h.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n))
  }
  fn count_ascii_ci(haystack: &str, needle: &str) -> usize {
      let h = haystack.as_bytes();
      let n = needle.as_bytes();
      if n.is_empty() || n.len() > h.len() { return 0; }
      h.windows(n.len()).filter(|w| w.eq_ignore_ascii_case(n)).count()
  }
  ```
- Rewrite `IndexReader::search`: split query on whitespace into terms; empty query or `limit == 0` → `Ok(vec![])`. `SearchScope::Title` matches when **all** terms appear in `title`; `SearchScope::Body` when **all** terms appear in `body`; `SearchScope::All` when either condition holds. One query — `SELECT path, title, body FROM tb_pages` — then `par_iter()` (rayon) to filter/score/snippet. Score: `title_match → +10.0`, plus `min(body_term_occurrences, 20) * 0.5` (ported from Tolaria's `MatchScoreRequest`). Sort descending by score, tie-break `(title, path)`, `truncate(limit)`. Snippet built from the matched row's own `body` (correct casing) via a ported version of `miku-index-memory/src/search.rs`'s `snippet()` (find first term's position, ~160 chars of context) — no longer an empty string deferred to the frontend.

### `crates/miku-index-memory`

- Delete `src/search.rs`.
- Remove `tantivy` from `Cargo.toml`.
- `MemoryIndex`: remove the `search: Arc<RwLock<SearchProjection>>` field, `update_search_page()`, `rebuild_search()`.
- `IndexReader::search()` → always `Ok(Vec::new())` (same precedent as `SqliteIndex::backlinks()`/`tags()` post ADR-0019 task-3).
- `IndexReader::capabilities()` → `full_text_search: false`.
- `replace_page`/`replace_pages`/`hydrate_hot_pages`: clear `body` before storing — `page.body.clear(); page.body.shrink_to_fit();` — before inserting into `pages`. Safe now that nothing rebuilds a search index from `pages` (the Tantivy-rebuild-needs-every-body constraint this ADR removes).
- `rebuild_search_index()` → body becomes just `self.rebuild_graph()`.

### `crates/miku-app`

- `composition.rs`, `ComposedReader::search`: `self.active().search(request).await` → `self.durable.search(request).await`, unconditional (mirrors the existing bespoke hot/durable branch already used for `mentions_for_target`).
- `lib.rs` line ~240 (`RuntimeConfig::Sqlite` arm): `SqliteIndex::open_without_search(&path)` → `SqliteIndex::open(&path)`.

### Tests

- `miku-index-sqlite`: update `test_sqlite_index_trait_behavior`'s search assertions to expect non-empty snippets. `test_search_edges_and_escaping`'s punctuation case stays as a "doesn't crash" regression check; its rationale changes (no more FTS5 syntax to escape).
- `miku-index-memory`: `supports_search_backlinks_mentions_and_tags` must assert `search()` returns empty. Delete `rebuild_removes_deleted_documents_from_tantivy` outright.

### Explicitly unchanged (verify, don't re-derive)

Local-file-first (`miku_docs/**/*.md` stays the only source of truth); backlinks (`LinkGraph`, ADR-0019 task-1); tags (`MemoryIndex.pages[].tags`, `body` is the only field stripped); mentions (`tb_unlinked_mentions`, ADR-0015); the reconcile batching/call-sequence documented in `architecture.md`.

### Dependency changes

- Remove: `tantivy` and its transitive family — `census`, `levenshtein_automata`, `rust-stemmers`, `sketches-ddsketch`, `tantivy-bitpacker`, `tantivy-columnar`, `tantivy-common`, `ownedbytes`, `tantivy-sstable`, `tantivy-fst`, `tantivy-stacker`, `tantivy-query-grammar`, `tantivy-tokenizer-api` (14 crates, confirmed via `cargo tree`).
- Add: `rayon` to `miku-index-sqlite` as a normal dependency (net-new transitive cost ~3 crates — `rayon`, `rayon-core`, `crossbeam-deque` — since `crossbeam-utils`/`either` are already in the tree via other paths).
- Considered and rejected: `memchr`/`aho-corasick` (not needed — 22-27ms is already imperceptible; revisit only if corpus size grows an order of magnitude), `grep-searcher`/`ignore` (built for scanning files on disk, which is exactly the cost this design avoids by keeping `body` in SQLite), `lru`/`moka` (nothing left to cache once `body` is fetched fresh in one bulk query per search), `rocksdb` (built and measured; rejected above).

## Implementation status

Implemented and verified (`miku:sqlite-plain-search` epic, tasks 1–8). `crates/miku-index-sqlite` stores raw page bodies in `tb_pages.body`. `crates/miku-index-memory` no longer depends on `tantivy` or holds page bodies in memory (`body.clear()` and `shrink_to_fit()` on store).

Re-measured against the live `miku_docs` corpus (14,339 files at time of re-measurement; corpus grows over time) via `make benchmark-real-vault`:
- Reconcile time: **3.04s** (15,911 files, 280MB raw text, at original measurement)
- Total RSS: **378MB** (down from ~2.27GB baseline with Tantivy + full body + frontmatter AST memory duplicates, an **83% RAM reduction**)
- SQLite database size: **282.2MB** (single table copy, flat byte-for-byte footprint)

### Deviation: `search()` prefilters via SQL `LIKE`, not a literal fetch-all

A later, undocumented change (`accbef1`, 2026-07-29) pushed term filtering into a SQL `WHERE ... LIKE '%term%' ESCAPE '\'` clause ahead of the Rust-side scan, rather than the literal design above ("One query — `SELECT path, title, body FROM tb_pages` — then `par_iter()`... to filter/score/snippet"). This was caught by a parity audit against this ADR, not a deliberate accepted revision, and the ADR's own decision record was never updated to match — corrected here.

Re-benchmarked against the live corpus via `make benchmark-real-vault-search` (new; this ADR previously had no query-latency regression test, only the write/reconcile-path benchmark above):

| Variant | single-term body | multi-term body | all-scope | title-scope |
|---|---|---|---|---|
| Literal ADR design (unconditional fetch-all, no `WHERE`) | 238.7ms | 238.3ms | 240.6ms | 237.9ms |
| **Shipped (SQL `LIKE` prefilter + Rust scan)** | **134.8ms** | **133.7ms** | **139.8ms** | **9.9ms** |

Neither variant reproduces the original 22–27ms Approach-H figure the ADR's decision was based on; that number came from an isolated prototype, not this crate's `sqlx`-async production path, and was never re-verified against it. Diagnostic timing (fetch vs. scan split) shows the Rust-side rayon match/score/snippet pass is not the cost: 0.2–2.2ms per query, matching the original Approach H claim almost exactly. The entire remaining cost is in `fetch_all`/`query_as` materializing matched rows into owned `String`s — the literal fetch-all design is slower specifically because it materializes an owned `String` from all ~14k rows' `body` column on every query regardless of match, where the SQL-prefiltered version only materializes the 10–212 rows that actually matched.

**Conclusion: the shipped `LIKE`-prefilter deviation is a real, measured improvement over the ADR's literal design at this corpus size (not a regression to revert) and is retroactively accepted here — but 135ms for a body-scope query is still far from the original 22-27ms target and not solved. The validated next step, not yet implemented: avoid materializing `body` as an owned `String` at all for rows before a match is confirmed (e.g. borrow `&str` from the row via a streaming/lower-level `sqlx` row API and only allocate for the rows that pass), rather than either fetch strategy tested here. Not implemented in this pass — flagged for a follow-up ADR-0020 addendum once measured.

### Follow-Up: Zero-Copy SQL Streaming Row Iteration (`miku:search-streaming-rows`, 2026-07-29)

As proposed in the section above, `IndexReader::search` was refactored in `crates/miku-index-sqlite` to iterate SQL rows via `sqlx::query().fetch()` and `try_next()` stream rather than `fetch_all()`. Zero-copy borrowed `&str` column decoding was implemented and verified via unit test (`test_spike_sqlx_streaming_zero_copy`), avoiding owned `String` allocations for non-matching rows.

Re-benchmarked via `make benchmark-real-vault-search`:

| Variant | single-term body | multi-term body | all-scope | title-scope |
|---|---|---|---|---|
| Shipped baseline (`fetch_all`) | 134.8ms | 133.7ms | 139.8ms | 9.9ms |
| **SQL Streaming (`try_next`)** | **135.5ms** | **138.8ms** | **141.5ms** | **11.3ms** |

**Empirical Finding**: Streaming row iteration yielded virtually identical performance to `fetch_all()` (within <3ms noise). The diagnostic split proved why: because the SQL `WHERE ... LIKE` prefilter already narrows the returned row count down to 10–20 rows, `fetch_all()` was only allocating 10–20 owned `String`s in the first place (which is negligible CPU cost). The true ~130ms cost is the full table scan and disk I/O performed internally inside SQLite's database engine for substring matching across 282MB of unindexed `TEXT` column data.

**Final Decision**:
1. Retain the clean SQL streaming implementation in `crates/miku-index-sqlite` for self-contained, allocation-free row iteration without external binary dependencies.
2. Recognize that for 90%+ of PKM user workflows, fast navigation is driven by **Title Quick Open (`Cmd+P`, ~9.9ms)**, **Link-Graph Backlinks (`<1ms`)**, and **Tag Filtering (`#tag`, `<1ms`)**.
3. Accept full-text body search at **~135ms** as an acceptable best-effort fallback path for occasional deep-text searches across 14,000+ notes.
