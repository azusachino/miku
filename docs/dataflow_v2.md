# Dataflow & Workflows (v2 — RocksDB queue for 100k-file scale)

This revises the v1 dataflow (`docs/dataflow.md`) to remove Miku's dependency on a recursive `notify`/inotify watch, which does not survive the 100k-file scale target (`architecture.md` → _Vault
layout & scale_). It introduces a **RocksDB durable work-queue + read cache** while keeping the core invariant intact.

## Why this changes (the notify limit)

v1 uses the `notify` watcher as the **sole** index trigger — even for the app's own saves: a `POST` writes the file, and the resulting `rename` event is what tells the indexer to reindex. At 10k–100k
files a recursive inotify watch exceeds `fs.inotify.max_user_watches`, so the _trigger mechanism itself_ is what breaks at scale, not Postgres.

The fix is to stop **watching** for app-originated changes and instead have the save handler **enqueue** its own index work directly. RocksDB backs that queue durably, so:

- App saves never depend on a filesystem watch → the inotify budget is a non-issue for normal editing.
- The queue survives a crash, so startup **drains the queue** instead of re-walking 100k files (a full mtime rescan at that scale is exactly the cost we want to avoid).
- External edits (git pull, another editor) are caught by a **periodic reconcile** scan rather than a live per-file watch.

**Core invariant preserved.** Markdown files under `miku_docs/` remain the source of truth — saves still atomic-write the file (temp + `fsync` + `rename`) _before_ enqueuing. **RocksDB and Postgres
are both disposable caches**, fully rebuildable from `miku_docs/**/*.md`; deleting either loses nothing but rebuild time. RocksDB holds two column families: a durable `queue` CF (pending index work)
and an optional `body` read-cache CF (`slug → body + content-hash + mtime`) that speeds reads and cheap change-detection.

---

## 1. System overview (v2)

```mermaid
flowchart LR
  Browser["Browser<br/>(rendered HTML + textarea)"]

  subgraph Server["Miku — Rust single binary"]
    HTTP["axum HTTP layer<br/>(read-only on Postgres)"]
    Store["Store<br/>(atomic file I/O)"]
    Rocks["RocksDB<br/>(durable dirty queue<br/>+ body read cache)"]
    Indexer["Background indexer<br/>(drains queue → Postgres)"]
    Reconcile["Reconcile task<br/>(periodic / manual)"]
  end

  FS[("miku_docs/ Markdown<br/>source of truth")]
  PG[("Postgres<br/>disposable index")]

  Browser -->|"GET view / edit"| HTTP
  Browser -->|"POST save"| HTTP
  HTTP -->|"atomic write temp + rename"| Store
  Store --> FS
  HTTP -->|"mirror body + enqueue dirty key"| Rocks
  HTTP -->|"read body (cache hit)"| Rocks
  HTTP -->|"queries:<br/>backlinks / tags / FTS"| PG
  Rocks -.->|"pop dirty keys"| Indexer
  Indexer -->|"reindex tx"| PG
  FS -.->|"mtime + hash scan"| Reconcile
  Reconcile -.->|"external edits → enqueue"| Rocks
```

---

## 2. Rendering model — view vs edit

Reads serve from the RocksDB body cache, falling back to the file on a miss (then back-filling the cache). Editing is opt-in, no client JS — unchanged from v1 in spirit.

```mermaid
flowchart TD
  V["GET /page/Foo"] --> X{"Foo in RocksDB cache?"}
  X -- hit --> R["render cached body -> HTML"] --> RO["readonly view"]
  X -- miss --> F{"Foo.md exists?"}
  F -- yes --> RB["read file -> back-fill cache -> render"] --> RO
  F -- no --> NEW["offer: create Foo?"]

  RO -->|"click Edit"| E["GET /page/Foo/edit"]
  E --> T["read body -> textarea<br/>(embed content-hash, per ADR-3)"]
  T -->|"Save"| P["POST /page/Foo"]
  P --> S["atomic save + enqueue (see §3)"]
  S --> RD["303 redirect to /page/Foo (view)"]
  RD --> V
```

---

## 3. Save → enqueue → index pipeline

The handler writes the file (truth), mirrors the body into RocksDB, and enqueues a dirty key — then returns. It **never** indexes. The indexer is woken by an in-process signal (no polling interval)
and drains the durable queue; the queue is also drained on startup, so nothing is lost across a crash.

```mermaid
sequenceDiagram
  participant B as Browser
  participant H as axum handler
  participant FS as miku_docs/*.md
  participant R as RocksDB (queue + cache)
  participant I as Indexer
  participant PG as Postgres

  B->>H: POST /page/Foo (markdown body + prior hash)
  Note over H: re-hash file, mismatch results in 409 (ADR-3 optimistic concurrency)
  H->>FS: write Foo.md.tmp + fsync then rename (atomic, truth)
  H->>R: mirror body + hash and enqueue dirty key (WAL fsync)
  H->>I: signal "work available" (in-process)
  H-->>B: 303 redirect to /page/Foo (view)
  Note over H,PG: handler enqueues and does NOT touch the index

  I->>R: pop next dirty key
  R-->>I: Foo (body + hash)
  I->>I: parse title, [[links]], #tags, body_tsv
  I->>PG: reindex transaction (§4 of v1)
  I->>R: mark key done (remove from queue CF)
  Note over I,R: the queue is the trigger — no inotify watch
```

Crash safety: file write commits the truth; if the process dies before the enqueue, the periodic reconcile (§5) re-detects the change by content-hash. If it dies after enqueue but before indexing, the
durable queue replays on startup.

---

## 4. FTS search & snippets

Unchanged from v1 and ADR-1: search reads **only** Postgres, snippets via `ts_headline('english', …)`. The RocksDB body cache is not on the search path — keeping ADR-1 intact and search a single
round-trip.

```mermaid
flowchart LR
  SR["GET /search?q="] -->|"body_tsv @@ query (GIN)<br/>ts_headline snippet"| PG[("Postgres")]
  BL["GET /page/Foo/backlinks"] -->|"links.target_id = Foo.id<br/>LIMIT/OFFSET"| PG
  TG["GET /tags/:tag"] -->|"tags.tag = :tag"| PG
```

---

## 5. External-edit reconciliation (replaces the live watch)

External writes (git pull, another editor) have no `notify` event in v2. A reconcile task — periodic, manual, and run once at startup — walks the tree and enqueues anything whose content changed.
mtime is a cheap pre-filter only; **content-hash confirms** the change (ADR-3: mtime lies across git/rsync).

```mermaid
flowchart TD
  A["reconcile (startup / periodic / manual)"] --> B["scan miku_docs/**/*.md"]
  B --> C{"mtime changed?"}
  C -- no --> E["skip"]
  C -- yes --> D{"content-hash ≠ cached hash?"}
  D -- no --> E
  D -- yes --> U["update RocksDB cache + hash<br/>enqueue dirty key"]
  A --> M["page key with no file on disk"]
  M --> X["enqueue delete<br/>(Postgres cascade; inbound links -> dangling)"]
  U --> H["indexer drains queue (§3)"]
  X --> H
```

Trade-off vs v1: external edits are picked up at reconcile cadence, not instantly. For app-driven editing this is invisible. Deployments that need live external-edit pickup _and_ stay under the watch
budget can add a single **directory-level** watch on the content root (watch dirs, not files) to trigger a scoped reconcile — optional, off by default.
