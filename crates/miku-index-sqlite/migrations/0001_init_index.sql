-- Miku SQLite disposable index schema.
-- Source of truth is miku/**/*.md; every table here is rebuildable from files.
-- Single-writer: only the background indexer writes; HTTP handlers read.
--
-- Per ADR-0019, this is a flat page KV + FTS5 cache: link, slug, alias, tag,
-- and backlink resolution live entirely in the hot MemoryIndex projection,
-- not as relational join targets here.

-- One row per Markdown file under miku/.
CREATE TABLE tb_pages (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  path        TEXT NOT NULL UNIQUE,            -- relative to miku/, e.g. 'sub/Bar.md'
  title       TEXT NOT NULL,                   -- frontmatter title, else filename stem
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

-- FTS5 virtual table for full-text search
CREATE VIRTUAL TABLE tb_pages_fts USING fts5(
  path UNINDEXED, title, body, tokenize = 'porter unicode61'
);
