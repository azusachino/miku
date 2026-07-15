//! SQLite implementation of Miku's backend-neutral index contract.

use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;
use std::time::Duration;

use miku_domain::{
    Backlink, IndexCapabilities, IndexEvent, IndexReader, IndexWriter, LinkKind, MentionRecord,
    PageIndex, PageSummary, SearchHit, SearchRequest, SearchScope, StoreError, StoreResult,
    TagCount,
};
use miku_indexer::page_slug;

const MENTIONS_READY_VERSION: &str = "2";

/// SQLite-backed index projection.
#[derive(Clone)]
pub struct SqliteIndex {
    pool: SqlitePool,
}

impl SqliteIndex {
    /// Open a new SQLite-backed index at the given path.
    pub async fn open(path: &str) -> StoreResult<Self> {
        let opts = SqliteConnectOptions::from_str(&format!("sqlite://{path}"))
            .map_err(|e| StoreError::InvalidInput(format!("invalid connection string: {e}")))?
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true)
            .busy_timeout(Duration::from_secs(5));

        let pool = SqlitePoolOptions::new()
            .max_connections(4)
            .connect_with(opts)
            .await
            .map_err(|e| StoreError::Unavailable(format!("failed to connect to database: {e}")))?;

        // Run migrations
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .map_err(|e| StoreError::Unavailable(format!("failed to run migrations: {e}")))?;

        // Explicit CREATE VIRTUAL TABLE FTS5 smoke verification
        sqlx::query("CREATE VIRTUAL TABLE IF NOT EXISTS fts5_smoke_test USING fts5(content);")
            .execute(&pool)
            .await
            .map_err(|e| StoreError::Unavailable(format!("FTS5 check failed: {e}")))?;

        sqlx::query("DROP TABLE IF EXISTS fts5_smoke_test;")
            .execute(&pool)
            .await
            .map_err(|e| StoreError::Unavailable(format!("FTS5 cleanup failed: {e}")))?;

        Ok(Self { pool })
    }

    /// Return reference to the underlying connection pool.
    pub fn pool(&self) -> &SqlitePool {
        &self.pool
    }
}

fn database_error(error: sqlx::Error) -> StoreError {
    StoreError::Unavailable(error.to_string())
}

fn page_path(path: &str) -> String {
    if path.ends_with(".md") {
        path.to_string()
    } else {
        format!("{path}.md")
    }
}

fn sanitize_fts5_query(query: &str) -> String {
    query
        .split_whitespace()
        .map(|term| {
            let cleaned: String = term
                .chars()
                .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
                .collect();
            if cleaned.is_empty() {
                String::new()
            } else {
                format!("\"{}\"*", cleaned)
            }
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

#[async_trait]
impl IndexReader for SqliteIndex {
    async fn capabilities(&self) -> StoreResult<IndexCapabilities> {
        Ok(IndexCapabilities {
            durable: true,
            full_text_search: true,
            fuzzy_page_search: false,
            transactions: true,
            remote_sync: false,
        })
    }

    async fn list_pages(&self) -> StoreResult<Vec<PageSummary>> {
        let rows = sqlx::query_as::<_, (String, String, String, i64)>(
            "SELECT path, title, frontmatter, mtime FROM tb_pages ORDER BY title, path",
        )
        .fetch_all(self.pool())
        .await
        .map_err(database_error)?;

        let mut summaries = Vec::with_capacity(rows.len());
        for (path, title, frontmatter_str, mtime) in rows {
            let frontmatter = serde_json::from_str(&frontmatter_str)
                .map_err(|e| StoreError::Operation(format!("invalid frontmatter JSON: {e}")))?;
            summaries.push(PageSummary {
                path,
                title,
                frontmatter,
                mtime,
            });
        }
        Ok(summaries)
    }

    async fn page(&self, path: &str) -> StoreResult<Option<PageSummary>> {
        let row = sqlx::query_as::<_, (String, String, String, i64)>(
            "SELECT path, title, frontmatter, mtime FROM tb_pages WHERE path = ?",
        )
        .bind(page_path(path))
        .fetch_optional(self.pool())
        .await
        .map_err(database_error)?;

        if let Some((path, title, frontmatter_str, mtime)) = row {
            let frontmatter = serde_json::from_str(&frontmatter_str)
                .map_err(|e| StoreError::Operation(format!("invalid frontmatter JSON: {e}")))?;
            Ok(Some(PageSummary {
                path,
                title,
                frontmatter,
                mtime,
            }))
        } else {
            Ok(None)
        }
    }

    async fn search(&self, request: SearchRequest) -> StoreResult<Vec<SearchHit>> {
        let query = request.query.trim();
        if query.is_empty() || request.limit == 0 {
            return Ok(Vec::new());
        }

        let like = format!("%{}%", query.replace('%', "\\%").replace('_', "\\_"));
        let fts_query = sanitize_fts5_query(query);

        let rows = match request.scope {
            SearchScope::Body => {
                if fts_query.is_empty() {
                    return Ok(Vec::new());
                }
                sqlx::query_as::<_, (String, String)>(
                    "SELECT p.path, p.title FROM tb_pages p
                     JOIN tb_pages_fts f ON f.path = p.path
                     WHERE tb_pages_fts MATCH ?
                     ORDER BY bm25(tb_pages_fts, 10.0, 1.0) ASC, p.title
                     LIMIT ?",
                )
                .bind(fts_query)
                .bind(request.limit as i64)
                .fetch_all(self.pool())
                .await
            }
            SearchScope::Title => {
                sqlx::query_as::<_, (String, String)>(
                    "SELECT path, title FROM tb_pages
                     WHERE title LIKE ? ESCAPE '\\' OR path LIKE ? ESCAPE '\\'
                     ORDER BY title, path LIMIT ?",
                )
                .bind(&like)
                .bind(&like)
                .bind(request.limit as i64)
                .fetch_all(self.pool())
                .await
            }
            SearchScope::All => {
                if fts_query.is_empty() {
                    sqlx::query_as::<_, (String, String)>(
                        "SELECT path, title FROM tb_pages
                         WHERE title LIKE ? ESCAPE '\\' OR path LIKE ? ESCAPE '\\'
                         ORDER BY title, path LIMIT ?",
                    )
                    .bind(&like)
                    .bind(&like)
                    .bind(request.limit as i64)
                    .fetch_all(self.pool())
                    .await
                } else {
                    sqlx::query_as::<_, (String, String)>(
                        "SELECT path, title FROM tb_pages
                         WHERE path IN (SELECT path FROM tb_pages_fts WHERE tb_pages_fts MATCH ?)
                            OR title LIKE ? ESCAPE '\\'
                            OR path LIKE ? ESCAPE '\\'
                         ORDER BY title, path LIMIT ?",
                    )
                    .bind(fts_query)
                    .bind(&like)
                    .bind(&like)
                    .bind(request.limit as i64)
                    .fetch_all(self.pool())
                    .await
                }
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
             WHERE target.path = ? AND link.kind = 'page'
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

    async fn mentions_for_target(&self, _path: &str) -> StoreResult<Vec<MentionRecord>> {
        sqlx::query_as::<_, (String, String, String, String, String)>(
            "SELECT target_path, source_path, source_title, matched_text, snippet
             FROM tb_unlinked_mentions
             WHERE target_path = ?
             ORDER BY source_title, source_path, matched_text
             LIMIT 20",
        )
        .bind(page_path(_path))
        .fetch_all(self.pool())
        .await
        .map(|rows| {
            rows.into_iter()
                .map(
                    |(target_path, source_path, source_title, matched_text, snippet)| {
                        MentionRecord {
                            target_path,
                            source_path,
                            source_title,
                            matched_text,
                            snippet,
                        }
                    },
                )
                .collect()
        })
        .map_err(database_error)
    }

    async fn mentions_ready(&self) -> StoreResult<bool> {
        let value = sqlx::query_scalar::<_, String>(
            "SELECT value FROM tb_index_meta WHERE key = 'mentions_ready'",
        )
        .fetch_optional(self.pool())
        .await
        .map_err(database_error)?;
        Ok(value.as_deref() == Some(MENTIONS_READY_VERSION))
    }

    async fn index_metadata(&self, key: &str) -> StoreResult<Option<String>> {
        sqlx::query_scalar("SELECT value FROM tb_index_meta WHERE key = ?")
            .bind(key)
            .fetch_optional(self.pool())
            .await
            .map_err(database_error)
    }

    async fn tags(&self) -> StoreResult<Vec<TagCount>> {
        sqlx::query_as::<_, (String, i64)>(
            "SELECT tag, COUNT(*) FROM tb_tags GROUP BY tag ORDER BY COUNT(*) DESC, tag",
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

    async fn pages_with_tag(&self, tag: &str) -> StoreResult<Vec<PageSummary>> {
        let rows = sqlx::query_as::<_, (String, String, String, i64)>(
            "SELECT p.path, p.title, p.frontmatter, p.mtime
             FROM tb_tags t JOIN tb_pages p ON p.id = t.page_id
             WHERE t.tag = ? ORDER BY p.title, p.path",
        )
        .bind(tag)
        .fetch_all(self.pool())
        .await
        .map_err(database_error)?;

        let mut summaries = Vec::with_capacity(rows.len());
        for (path, title, frontmatter_str, mtime) in rows {
            let frontmatter = serde_json::from_str(&frontmatter_str)
                .map_err(|e| StoreError::Operation(format!("invalid frontmatter JSON: {e}")))?;
            summaries.push(PageSummary {
                path,
                title,
                frontmatter,
                mtime,
            });
        }
        Ok(summaries)
    }
}

async fn replace_page_conn(
    conn: &mut sqlx::SqliteConnection,
    page: PageIndex,
) -> StoreResult<IndexEvent> {
    let path = page.summary.path.clone();
    let slug = page_slug(&path);
    let frontmatter_str = page.summary.frontmatter.to_string();
    let has_mermaid_int = if page.has_mermaid { 1 } else { 0 };

    let (page_id,): (i64,) = sqlx::query_as(
        "INSERT INTO tb_pages (path, slug, title, frontmatter, has_mermaid, mtime)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT (path) DO UPDATE SET
           slug = EXCLUDED.slug,
           title = EXCLUDED.title,
           frontmatter = EXCLUDED.frontmatter,
           has_mermaid = EXCLUDED.has_mermaid,
           mtime = EXCLUDED.mtime
         RETURNING id",
    )
    .bind(&path)
    .bind(&slug)
    .bind(&page.summary.title)
    .bind(&frontmatter_str)
    .bind(has_mermaid_int)
    .bind(page.summary.mtime)
    .fetch_one(&mut *conn)
    .await
    .map_err(database_error)?;

    sqlx::query("DELETE FROM tb_pages_fts WHERE path = ?")
        .bind(&path)
        .execute(&mut *conn)
        .await
        .map_err(database_error)?;

    sqlx::query("INSERT INTO tb_pages_fts (path, title, body) VALUES (?, ?, ?)")
        .bind(&path)
        .bind(&page.summary.title)
        .bind(&page.body)
        .execute(&mut *conn)
        .await
        .map_err(database_error)?;

    sqlx::query("DELETE FROM tb_links WHERE src_id = ?")
        .bind(page_id)
        .execute(&mut *conn)
        .await
        .map_err(database_error)?;

    sqlx::query("DELETE FROM tb_tags WHERE page_id = ?")
        .bind(page_id)
        .execute(&mut *conn)
        .await
        .map_err(database_error)?;

    sqlx::query("DELETE FROM tb_page_aliases WHERE page_id = ?")
        .bind(page_id)
        .execute(&mut *conn)
        .await
        .map_err(database_error)?;

    for tag in &page.tags {
        sqlx::query("INSERT INTO tb_tags (page_id, tag) VALUES (?, ?)")
            .bind(page_id)
            .bind(tag)
            .execute(&mut *conn)
            .await
            .map_err(database_error)?;
    }

    for alias in &page.aliases {
        sqlx::query("INSERT INTO tb_page_aliases (page_id, alias) VALUES (?, ?)")
            .bind(page_id)
            .bind(alias)
            .execute(&mut *conn)
            .await
            .map_err(database_error)?;
    }

    for link in &page.links {
        let kind = match link.kind {
            LinkKind::Page => "page",
            LinkKind::Asset => "asset",
        };
        let is_embed_int = if link.is_embed { 1 } else { 0 };
        sqlx::query(
            "INSERT INTO tb_links (src_id, kind, is_embed, target, target_norm, alias)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT (src_id, kind, target_norm, is_embed) DO NOTHING",
        )
        .bind(page_id)
        .bind(kind)
        .bind(is_embed_int)
        .bind(&link.target)
        .bind(&link.target_norm)
        .bind(&link.alias)
        .execute(&mut *conn)
        .await
        .map_err(database_error)?;
    }

    sqlx::query(
        "UPDATE tb_links
         SET target_id = target.id
         FROM tb_pages target
         WHERE tb_links.src_id = ? AND tb_links.kind = 'page'
           AND tb_links.target_norm = target.slug",
    )
    .bind(page_id)
    .execute(&mut *conn)
    .await
    .map_err(database_error)?;

    sqlx::query("UPDATE tb_links SET target_id = ? WHERE target_norm = ? AND target_id IS NULL")
        .bind(page_id)
        .bind(&slug)
        .execute(&mut *conn)
        .await
        .map_err(database_error)?;

    Ok(IndexEvent::PageIndexed { path })
}

#[async_trait]
impl IndexWriter for SqliteIndex {
    async fn replace_page(&self, page: PageIndex) -> StoreResult<IndexEvent> {
        let mut tx = self.pool.begin().await.map_err(database_error)?;
        let event = replace_page_conn(&mut tx, page).await?;
        tx.commit().await.map_err(database_error)?;
        Ok(event)
    }

    async fn replace_pages(&self, pages: Vec<PageIndex>) -> StoreResult<Vec<IndexEvent>> {
        let mut tx = self.pool.begin().await.map_err(database_error)?;
        let mut events = Vec::with_capacity(pages.len());
        for page in pages {
            events.push(replace_page_conn(&mut tx, page).await?);
        }
        tx.commit().await.map_err(database_error)?;
        Ok(events)
    }

    async fn replace_mentions_for_source(
        &self,
        source_path: &str,
        mentions: Vec<MentionRecord>,
    ) -> StoreResult<()> {
        let mut tx = self.pool.begin().await.map_err(database_error)?;
        let normalized_source = page_path(source_path);

        sqlx::query("DELETE FROM tb_unlinked_mentions WHERE source_path = ?")
            .bind(&normalized_source)
            .execute(&mut *tx)
            .await
            .map_err(database_error)?;

        for mention in mentions {
            sqlx::query(
                "INSERT INTO tb_unlinked_mentions
                 (target_path, source_path, source_title, matched_text, snippet)
                 VALUES (?, ?, ?, ?, ?)
                 ON CONFLICT (target_path, source_path, matched_text) DO UPDATE SET
                   source_title = EXCLUDED.source_title, snippet = EXCLUDED.snippet",
            )
            .bind(page_path(&mention.target_path))
            .bind(page_path(&mention.source_path))
            .bind(mention.source_title)
            .bind(mention.matched_text)
            .bind(mention.snippet)
            .execute(&mut *tx)
            .await
            .map_err(database_error)?;
        }
        tx.commit().await.map_err(database_error)
    }

    async fn replace_mentions_for_sources(
        &self,
        entries: Vec<(String, Vec<MentionRecord>)>,
    ) -> StoreResult<()> {
        let mut tx = self.pool.begin().await.map_err(database_error)?;
        for (source_path, mentions) in entries {
            let normalized_source = page_path(&source_path);
            sqlx::query("DELETE FROM tb_unlinked_mentions WHERE source_path = ?")
                .bind(&normalized_source)
                .execute(&mut *tx)
                .await
                .map_err(database_error)?;
            for mention in mentions {
                sqlx::query(
                    "INSERT INTO tb_unlinked_mentions
                     (target_path, source_path, source_title, matched_text, snippet)
                     VALUES (?, ?, ?, ?, ?)
                     ON CONFLICT (target_path, source_path, matched_text) DO UPDATE SET
                       source_title = EXCLUDED.source_title, snippet = EXCLUDED.snippet",
                )
                .bind(page_path(&mention.target_path))
                .bind(page_path(&mention.source_path))
                .bind(mention.source_title)
                .bind(mention.matched_text)
                .bind(mention.snippet)
                .execute(&mut *tx)
                .await
                .map_err(database_error)?;
            }
        }
        tx.commit().await.map_err(database_error)
    }

    async fn delete_mentions_for_source(&self, source_path: &str) -> StoreResult<()> {
        sqlx::query("DELETE FROM tb_unlinked_mentions WHERE source_path = ?")
            .bind(page_path(source_path))
            .execute(self.pool())
            .await
            .map_err(database_error)
            .map(|_| ())
    }

    async fn delete_mentions_for_target(&self, target_path: &str) -> StoreResult<()> {
        sqlx::query("DELETE FROM tb_unlinked_mentions WHERE target_path = ?")
            .bind(page_path(target_path))
            .execute(self.pool())
            .await
            .map_err(database_error)
            .map(|_| ())
    }

    async fn delete_mentions_for_targets(&self, target_paths: Vec<String>) -> StoreResult<()> {
        if target_paths.is_empty() {
            return Ok(());
        }
        let placeholders = target_paths
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let query_str = format!(
            "DELETE FROM tb_unlinked_mentions WHERE target_path IN ({})",
            placeholders
        );
        let mut query = sqlx::query(&query_str);
        for path in target_paths {
            query = query.bind(page_path(&path));
        }
        query.execute(self.pool()).await.map_err(database_error)?;
        Ok(())
    }

    async fn mark_mentions_ready(&self) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO tb_index_meta (key, value) VALUES ('mentions_ready', ?)
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
        )
        .bind(MENTIONS_READY_VERSION)
        .execute(self.pool())
        .await
        .map_err(database_error)
        .map(|_| ())
    }

    async fn set_index_metadata(&self, key: &str, value: &str) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO tb_index_meta (key, value) VALUES (?, ?)
             ON CONFLICT (key) DO UPDATE SET value = EXCLUDED.value",
        )
        .bind(key)
        .bind(value)
        .execute(self.pool())
        .await
        .map_err(database_error)
        .map(|_| ())
    }

    async fn delete_page(&self, path: &str) -> StoreResult<IndexEvent> {
        let mut tx = self.pool.begin().await.map_err(database_error)?;
        let normalized_path = page_path(path);

        // Delete unlinked mentions where this page is source or target
        sqlx::query("DELETE FROM tb_unlinked_mentions WHERE source_path = ?")
            .bind(&normalized_path)
            .execute(&mut *tx)
            .await
            .map_err(database_error)?;

        sqlx::query("DELETE FROM tb_unlinked_mentions WHERE target_path = ?")
            .bind(&normalized_path)
            .execute(&mut *tx)
            .await
            .map_err(database_error)?;

        // Delete from FTS virtual table
        sqlx::query("DELETE FROM tb_pages_fts WHERE path = ?")
            .bind(&normalized_path)
            .execute(&mut *tx)
            .await
            .map_err(database_error)?;

        // Delete from tb_pages
        sqlx::query("DELETE FROM tb_pages WHERE path = ?")
            .bind(&normalized_path)
            .execute(&mut *tx)
            .await
            .map_err(database_error)?;

        tx.commit().await.map_err(database_error)?;

        Ok(IndexEvent::PageDeleted {
            path: normalized_path,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn test_page(
        path: &str,
        body: &str,
        tags: Vec<&str>,
        links: Vec<miku_domain::LinkRecord>,
    ) -> PageIndex {
        PageIndex {
            summary: PageSummary {
                path: path.to_string(),
                title: path.trim_end_matches(".md").to_string(),
                frontmatter: serde_json::json!({"status": "draft"}),
                mtime: 12345,
            },
            body: body.to_string(),
            links,
            tags: tags.into_iter().map(String::from).collect(),
            aliases: vec![format!("alias-{}", path.trim_end_matches(".md"))],
            has_mermaid: true,
            signals: Default::default(),
        }
    }

    #[tokio::test]
    async fn test_open_and_smoke_verification() {
        let temp_file = NamedTempFile::new().expect("failed to create temp file");
        let temp_path = temp_file
            .path()
            .to_str()
            .expect("failed to get path string");

        let store_result = SqliteIndex::open(temp_path).await;
        assert!(
            store_result.is_ok(),
            "SqliteIndex::open failed: {:?}",
            store_result.err()
        );

        let store = store_result.unwrap();
        let capabilities = store
            .capabilities()
            .await
            .expect("failed to get capabilities");
        assert!(capabilities.durable);
        assert!(capabilities.full_text_search);
        assert!(!capabilities.fuzzy_page_search);
        assert!(capabilities.transactions);
        assert!(!capabilities.remote_sync);
    }

    #[tokio::test]
    async fn test_sqlite_index_trait_behavior() {
        let temp_file = NamedTempFile::new().expect("failed to create temp file");
        let temp_path = temp_file
            .path()
            .to_str()
            .expect("failed to get path string");

        let store = SqliteIndex::open(temp_path)
            .await
            .expect("failed to open store");

        // Write pages
        let link1 = miku_domain::LinkRecord {
            target: "Second.md".to_string(),
            target_norm: "second".to_string(),
            alias: Some("alias_link".to_string()),
            kind: LinkKind::Page,
            is_embed: false,
        };
        let page1 = test_page(
            "First.md",
            "This is the first page about Miku wiki",
            vec!["miku", "wiki"],
            vec![link1],
        );
        let page2 = test_page("Second.md", "This is another note", vec!["note"], vec![]);

        store.replace_page(page1).await.expect("replace page 1");
        store.replace_page(page2).await.expect("replace page 2");

        // List pages
        let summaries = store.list_pages().await.expect("list pages");
        assert_eq!(summaries.len(), 2);
        assert_eq!(summaries[0].title, "First");
        assert_eq!(summaries[1].title, "Second");

        // Get single page
        let page_opt = store.page("First").await.expect("get page");
        assert!(page_opt.is_some());
        let page = page_opt.unwrap();
        assert_eq!(page.title, "First");
        assert_eq!(page.frontmatter["status"], "draft");

        // Search body
        let hits = store
            .search(SearchRequest {
                query: "Miku".to_string(),
                scope: SearchScope::Body,
                limit: 10,
            })
            .await
            .expect("search body");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "First.md");

        // Search title
        let hits = store
            .search(SearchRequest {
                query: "Sec".to_string(),
                scope: SearchScope::Title,
                limit: 10,
            })
            .await
            .expect("search title");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "Second.md");

        // Search all
        let hits = store
            .search(SearchRequest {
                query: "note".to_string(),
                scope: SearchScope::All,
                limit: 10,
            })
            .await
            .expect("search all");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "Second.md");

        // Punctuation search does not crash
        let punctuation_hits = store
            .search(SearchRequest {
                query: "!!! *** []".to_string(),
                scope: SearchScope::All,
                limit: 10,
            })
            .await
            .expect("search punctuation");
        assert!(punctuation_hits.is_empty());

        // Backlinks
        let links = store.backlinks("Second").await.expect("backlinks");
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].path, "First");

        // Tags
        let tags = store.tags().await.expect("tags");
        assert_eq!(tags.len(), 3);
        assert_eq!(tags[0].tag, "miku");
        assert_eq!(tags[0].count, 1);

        let tagged_pages = store.pages_with_tag("note").await.expect("pages with tag");
        assert_eq!(tagged_pages.len(), 1);
        assert_eq!(tagged_pages[0].path, "Second.md");

        // Mentions
        let mention = MentionRecord {
            target_path: "Second.md".to_string(),
            source_path: "First.md".to_string(),
            source_title: "First".to_string(),
            matched_text: "Second".to_string(),
            snippet: "mentioning Second here".to_string(),
        };
        store
            .replace_mentions_for_source("First", vec![mention])
            .await
            .expect("replace mentions");

        let has_ready = store.mentions_ready().await.expect("mentions ready before");
        assert!(!has_ready);

        store
            .mark_mentions_ready()
            .await
            .expect("mark mentions ready");
        let has_ready = store.mentions_ready().await.expect("mentions ready after");
        assert!(has_ready);

        let mentions = store
            .mentions_for_target("Second")
            .await
            .expect("mentions for target");
        assert_eq!(mentions.len(), 1);
        assert_eq!(mentions[0].source_path, "First.md");

        // Delete page
        let event = store.delete_page("Second").await.expect("delete page");
        assert_eq!(
            event,
            IndexEvent::PageDeleted {
                path: "Second.md".to_string()
            }
        );
        let summaries = store.list_pages().await.expect("list pages post delete");
        assert_eq!(summaries.len(), 1);

        // Mentions should be cleaned up post delete
        let mentions = store
            .mentions_for_target("Second")
            .await
            .expect("mentions post delete");
        assert!(mentions.is_empty());

        let backlinks = store
            .backlinks("Second")
            .await
            .expect("backlinks post delete");
        assert!(backlinks.is_empty());
        let hits = store
            .search(SearchRequest {
                query: "another".to_string(),
                scope: SearchScope::Body,
                limit: 10,
            })
            .await
            .expect("search post delete");
        assert!(hits.is_empty());

        assert!(store.page("Missing").await.expect("missing page").is_none());
    }

    #[tokio::test]
    async fn test_batch_writes_and_transaction_rollback() {
        let temp_file = NamedTempFile::new().expect("failed to create temp file");
        let temp_path = temp_file.path().to_str().expect("temp path");
        let store = SqliteIndex::open(temp_path).await.expect("open store");

        let events = store
            .replace_pages(vec![
                test_page("BatchOne.md", "first batch body", vec![], vec![]),
                test_page("BatchTwo.md", "second batch body", vec![], vec![]),
            ])
            .await
            .expect("replace batch");
        assert_eq!(events.len(), 2);
        assert!(store
            .replace_pages(Vec::new())
            .await
            .expect("empty batch")
            .is_empty());

        let invalid = test_page(
            "Broken.md",
            "must not commit",
            vec!["duplicate", "duplicate"],
            vec![],
        );
        assert!(store.replace_page(invalid).await.is_err());
        assert!(store
            .page("Broken")
            .await
            .expect("rollback lookup")
            .is_none());
        assert!(store
            .search(SearchRequest {
                query: "must not commit".to_string(),
                scope: SearchScope::Body,
                limit: 10,
            })
            .await
            .expect("rollback search")
            .is_empty());

        store.rebuild_search_index().await.expect("default rebuild");
    }

    #[tokio::test]
    async fn test_search_edges_and_escaping() {
        let temp_file = NamedTempFile::new().expect("failed to create temp file");
        let temp_path = temp_file.path().to_str().expect("temp path");
        let store = SqliteIndex::open(temp_path).await.expect("open store");
        store
            .replace_page(test_page(
                "Percent%_Page.md",
                "body content",
                vec![],
                vec![],
            ))
            .await
            .expect("replace page");

        for scope in [SearchScope::Body, SearchScope::Title, SearchScope::All] {
            assert!(store
                .search(SearchRequest {
                    query: String::new(),
                    scope,
                    limit: 10,
                })
                .await
                .expect("empty search")
                .is_empty());
            assert!(store
                .search(SearchRequest {
                    query: "body".to_string(),
                    scope,
                    limit: 0,
                })
                .await
                .expect("zero-limit search")
                .is_empty());
        }

        let escaped = store
            .search(SearchRequest {
                query: "%_".to_string(),
                scope: SearchScope::Title,
                limit: 10,
            })
            .await
            .expect("escaped title search");
        assert_eq!(escaped.len(), 1);
        assert_eq!(escaped[0].path, "Percent%_Page.md");

        let metadata_only = store
            .search(SearchRequest {
                query: "Percent".to_string(),
                scope: SearchScope::All,
                limit: 10,
            })
            .await
            .expect("metadata search");
        assert_eq!(metadata_only.len(), 1);
    }

    #[tokio::test]
    async fn test_mentions_batches_and_deletions() {
        let temp_file = NamedTempFile::new().expect("failed to create temp file");
        let temp_path = temp_file.path().to_str().expect("temp path");
        let store = SqliteIndex::open(temp_path).await.expect("open store");
        let mention = |target: &str, source: &str| MentionRecord {
            target_path: target.to_string(),
            source_path: source.to_string(),
            source_title: source.trim_end_matches(".md").to_string(),
            matched_text: target.trim_end_matches(".md").to_string(),
            snippet: "context".to_string(),
        };

        store
            .replace_mentions_for_sources(vec![
                (
                    "SourceOne".to_string(),
                    vec![mention("TargetOne", "SourceOne")],
                ),
                (
                    "SourceTwo".to_string(),
                    vec![mention("TargetTwo", "SourceTwo")],
                ),
            ])
            .await
            .expect("replace mention batch");
        assert_eq!(
            store
                .mentions_for_target("TargetOne")
                .await
                .expect("target one")
                .len(),
            1
        );
        assert_eq!(
            store
                .mentions_for_target("TargetTwo")
                .await
                .expect("target two")
                .len(),
            1
        );

        store
            .delete_mentions_for_source("SourceOne")
            .await
            .expect("delete source mentions");
        assert!(store
            .mentions_for_target("TargetOne")
            .await
            .expect("deleted source")
            .is_empty());
        store
            .delete_mentions_for_target("TargetTwo")
            .await
            .expect("delete target mentions");
        assert!(store
            .mentions_for_target("TargetTwo")
            .await
            .expect("deleted target")
            .is_empty());

        store
            .replace_mentions_for_sources(vec![
                (
                    "SourceOne".to_string(),
                    vec![mention("TargetOne", "SourceOne")],
                ),
                (
                    "SourceTwo".to_string(),
                    vec![mention("TargetTwo", "SourceTwo")],
                ),
            ])
            .await
            .expect("restore mention batch");
        store
            .delete_mentions_for_targets(vec!["TargetOne".to_string(), "TargetTwo".to_string()])
            .await
            .expect("delete target batch");
        assert!(store
            .mentions_for_target("TargetOne")
            .await
            .expect("batch target one")
            .is_empty());
        assert!(store
            .mentions_for_target("TargetTwo")
            .await
            .expect("batch target two")
            .is_empty());
        store
            .delete_mentions_for_targets(Vec::new())
            .await
            .expect("empty target batch");
    }

    #[tokio::test]
    async fn test_malformed_persisted_frontmatter_is_reported() {
        let temp_file = NamedTempFile::new().expect("failed to create temp file");
        let temp_path = temp_file.path().to_str().expect("temp path");
        let store = SqliteIndex::open(temp_path).await.expect("open store");
        store
            .replace_page(test_page("Malformed.md", "body", vec!["broken"], vec![]))
            .await
            .expect("replace page");
        sqlx::query("UPDATE tb_pages SET frontmatter = '{broken' WHERE path = ?")
            .bind("Malformed.md")
            .execute(store.pool())
            .await
            .expect("corrupt frontmatter");
        assert!(store.list_pages().await.is_err());
        assert!(store.pages_with_tag("broken").await.is_err());
    }

    #[tokio::test]
    async fn test_persistence_across_reopen() {
        let temp_file = NamedTempFile::new().expect("failed to create temp file");
        let temp_path = temp_file
            .path()
            .to_str()
            .expect("failed to get path string");

        {
            let store = SqliteIndex::open(temp_path).await.expect("open first");
            let page = test_page("Today.md", "Miku note", vec![], vec![]);
            store.replace_page(page).await.expect("replace page");
        }

        {
            let reopened = SqliteIndex::open(temp_path).await.expect("open reopen");
            let summaries = reopened
                .list_pages()
                .await
                .expect("list pages after reopen");
            assert_eq!(summaries.len(), 1);
            assert_eq!(summaries[0].path, "Today.md");

            let hits = reopened
                .search(SearchRequest {
                    query: "Miku".to_string(),
                    scope: SearchScope::Body,
                    limit: 10,
                })
                .await
                .expect("search reopened");
            assert_eq!(hits.len(), 1);
        }
    }
}
