---
id: ADR-0016
title: SQLite (sqlx) local index
slug: sqlite-local-index
status: Accepted
date-proposed: 2026-07-15
date-accepted: 2026-07-15
deciders: [haru]
mirror: asobi:miku:decision:sqlite-local-index
supersedes: [ADR-0011, ADR-0014]
superseded-by:
relates-to: [ADR-0009, ADR-0010, ADR-0012, ADR-0013]
rejects: [rusqlite-sync, libsql-embedded, redb-kv, retained-legacy-backend]
impacts: [crates/miku-index-sqlite, crates/miku-app, Cargo.toml, Makefile, miku_docs/setup.md, miku_docs/architecture.md]
config-keys: [MIKU_INDEX_BACKEND, MIKU_INDEX_PATH]
tags: [index, sqlite, sqlx, fts5, dependencies]
---

# ADR-0016 — SQLite (sqlx) local index

## Decision

The local durable index is **SQLite via `sqlx`**, sharing the SQL stack with the existing Postgres backend. Full-text search uses SQLite's built-in **FTS5**. `memory` + `sqlite` are the default
features; `postgres`/`valkey` remain opt-in. The disposable-index invariant is unchanged: the index is rebuilt from `miku_docs/**/*.md`, so there is no data migration. Any old local index file is
disposable and may be removed before the next startup.

## Why

The previous local backend pulled a disproportionate dependency closure, including a second full-text implementation and unrelated synchronization/encryption support. Repository-owned checks could not
gate that weight without maintaining a fork.

Any **C-SQLite-based** backend collapses this: FTS5 ships inside SQLite's C amalgamation (no `tantivy`), and the Rust footprint is small. `sqlx` + `sqlite` also **reuses the SQL driver already pulled
for Postgres**, unifying both tiers on one async SQL layer (ADR-0009 composition, ADR-0010 boundaries). Expected default-build delta: **~305 → ~100 normal crates**.

This also simplifies the crates.io release surface (ADR-0013): one fewer backend crate to publish, and the default `cargo install miku` gets a lean, durable, FTS-capable index with no
C-reimplementation baggage.

The resolved dependency closure was measured at the migration boundary. The root package's normal dependency tree fell from 403 unique packages before this ADR to 254 after it, a reduction of 149
packages (36.9%). The lockfile fell from 482 package records to 312, a reduction of 170 records (35.3%). These counts include the complete normal workspace resolution, including transitive
dependencies.

## Design

### New crate `crates/miku-index-sqlite`

Mirrors `crates/miku-index-postgres`. Implements `IndexReader`/`IndexWriter` from `miku-domain` over an `sqlx::SqlitePool`.

```toml
# Cargo.toml deps
async-trait.workspace = true
miku-domain  = { version = "0.0.2", path = "../miku-domain" }
miku-indexer = { version = "0.0.2", path = "../miku-indexer" }
serde_json.workspace = true
sqlx = { version = "0.8.6", default-features = false, features = ["sqlite", "runtime-tokio", "migrate", "macros"] }
```

- **SQLite-only runtime features** (SQLite needs no TLS → leaner default). The `macros` feature is used only by `sqlx::migrate!`; all application queries use runtime `query`/`query_as` calls, keeping
  compile-time database access out of the application path.
- `pub async fn open(path: &str) -> StoreResult<Self>`:
  ```rust
  let opts = SqliteConnectOptions::from_str(&format!("sqlite://{path}"))?
      .create_if_missing(true)
      .journal_mode(WAL)          // concurrent readers, single writer
      .foreign_keys(true)
      .busy_timeout(Duration::from_secs(5));
  let pool = SqlitePoolOptions::new().max_connections(4).connect_with(opts).await?;
  sqlx::migrate!("./migrations").run(&pool).await?;
  ```

### Schema `crates/miku-index-sqlite/migrations/0001_init_index.sql`

Same tables as Postgres (`tb_pages`, `tb_links`, `tb_tags`, `tb_page_aliases`, `tb_unlinked_mentions`, `tb_index_meta`), SQLite dialect:

- `id INTEGER PRIMARY KEY AUTOINCREMENT`; `frontmatter TEXT` (JSON as text); `has_mermaid INTEGER`; `mtime INTEGER`. Drop `body_tsv`, `pg_trgm`, and the trigram indexes.
- FTS5 replaces the tsvector:
  ```sql
  CREATE VIRTUAL TABLE tb_pages_fts USING fts5(
    path UNINDEXED, title, body, tokenize = 'porter unicode61'
  );
  ```

### SQL translation (Postgres → SQLite)

| Concern            | Postgres                                           | SQLite                                                                                                    |
| ------------------ | -------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| Placeholders       | `$1`                                               | `?`                                                                                                       |
| Autoincrement id   | `GENERATED ALWAYS AS IDENTITY`                     | `INTEGER PRIMARY KEY AUTOINCREMENT`                                                                       |
| `RETURNING id`     | yes                                                | yes (SQLite ≥ 3.35; bundled is newer)                                                                     |
| JSON column        | `JSONB`                                            | `TEXT`; bind `frontmatter.to_string()`, read `String` then `serde_json::from_str`                         |
| Body FTS           | `to_tsvector` / `websearch_to_tsquery` / `ts_rank` | write `tb_pages_fts` on replace/delete; `WHERE tb_pages_fts MATCH ? ORDER BY bm25(tb_pages_fts,10.0,1.0)` |
| Title/path search  | `ILIKE ... ESCAPE`                                 | `LIKE ? ESCAPE '\'` (SQLite `LIKE` is ASCII case-insensitive)                                             |
| `= ANY($1)`        | array bind                                         | dynamic `IN (?,?,…)` or loop                                                                              |
| `COUNT(*)::BIGINT` | cast                                               | plain `COUNT(*)` → `i64`                                                                                  |
| snippet            | empty (`SearchHit.snippet=""`)                     | keep **empty** — the search page builds snippets from disk (unchanged)                                    |

- `replace_page`: after upserting `tb_pages`, keep FTS in sync — `DELETE FROM tb_pages_fts WHERE path=?; INSERT INTO tb_pages_fts(path,title,body) VALUES(?,?,?)`. The links/tags/aliases delete+insert
  and the two `UPDATE tb_links` resolve passes translate 1:1.
- `delete_page`: also `DELETE FROM tb_pages_fts WHERE path=?`.
- `rebuild_search_index`: no-op (FTS rows written per page) — keep the trait default.
- Consider overriding `replace_pages` to wrap the batch in one transaction.

### Wiring (`crates/miku-app/src/lib.rs`)

- `RuntimeConfig::Sqlite { path }`; extend `runtime_name`/`runtime_feature`/ `runtime_enabled`.
- `resolve_runtime`: `"sqlite"` arm reading `MIKU_INDEX_PATH` (default `miku_docs/.miku-index.sqlite`); make `sqlite` the default backend.
- `compose_index`: `#[cfg(feature="sqlite")]` → `IndexApi::from_store(Arc::new(SqliteIndex::open(&path).await?))`.
- `Cargo.toml`: optional `miku-index-sqlite` dep + `sqlite = ["dep:miku-index-sqlite"]`; `default = ["memory", "sqlite"]`.

### Defaults across the tree

- Root `Cargo.toml`: `default = ["memory", "sqlite"]`; add a `sqlite` feature forwarding `miku-app/sqlite`.
- The workspace contains only the supported SQLite, Postgres, memory, and Valkey backend crates.
- `Makefile`: `MIKU_INDEX_BACKEND ?= sqlite`, `MIKU_INDEX_PATH ?= miku_docs/.miku-index.sqlite`; `inspect-index` targets the SQLite example.
- `.gitignore`: ignore `*.sqlite*` (WAL/SHM sidecars) under `miku_docs/`.
- Docs: README config table, `miku_docs/setup.md`, `miku_docs/architecture.md`.

### Capabilities

`durable: true`, `full_text_search: true` (FTS5), `transactions: true`, `fuzzy_page_search: false` (no `pg_trgm`; quickswitch fuzzy already runs in Rust in `main.rs`), `remote_sync: false`.

## Trade-offs / Rejected

- **rusqlite (sync, bundled+fts5)** — leanest (~15–25 crates) but synchronous; would need `spawn_blocking` wrappers and a second SQL idiom alongside sqlx. Rejected for consistency: `sqlx` already
  lives in the tree for Postgres, so unifying on it costs a few more crates but one async SQL layer.
- **libsql (embedded)** — async-native, but heavier than sqlx-sqlite and pulls sync/replication machinery unless carefully gated. Rejected; sqlx-sqlite is simpler and shared.
- **redb / sled (pure-Rust KV)** — tiny (~1–10 crates) but no SQL and no FTS; would force hand-building backlinks/tags/search in app code. Rejected.
- **Retain the previous backend as an opt-in feature** — rejected: `cargo test --workspace` and `make check` build every member crate. Keeping a retired backend in the workspace would preserve its
  dependency closure and make the default release surface ambiguous.

## Gotchas (verify early)

1. **FTS5 must be compiled into sqlx's SQLite build.** Verify first: run `CREATE VIRTUAL TABLE t USING fts5(x)` at startup. If it errors, enable FTS5 via `libsqlite3-sys` (bundled build flag). This is
   the #1 risk — check before writing the full impl.
2. **FTS5 `MATCH` query escaping.** Raw user input (`-` `"` `*` `:`) can break MATCH syntax. Sanitize: split on whitespace, wrap each term in double quotes, optionally append `*` for prefix
   (`"foo" "bar"*`). Add a punctuation test.
3. **`SQLITE_BUSY`** under concurrent reconcile + reads — WAL + `busy_timeout` handle it; keep the single-writer invariant (indexer is the only writer).
4. **`bool` binding** maps to INTEGER 0/1 in sqlx-sqlite — read `has_mermaid` back consistently.

## Verification checklist

- `cargo test -p miku-index-sqlite` against a `tempfile` DB: page round-trip, backlinks, tags, FTS body search, mentions.
- `MIKU_INDEX_BACKEND=sqlite make run`: a page indexes, `/search` returns FTS hits, restart persists (durable).
- `cargo tree -p miku --edges normal --prefix none | sort -u | wc -l` → current root normal dependency closure: 254 packages, down from 403 before this migration.
- `rg '^name = ' Cargo.lock | wc -l` → current lockfile resolution: 312 package records, down from 482 before this migration.
- `make check` and `make check-all-features` green.
- Ships under `0.0.2`; separate PR from the installable/publishable work.
