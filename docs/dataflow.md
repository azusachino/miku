# Dataflow & Workflows

All diagrams are Mermaid. See `docs/architecture.md` for the prose design and
schema.

## 1. System overview

Files are the source of truth; Postgres is a disposable index. HTTP handlers
only **read** Postgres; the background indexer is the **only** writer.

```mermaid
flowchart LR
  Browser["Browser<br/>(rendered HTML + textarea)"]

  subgraph Server["Miku — Rust single binary"]
    HTTP["axum HTTP layer<br/>(read-only on Postgres)"]
    Store["Store<br/>(atomic file I/O)"]
    Indexer["Background indexer<br/>(sole Postgres writer)"]
  end

  FS[("miku_docs/ Markdown<br/>source of truth")]
  PG[("Postgres<br/>disposable index")]

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

The readonly rendered view is the **primary** mode; editing is opt-in. Classic
wiki model, no client JS.

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

The save handler writes the file and returns. It **never** touches the index.
The `notify` watcher is the sole index trigger, so there is no double-index and
no save↔index race.

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

`notify` can miss events while the process is down, so startup does a full
mtime-based reconcile before the live watcher takes over.

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

A `[[link]]` may point at a page that does not exist yet. Backlinks appear the
moment the target is created; they go dangling again if it is deleted.

```mermaid
stateDiagram-v2
  [*] --> Dangling: "[[Bar]] written, Bar.md absent"
  Dangling --> Resolved: "Bar.md created -> indexer sets target_id"
  Resolved --> Dangling: "Bar.md deleted -> ON DELETE SET NULL"
  Resolved --> [*]: "source link removed"
  Dangling --> [*]: "source link removed"
```

## 7. Read-path queries (no filesystem touch)

Backlinks, tags, and search read **only** Postgres — never the filesystem — and
are paginated so the full edge set is never loaded at once.

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
