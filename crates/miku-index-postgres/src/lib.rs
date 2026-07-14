//! Postgres implementation of Miku's backend-neutral index contract.

use async_trait::async_trait;
use miku_domain::{
    Backlink, IndexCapabilities, IndexEvent, IndexReader, IndexWriter, LinkKind, PageIndex,
    PageSummary, SearchHit, SearchRequest, SearchScope, StoreError, StoreResult, TagCount,
    UnlinkedMention,
};
use miku_indexer::page_slug;
use sqlx::PgPool;

/// Postgres-backed index projection using the repository's existing schema.
#[derive(Clone)]
pub struct PostgresIndex {
    pool: PgPool,
}

impl PostgresIndex {
    /// Wrap an already migrated connection pool.
    #[must_use]
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    fn pool(&self) -> &PgPool {
        &self.pool
    }
}

fn database_error(error: sqlx::Error) -> StoreError {
    StoreError::Unavailable(error.to_string())
}

fn page_path(path: &str) -> String {
    path.strip_suffix(".md")
        .map_or_else(|| format!("{path}.md"), str::to_string)
}

#[async_trait]
impl IndexReader for PostgresIndex {
    async fn capabilities(&self) -> StoreResult<IndexCapabilities> {
        Ok(IndexCapabilities {
            durable: true,
            full_text_search: true,
            fuzzy_page_search: true,
            transactions: true,
            remote_sync: false,
        })
    }

    async fn list_pages(&self) -> StoreResult<Vec<PageSummary>> {
        sqlx::query_as::<_, (String, String, serde_json::Value, i64)>(
            "SELECT path, title, frontmatter, mtime FROM tb_pages ORDER BY title, path",
        )
        .fetch_all(self.pool())
        .await
        .map(|rows| {
            rows.into_iter()
                .map(|(path, title, frontmatter, mtime)| PageSummary {
                    path,
                    title,
                    frontmatter,
                    mtime,
                })
                .collect()
        })
        .map_err(database_error)
    }

    async fn page(&self, path: &str) -> StoreResult<Option<PageSummary>> {
        sqlx::query_as::<_, (String, String, serde_json::Value, i64)>(
            "SELECT path, title, frontmatter, mtime FROM tb_pages WHERE path = $1",
        )
        .bind(page_path(path))
        .fetch_optional(self.pool())
        .await
        .map(|row| {
            row.map(|(path, title, frontmatter, mtime)| PageSummary {
                path,
                title,
                frontmatter,
                mtime,
            })
        })
        .map_err(database_error)
    }

    async fn search(&self, request: SearchRequest) -> StoreResult<Vec<SearchHit>> {
        let query = request.query.trim();
        if query.is_empty() || request.limit == 0 {
            return Ok(Vec::new());
        }
        let like = format!("%{}%", query.replace('%', "\\%").replace('_', "\\_"));
        let rows = match request.scope {
            SearchScope::Body => {
                sqlx::query_as::<_, (String, String)>(
                    "SELECT path, title FROM tb_pages
                 WHERE body_tsv @@ websearch_to_tsquery('english', $1)
                 ORDER BY ts_rank(body_tsv, websearch_to_tsquery('english', $1)) DESC, title
                 LIMIT $2",
                )
                .bind(query)
                .bind(request.limit as i64)
                .fetch_all(self.pool())
                .await
            }
            SearchScope::Title => {
                sqlx::query_as::<_, (String, String)>(
                    "SELECT path, title FROM tb_pages
                 WHERE title ILIKE $1 ESCAPE '\\' OR path ILIKE $1 ESCAPE '\\'
                 ORDER BY title, path LIMIT $2",
                )
                .bind(like)
                .bind(request.limit as i64)
                .fetch_all(self.pool())
                .await
            }
            SearchScope::All => {
                sqlx::query_as::<_, (String, String)>(
                    "SELECT path, title FROM tb_pages
                 WHERE body_tsv @@ websearch_to_tsquery('english', $1)
                    OR title ILIKE $2 ESCAPE '\\'
                    OR path ILIKE $2 ESCAPE '\\'
                 ORDER BY title, path LIMIT $3",
                )
                .bind(query)
                .bind(like)
                .bind(request.limit as i64)
                .fetch_all(self.pool())
                .await
            }
        };
        rows.map(|rows| {
            rows.into_iter()
                .map(|(path, title)| SearchHit {
                    path,
                    title,
                    snippet: String::new(),
                })
                .collect()
        })
        .map_err(database_error)
    }

    async fn backlinks(&self, path: &str) -> StoreResult<Vec<Backlink>> {
        sqlx::query_as::<_, (String, String)>(
            "SELECT DISTINCT src.path, src.title
             FROM tb_links link
             JOIN tb_pages target ON target.id = link.target_id
             JOIN tb_pages src ON src.id = link.src_id
             WHERE target.path = $1 AND link.kind = 'page'
             ORDER BY src.title, src.path LIMIT 50",
        )
        .bind(page_path(path))
        .fetch_all(self.pool())
        .await
        .map(|rows| {
            rows.into_iter()
                .map(|(path, title)| Backlink {
                    path: path.strip_suffix(".md").unwrap_or(&path).to_string(),
                    title,
                })
                .collect()
        })
        .map_err(database_error)
    }

    async fn unlinked_mentions(&self, _path: &str) -> StoreResult<Vec<UnlinkedMention>> {
        // The raw Markdown body belongs to the filesystem source of truth and
        // is intentionally not duplicated in the Postgres projection.
        Ok(Vec::new())
    }

    async fn tags(&self) -> StoreResult<Vec<TagCount>> {
        sqlx::query_as::<_, (String, i64)>(
            "SELECT tag, COUNT(*)::BIGINT FROM tb_tags GROUP BY tag ORDER BY COUNT(*) DESC, tag",
        )
        .fetch_all(self.pool())
        .await
        .map(|rows| {
            rows.into_iter()
                .map(|(tag, count)| TagCount { tag, count })
                .collect()
        })
        .map_err(database_error)
    }
}

#[async_trait]
impl IndexWriter for PostgresIndex {
    async fn replace_page(&self, page: PageIndex) -> StoreResult<IndexEvent> {
        let path = page.summary.path.clone();
        let slug = page_slug(&path);
        let mut tx = self.pool.begin().await.map_err(database_error)?;
        let (page_id,): (i64,) = sqlx::query_as(
            "INSERT INTO tb_pages (path, slug, title, frontmatter, has_mermaid, mtime, body_tsv)
             VALUES ($1, $2, $3, $4, $5, $6,
               setweight(to_tsvector('english', COALESCE($3, '')), 'A') ||
               setweight(to_tsvector('english', COALESCE($7, '')), 'B'))
             ON CONFLICT (path) DO UPDATE SET slug = EXCLUDED.slug,
               title = EXCLUDED.title, frontmatter = EXCLUDED.frontmatter,
               has_mermaid = EXCLUDED.has_mermaid, mtime = EXCLUDED.mtime,
               body_tsv = EXCLUDED.body_tsv
             RETURNING id",
        )
        .bind(&path)
        .bind(&slug)
        .bind(&page.summary.title)
        .bind(&page.summary.frontmatter)
        .bind(page.has_mermaid)
        .bind(page.summary.mtime)
        .bind(&page.body)
        .fetch_one(&mut *tx)
        .await
        .map_err(database_error)?;

        sqlx::query("DELETE FROM tb_links WHERE src_id = $1")
            .bind(page_id)
            .execute(&mut *tx)
            .await
            .map_err(database_error)?;
        sqlx::query("DELETE FROM tb_tags WHERE page_id = $1")
            .bind(page_id)
            .execute(&mut *tx)
            .await
            .map_err(database_error)?;
        sqlx::query("DELETE FROM tb_page_aliases WHERE page_id = $1")
            .bind(page_id)
            .execute(&mut *tx)
            .await
            .map_err(database_error)?;

        for tag in &page.tags {
            sqlx::query("INSERT INTO tb_tags (page_id, tag) VALUES ($1, $2)")
                .bind(page_id)
                .bind(tag)
                .execute(&mut *tx)
                .await
                .map_err(database_error)?;
        }
        for alias in &page.aliases {
            sqlx::query("INSERT INTO tb_page_aliases (page_id, alias) VALUES ($1, $2)")
                .bind(page_id)
                .bind(alias)
                .execute(&mut *tx)
                .await
                .map_err(database_error)?;
        }
        for link in &page.links {
            let kind = match link.kind {
                LinkKind::Page => "page",
                LinkKind::Asset => "asset",
            };
            sqlx::query(
                "INSERT INTO tb_links (src_id, kind, is_embed, target, target_norm, alias)
                 VALUES ($1, $2, $3, $4, $5, $6)
                 ON CONFLICT (src_id, kind, target_norm, is_embed) DO NOTHING",
            )
            .bind(page_id)
            .bind(kind)
            .bind(link.is_embed)
            .bind(&link.target)
            .bind(&link.target_norm)
            .bind(&link.alias)
            .execute(&mut *tx)
            .await
            .map_err(database_error)?;
        }
        sqlx::query(
            "UPDATE tb_links link SET target_id = target.id
             FROM tb_pages target
             WHERE link.src_id = $1 AND link.kind = 'page'
               AND link.target_norm = target.slug",
        )
        .bind(page_id)
        .execute(&mut *tx)
        .await
        .map_err(database_error)?;
        sqlx::query(
            "UPDATE tb_links SET target_id = $1 WHERE target_norm = $2 AND target_id IS NULL",
        )
        .bind(page_id)
        .bind(&slug)
        .execute(&mut *tx)
        .await
        .map_err(database_error)?;
        tx.commit().await.map_err(database_error)?;
        Ok(IndexEvent::PageIndexed { path })
    }

    async fn delete_page(&self, path: &str) -> StoreResult<IndexEvent> {
        sqlx::query("DELETE FROM tb_pages WHERE path = $1")
            .bind(page_path(path))
            .execute(self.pool())
            .await
            .map_err(database_error)?;
        Ok(IndexEvent::PageDeleted {
            path: page_path(path),
        })
    }
}
