# Architecture

Miku is a filesystem-owned personal Markdown wiki: a browser editor over plain `.md` files, with server-side background indexing for backlinks, tags, and full-text search.

## Core invariant

Markdown files and assets under `miku_docs/` are the **source of truth**. Postgres holds only a **disposable index** that is fully rebuildable from `miku_docs/**/*.md`. Deleting the database loses
nothing but rebuild time.

```
repo/
  miku_docs/             # content root (source of truth)
    *.md            # pages
    assets/         # images (roadmap: drag/drop upload)
  src/              # Rust server
  migrations/       # sqlx Postgres migrations (index schema)
  static/           # server-rendered frontend assets
```

## Components

- **HTTP layer (axum):** page render/edit/save routes, search, tags, backlinks, static asset serving. **Read-only** against Postgres.
- **Store:** filesystem read/write of `miku_docs/*.md`. Atomic save = write temp + `fsync` + `rename`.
- **Background indexer:** `notify` watcher on `miku_docs/`; parses changed pages off the request path into Postgres. The **sole writer**.
- **Postgres index:** `pages`, `links`, `tags`, and a `tsvector` FTS column.

## Save / index contract (single-writer model)

This is the key design decision that removes save↔index races:

1. `POST /save` writes to a temp file → `fsync` → atomic `rename` into `miku_docs/<path>.md`. The handler returns immediately and **does not touch the index**.
2. The `rename` fires a `notify` event → debounced (~200ms) → the indexer reindexes just that page.
3. **`notify` is the only index trigger.** Handlers never index directly, so there is no double-indexing and no race. Cost: backlinks lag a save by ~200ms — invisible for a personal wiki.
4. **Startup reconcile:** full scan comparing file mtime vs `pages.mtime` to catch anything `notify` missed while the process was down.

Reindex-one-page is a single transaction: upsert the `pages` row, wipe+rewrite that page's `links` / `tags` / FTS rows, then re-resolve any dangling links now pointing at this page.

Postgres runs with the standard single-writer pattern here: only the indexer writes, HTTP handlers only read.

Tables are prefixed `tb_`. The authoritative DDL is `migrations/0001_init_index.sql`; this is the overview.

```sql
CREATE TABLE tb_pages (
  id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  path        TEXT NOT NULL UNIQUE,           -- relative to miku_docs/, e.g. 'sub/Bar.md'
  slug        TEXT NOT NULL,                  -- normalized basename for [[ ]] resolution
  title       TEXT NOT NULL,                  -- frontmatter title, else first H1, else filename
  frontmatter JSONB NOT NULL DEFAULT '{}',    -- opaque user properties
  has_mermaid BOOLEAN NOT NULL DEFAULT false, -- index-driven lazy mermaid.js injection
  mtime       BIGINT NOT NULL,                -- file mtime (unix), for reconcile
  body_tsv    TSVECTOR,                       -- full-text search vector
  indexed_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- directed link/embed edges; target_id points at tb_pages only
CREATE TABLE tb_links (
  src_id      BIGINT NOT NULL REFERENCES tb_pages(id) ON DELETE CASCADE,
  kind        TEXT   NOT NULL CHECK (kind IN ('page', 'asset')),
  is_embed    BOOLEAN NOT NULL DEFAULT false, -- true for ![[target]]
  target      TEXT   NOT NULL,                -- link name as written
  target_norm TEXT   NOT NULL,                -- normalized resolver key
  target_id   BIGINT REFERENCES tb_pages(id) ON DELETE SET NULL,
  alias       TEXT,                           -- display text from [[target|alias]]
  PRIMARY KEY (src_id, kind, target_norm, is_embed)
);

CREATE TABLE tb_tags (
  page_id BIGINT NOT NULL REFERENCES tb_pages(id) ON DELETE CASCADE,
  tag     TEXT   NOT NULL,
  PRIMARY KEY (page_id, tag)
);

-- page-declared aliases (frontmatter `aliases:`) for [[Alias]] resolution
CREATE TABLE tb_page_aliases (
  page_id BIGINT NOT NULL REFERENCES tb_pages(id) ON DELETE CASCADE,
  alias   TEXT   NOT NULL,
  PRIMARY KEY (page_id, alias)
);
```

Indexes: GIN on `body_tsv` (FTS) and `frontmatter` (future property queries); btree on `slug` for exact `[[ ]]` resolution; `pg_trgm` GIN on `slug`/`title` for fuzzy `[[ ]]` autocomplete and the
Ctrl-K palette; btree on `tb_links(target_id)` (backlinks), `tb_links(target_norm)` (dangling re-resolve and asset reports), `tb_tags(tag)`, `tb_tags(tag text_pattern_ops)` (hierarchical tag prefix
views), and `tb_page_aliases(alias)`.

### Key queries

Backlinks (paginated — never loads the full edge set):

```sql
SELECT p.path, p.title
FROM tb_links l JOIN tb_pages p ON p.id = l.src_id
WHERE l.target_id = $1
ORDER BY p.title LIMIT $2 OFFSET $3;
```

Dangling → resolved: when a page is created that something already `[[linked]]` to, the indexer runs
`UPDATE tb_links SET target_id = $1 WHERE kind = 'page' AND target_norm = $2 AND target_id IS NULL`, and the backlink appears. `ON DELETE SET NULL` turns inbound links dangling again when a page is
deleted.

## Rendering model (view / edit)

The readonly rendered view is the **primary** mode; editing is opt-in (classic wiki model, no client JS):

- `GET /page/Foo` → render `Foo.md` → HTML (readonly view, default)
- `GET /page/Foo/edit` → `<textarea>` with raw Markdown
- `POST /page/Foo` → atomic save → 303 redirect back to the readonly view

A `MIKU_READONLY` flag (roadmap) gates the edit/save routes for publishing. Rich rendering beyond CommonMark is server-side via comrak, which supports wikilinks and Obsidian/GitHub `> [!note]`
callouts natively; `![[transclusion]]` is our own extractor — never MDX/JSX (see `docs/decisions.md` ADR-2 / asobi `miku:decision:no-mdx`). The `:::` directive syntax was considered and dropped in
favor of `[!type]` for compatibility.

See `docs/dataflow.md` for the full set of workflow / dataflow Mermaid diagrams.

## Link resolution

`[[Name]]` resolves by unique basename across `miku_docs/`; if multiple match, pick deterministically (shortest path). `[[sub/Bar]]` matches an exact relative path. Obsidian `[[ ]]` compatible — this
replaces a separate importer.

Slug normalization (decided once, lives in the indexer): filename → `slug` via Unicode **NFC + case-insensitive** matching, so `ミク`, `Foo Bar.md`, and `[[foo bar]]` resolve consistently.

## Vault layout & scale

Organization comes from **links + tags + frontmatter, not a deep directory tree**. `miku_docs/` is the content root; `miku_docs/assets/` holds binaries; shallow topic folders (`people/`, `projects/`)
are allowed as a loose filing cabinet but never the primary structure — the graph carries the meaning.

**No first-class "category" concept** — it would reinvent Notion databases (postponed). The need is covered by three existing primitives: folders (loose physical filing), tags incl. hierarchical
`#area/sub` (flexible many-to-many), and frontmatter `type:`/`category:` (structured, GIN-indexed in `frontmatter`). A `/category/:name` view can be added later with zero schema change.

**Scale (10k–100k files).** Postgres handles 100k rows trivially; the pressure is in the indexer, not the DB:

- **Linux file-watcher limit** — recursive `notify`/inotify on 100k files can exceed `fs.inotify.max_user_watches`. Raise the sysctl (document in setup), watch directories over files, and fall back to
  coarser/polling watch above a threshold. macOS FSEvents is unaffected (watches paths, not inodes).
- **Bulk index ≠ single-page** — startup/import uses a batched path (multi-row insert / `COPY`, commit every N-thousand; build GIN after load), distinct from the live single-page reindex transaction.
- **Large single directory (the 100k-in-one-folder question), resolved as policy — Miku never auto-shards files.** Opaque hashed subdirs (git-object style) would scan fast but break the "readable
  files you own in any editor" thesis, so they are rejected. Resolution:
  - Modern filesystems index directories (ext4 htree, XFS, APFS), so a large dir is _not_ catastrophic for the indexer's recursive walk — the real pain is human tooling (`ls`, `git status`, editors)
    and non-indexed filesystems.
  - **No auto-distribution.** A documented soft convention: keep any one directory under ~10k entries. The bulk **importer** shards by a _meaningful_ key (date `YYYY/MM`, or `type`/source) — never a
    hash — so imports stay readable and no folder explodes. Hand-authored content roots rarely approach this.
  - The indexer walks recursively and is depth-agnostic; `[[ ]]` resolves by slug, so whatever foldering the user or importer chooses is transparent to links. Physical layout and logical organization
    stay decoupled.
- 100k is import/archive territory, pairing naturally with `MIKU_READONLY`.

## Out of scope

- **Roadmap:** drag/drop image upload to `assets/`, CodeMirror 6 editor, side-by-side live preview (pairs with CM6), `MIKU_READONLY` publish flag, server-side rich-rendering directives (`:::note`,
  transclusion)
- **Rejected:** MDX/JSX — needs a JS runtime, breaks plain-Markdown portability
- **Dropped:** daily note, Obsidian import
- **Postponed (per design decision):** CRDT, canvas, Notion-style databases, plugin runtime, mobile offline sync
