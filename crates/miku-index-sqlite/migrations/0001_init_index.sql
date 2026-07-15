-- Miku SQLite disposable index schema.
-- Source of truth is miku/**/*.md; every table here is rebuildable from files.
-- Single-writer: only the background indexer writes; HTTP handlers read.

-- One row per Markdown file under miku/.
CREATE TABLE tb_pages (
  id          INTEGER PRIMARY KEY AUTOINCREMENT,
  path        TEXT NOT NULL UNIQUE,            -- relative to miku/, e.g. 'sub/Bar.md'
  slug        TEXT NOT NULL,                   -- normalized basename for [[ ]] resolution
  title       TEXT NOT NULL,                   -- frontmatter title, else first H1, else filename
  frontmatter TEXT NOT NULL DEFAULT '{}',      -- JSON as text
  has_mermaid INTEGER NOT NULL DEFAULT 0,      -- boolean 0/1
  mtime       INTEGER NOT NULL                 -- file mtime (unix) for startup reconcile
);

CREATE INDEX idx_pages_slug        ON tb_pages (slug);

-- Directed link/embed edges; one row per normalized target per source page.
CREATE TABLE tb_links (
  src_id      INTEGER NOT NULL REFERENCES tb_pages(id) ON DELETE CASCADE,
  kind        TEXT NOT NULL CHECK (kind IN ('page', 'asset')),
  is_embed    INTEGER NOT NULL DEFAULT 0,              -- boolean 0/1
  target      TEXT NOT NULL,                           -- link name as written
  target_norm TEXT NOT NULL,                           -- normalized resolver key
  target_id   INTEGER REFERENCES tb_pages(id) ON DELETE SET NULL,
  alias       TEXT,                                    -- display text from [[target|alias]]
  PRIMARY KEY (src_id, kind, target_norm, is_embed)
);
CREATE INDEX idx_links_target_id   ON tb_links(target_id);    -- backlinks lookup
CREATE INDEX idx_links_target_norm ON tb_links(target_norm);  -- dangling re-resolve

-- Tags from inline #tag and frontmatter `tags:` (merged into one set per page).
CREATE TABLE tb_tags (
  page_id INTEGER NOT NULL REFERENCES tb_pages(id) ON DELETE CASCADE,
  tag     TEXT NOT NULL,
  PRIMARY KEY (page_id, tag)
);
CREATE INDEX idx_tags_tag ON tb_tags(tag);

-- Page-declared aliases from frontmatter `aliases:`; lets [[Alias]] resolve to the page.
CREATE TABLE tb_page_aliases (
  page_id INTEGER NOT NULL REFERENCES tb_pages(id) ON DELETE CASCADE,
  alias   TEXT NOT NULL,
  PRIMARY KEY (page_id, alias)
);
CREATE INDEX idx_page_aliases_alias ON tb_page_aliases(alias);

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
