-- Miku disposable index schema.
-- Source of truth is miku/**/*.md; every table here is rebuildable from files.
-- Single-writer: only the background indexer writes; HTTP handlers read.

-- Fuzzy title/slug/alias matching for [[ ]] autocomplete and the Ctrl-K palette.
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- One row per Markdown file under miku/.
CREATE TABLE tb_pages (
  id          BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
  path        TEXT NOT NULL UNIQUE,            -- relative to miku/, e.g. 'sub/Bar.md'
  slug        TEXT NOT NULL,                   -- normalized basename for [[ ]] resolution
  title       TEXT NOT NULL,                   -- frontmatter title, else first H1, else filename
  frontmatter JSONB NOT NULL DEFAULT '{}',     -- opaque user properties (interpreted keys fan out below)
  has_mermaid BOOLEAN NOT NULL DEFAULT false,  -- index-driven client-asset injection (lazy-load mermaid.js)
  mtime       BIGINT NOT NULL,                 -- file mtime (unix) for startup reconcile
  body_tsv    TSVECTOR,                        -- FTS vector, set by the indexer (title weight A, body B)
  indexed_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_pages_tsv         ON tb_pages USING GIN (body_tsv);
CREATE INDEX idx_pages_frontmatter ON tb_pages USING GIN (frontmatter jsonb_path_ops);
CREATE INDEX idx_pages_slug        ON tb_pages (slug);
CREATE INDEX idx_pages_slug_trgm   ON tb_pages USING GIN (slug  gin_trgm_ops);
CREATE INDEX idx_pages_title_trgm  ON tb_pages USING GIN (title gin_trgm_ops);

-- Directed link/embed edges; one row per normalized target per source page.
-- target_id points at tb_pages only. It stays NULL for dangling page links and
-- asset links, whose bytes live under miku/assets/.
CREATE TABLE tb_links (
  src_id      BIGINT NOT NULL REFERENCES tb_pages(id) ON DELETE CASCADE,
  kind        TEXT   NOT NULL CHECK (kind IN ('page', 'asset')),
  is_embed    BOOLEAN NOT NULL DEFAULT false,          -- true for ![[target]]
  target      TEXT   NOT NULL,                         -- link name as written
  target_norm TEXT   NOT NULL,                         -- normalized resolver key
  target_id   BIGINT REFERENCES tb_pages(id) ON DELETE SET NULL,
  alias       TEXT,                                    -- display text from [[target|alias]]
  PRIMARY KEY (src_id, kind, target_norm, is_embed)
);
CREATE INDEX idx_links_target_id   ON tb_links(target_id);    -- backlinks lookup
CREATE INDEX idx_links_target_norm ON tb_links(target_norm);  -- dangling re-resolve / asset reports

-- Tags from inline #tag and frontmatter `tags:` (merged into one set per page).
CREATE TABLE tb_tags (
  page_id BIGINT NOT NULL REFERENCES tb_pages(id) ON DELETE CASCADE,
  tag     TEXT   NOT NULL,
  PRIMARY KEY (page_id, tag)
);
CREATE INDEX idx_tags_tag ON tb_tags(tag);
CREATE INDEX idx_tags_tag_pattern ON tb_tags(tag text_pattern_ops);

-- Page-declared aliases from frontmatter `aliases:`; lets [[Alias]] resolve to
-- the page. Distinct from tb_links.alias (which is per-link display text).
-- Cross-page collisions resolve deterministically (shortest path), mirroring
-- the basename rule — so this is PK(page_id, alias), not a global unique.
CREATE TABLE tb_page_aliases (
  page_id BIGINT NOT NULL REFERENCES tb_pages(id) ON DELETE CASCADE,
  alias   TEXT   NOT NULL,
  PRIMARY KEY (page_id, alias)
);
CREATE INDEX idx_page_aliases_alias ON tb_page_aliases(alias);
