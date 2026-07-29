-- Miku SQLite disposable index schema.
-- Source of truth is miku/**/*.md; every table here is rebuildable from files.
-- Single-writer: only the background indexer writes; HTTP handlers read.
--
-- Per ADR-0020, this is a flat page KV cache: link, slug, alias, tag,
-- and backlink resolution live entirely in the hot MemoryIndex projection,
-- not as relational join targets here. Full-text search prefilters via SQL
-- LIKE, then scores/snippets the matched rows over tb_pages.body in Rust
-- via rayon scanning (a measured deviation from the ADR's original
-- fetch-all design; see ADR-0020's implementation status).

-- One row per Markdown file under miku/.
CREATE TABLE tb_pages (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  path        TEXT NOT NULL UNIQUE,            -- relative to miku/, e.g. 'sub/Bar.md'
  title       TEXT NOT NULL,                   -- frontmatter title, else filename stem
  body        TEXT NOT NULL DEFAULT '',        -- raw page body markdown
  frontmatter TEXT NOT NULL DEFAULT '{}',      -- JSON as text
  has_mermaid INTEGER NOT NULL DEFAULT 0,      -- boolean 0/1
  mtime       INTEGER NOT NULL                 -- file mtime (unix) for startup reconcile
);

-- Derived plain-text title/alias mentions.
CREATE TABLE tb_unlinked_mentions (
  target_path  TEXT NOT NULL,
  source_path  TEXT NOT NULL,
  source_title TEXT NOT NULL,
  matched_text TEXT NOT NULL,
  snippet      TEXT NOT NULL,
  PRIMARY KEY (target_path, source_path, matched_text)
);
CREATE INDEX idx_unlinked_mentions_target ON tb_unlinked_mentions(target_path);
CREATE INDEX idx_unlinked_mentions_source ON tb_unlinked_mentions(source_path);

CREATE TABLE tb_index_meta (
  key   TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
