# Dataflow & Workflows

All diagrams are Mermaid. See `docs/architecture.md` for the prose design and schema. Watcher scaling (folder-scoped watching and fallbacks) is covered in §8.

## 1. System overview

Files are the source of truth; SQLite is the default disposable index, with
Postgres available as an explicit profile. HTTP handlers only **read** the
index; the background indexer is the **only** writer.

```mermaid
flowchart LR
  Browser["Browser<br/>(rendered HTML + textarea)"]

  subgraph Server["Miku — Rust single binary"]
    HTTP["axum HTTP layer<br/>(read-only on index)"]
    Store["Store<br/>(atomic file I/O)"]
    Indexer["Background indexer<br/>(sole index writer)"]
  end

  FS[("miku_docs/ Markdown<br/>source of truth")]
  PG[("SQLite / Postgres<br/>disposable index")]

  Browser -->|"GET view / edit"| HTTP
  Browser -->|"POST save"| HTTP
  HTTP -->|"read page text"| Store
  HTTP -->|"queries:<br/>backlinks / tags / FTS"| PG
  HTTP -->|"atomic save:<br/>write temp + rename"| Store
  Store --> FS
  FS -->|"fs events (notify)"| Indexer
  Indexer -->|"reindex tx"| PG
```

## 2. Rendering model — view vs edit (v0)

The readonly rendered view is the **primary** mode; editing is opt-in. Classic wiki model, no client JS.

```mermaid
flowchart TD
  V["GET /page/Foo"] --> X{"Foo.md exists?"}
  X -- yes --> R["read Foo.md -> render md->HTML"] --> RO["readonly view"]
  X -- no --> NEW["offer: create Foo?"]

  RO -->|"click Edit"| E["GET /page/Foo/edit"]
  E --> T["read Foo.md -> textarea"]
  T -->|"Save"| P["POST /page/Foo"]
  P --> S["atomic save"]
  S --> RD["303 redirect to /page/Foo (view)"]
  RD --> V
```

## 3. Save → index contract (single-writer, no race)

The save handler writes the file and returns. It **never** touches the index. The `notify` watcher is the sole index trigger, so there is no double-index and no save↔index race.

```mermaid
sequenceDiagram
  participant B as Browser
  participant H as axum handler
  participant FS as miku_docs/*.md
  participant W as notify watcher
  participant I as Indexer
  participant PG as Postgres

  B->>H: POST /page/Foo (markdown body)
  H->>FS: write Foo.md.tmp + fsync (miku_docs/)
  H->>FS: rename to Foo.md (atomic, miku_docs/)
  H-->>B: 303 redirect to /page/Foo (view)
  Note over H,PG: handler does NOT touch the index
  FS-->>W: modify event (Foo.md)
  W->>W: debounce ~200ms
  W->>I: reindex(Foo.md)
  I->>PG: reindex transaction
  Note over W,I: notify is the SOLE trigger -> no race
```

## 4. Reindex-one-page transaction

One page reindex is a single Postgres transaction.

```mermaid
flowchart TD
  S["reindex(miku_docs/ path)"] --> P["parse page:<br/>title, [[links]], #tags, body"]
  P --> BEGIN["BEGIN"]
  BEGIN --> U["upsert pages row -> id<br/>set body_tsv, mtime"]
  U --> DL["delete links where src_id=id<br/>insert fresh edges"]
  DL --> DT["delete tags where page_id=id<br/>insert fresh tags"]
  DT --> RES["resolve targets -> target_id<br/>(unique basename, shortest path)"]
  RES --> DAN["re-resolve dangling links<br/>now pointing at this page"]
  DAN --> COMMIT["COMMIT"]
```

## 5. Startup reconcile

`notify` can miss events while the process is down, so startup does a full mtime-based reconcile before the live watcher takes over.

```mermaid
flowchart TD
  A["startup"] --> B["scan miku_docs/**/*.md"]
  B --> C{"file mtime > pages.mtime?"}
  C -- "new / changed" --> D["reindex(file)"]
  C -- unchanged --> E["skip"]
  A --> F["pages row with no file on disk<br/>-> delete (cascade)"]
  D --> G["live watcher takes over"]
  E --> G
  F --> G
```

## 6. Link lifecycle (dangling ↔ resolved)

A `[[link]]` may point at a page that does not exist yet. Backlinks appear the moment the target is created; they go dangling again if it is deleted.

```mermaid
stateDiagram-v2
  [*] --> Dangling: "[[Bar]] written, Bar.md absent"
  Dangling --> Resolved: "Bar.md created -> indexer sets target_id"
  Resolved --> Dangling: "Bar.md deleted -> ON DELETE SET NULL"
  Resolved --> [*]: "source link removed"
  Dangling --> [*]: "source link removed"
```

## 7. Read-path queries (no filesystem touch)

Backlinks, tags, and search read **only** Postgres — never the filesystem — and are paginated so the full edge set is never loaded at once.

```mermaid
flowchart LR
  subgraph Read["read-only endpoints"]
    BL["GET /page/Foo/backlinks"]
    TG["GET /tags/:tag"]
    SR["GET /search?q="]
  end
  BL -->|"links.target_id = Foo.id<br/>LIMIT/OFFSET"| PG[("Postgres")]
  TG -->|"tags.tag = :tag"| PG
  SR -->|"body_tsv @@ query (GIN)"| PG
```

## 8. Watcher scale — folder-scoped watching

`notify` subscribes at **directory** granularity, so the watch budget scales with directory count, not file count. On Linux an inotify watch is added **per directory** and reports events for every
file directly inside it; `RecursiveMode::Recursive` adds one watch per subdirectory (auto-adding one when a new subdir appears). macOS FSEvents watches paths, with no per-file limit.

- 100k files across ~200 folders → ~200 watches (default `fs.inotify.max_user_watches` is 65k–524k — not close).
- 100k files in one folder → 1 watch.

This is why Miku needs no second store to "scale the watcher": an earlier plan (a rejected RocksDB work-queue detour) misdiagnosed the inotify limit as per-file. Three levers, in order of preference:

1. **Watch folders (default)** — already how recursive mode behaves; covers any realistic wiki.
2. **Raise the sysctl** — document `fs.inotify.max_user_watches` for the rare deep-tree case.
3. **`PollWatcher` fallback** — zero inotify watches, trading latency for budget; only past a genuinely extreme directory count.

```mermaid
flowchart TD
  A["startup"] --> B{"directory count > threshold?"}
  B -- no --> C["recursive inotify watch<br/>(1 watch per directory)"]
  B -- yes --> D["PollWatcher fallback<br/>(periodic mtime scan, 0 watches)"]
  C --> E["live external-edit pickup"]
  D --> E
  C -. "new subdir created" .-> F["crate auto-adds a watch;<br/>files raced before it lands<br/>are caught by startup reconcile (§5)"]
```

A file created in a brand-new directory before its watch registers can be missed; recursive mode auto-registers the new dir and the startup reconcile (§5) sweeps anything missed, so it self-heals.
