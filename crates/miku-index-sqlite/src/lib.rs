//! SQLite implementation of Miku's backend-neutral index contract.

use async_trait::async_trait;
use futures_util::TryStreamExt;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::str::FromStr;
use std::time::Duration;

use miku_domain::{
    Backlink, DurableProjection, IndexCapabilities, IndexEvent, IndexReader, IndexWriter,
    MentionRecord, PageIndex, PageSummary, SearchHit, SearchRequest, SearchScope, StoreError,
    StoreResult, TagCount,
};
const MENTIONS_READY_VERSION: &str = "2";

/// SQLite-backed index projection.
#[derive(Clone)]
pub struct SqliteIndex {
    pool: SqlitePool,
}

impl DurableProjection for SqliteIndex {}

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

fn contains_ascii_ci(haystack: &str, needle: &str) -> bool {
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() {
        return true;
    }
    if n.len() > h.len() {
        return false;
    }
    h.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n))
}

fn count_ascii_ci(haystack: &str, needle: &str) -> usize {
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.is_empty() || n.len() > h.len() {
        return 0;
    }
    h.windows(n.len())
        .filter(|w| w.eq_ignore_ascii_case(n))
        .count()
}

fn snippet(body: &str, terms: &[&str]) -> String {
    if body.is_empty() {
        return String::new();
    }
    let lower = body.to_lowercase();
    let start = terms
        .iter()
        .find_map(|term| lower.find(&term.to_lowercase()))
        .unwrap_or(0);
    let start_chars = lower[..start].chars().count();
    body.chars().skip(start_chars).take(160).collect()
}

/// Read the frontmatter `aliases` array so it survives alongside the raw
/// frontmatter blob, matching `miku_indexer`'s extraction at write time.
fn frontmatter_aliases(frontmatter: &serde_json::Value) -> Vec<String> {
    frontmatter
        .get("aliases")
        .and_then(serde_json::Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(|value| value.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
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
            let frontmatter: serde_json::Value = serde_json::from_str(&frontmatter_str)
                .map_err(|e| StoreError::Operation(format!("invalid frontmatter JSON: {e}")))?;
            let aliases = frontmatter_aliases(&frontmatter);
            summaries.push(PageSummary {
                path,
                title,
                frontmatter,
                mtime,
                aliases,
            });
        }
        Ok(summaries)
    }

    async fn list_pages_under(&self, prefix: &str) -> StoreResult<Vec<PageSummary>> {
        if prefix.is_empty() {
            return self.list_pages().await;
        }
        let escaped_prefix = prefix
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let rows = sqlx::query_as::<_, (String, String, String, i64)>(
            "SELECT path, title, frontmatter, mtime FROM tb_pages WHERE path LIKE ? ESCAPE '\\' ORDER BY title, path",
        )
        .bind(format!("{escaped_prefix}%"))
        .fetch_all(self.pool())
        .await
        .map_err(database_error)?;

        let mut summaries = Vec::with_capacity(rows.len());
        for (path, title, frontmatter_str, mtime) in rows {
            let frontmatter: serde_json::Value = serde_json::from_str(&frontmatter_str)
                .map_err(|e| StoreError::Operation(format!("invalid frontmatter JSON: {e}")))?;
            let aliases = frontmatter_aliases(&frontmatter);
            summaries.push(PageSummary {
                path,
                title,
                frontmatter,
                mtime,
                aliases,
            });
        }
        Ok(summaries)
    }

    async fn list_tree_pages(&self, prefix: &str) -> StoreResult<Vec<PageSummary>> {
        let escaped_prefix = prefix
            .replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_");
        let rows = sqlx::query_as::<_, (String, String, String, i64)>(
            "WITH scoped AS (
                 SELECT path,
                        CASE
                            WHEN instr(substr(path, length(?) + 1), '/') = 0
                                THEN substr(path, length(?) + 1)
                            ELSE substr(
                                substr(path, length(?) + 1),
                                1,
                                instr(substr(path, length(?) + 1), '/') - 1
                            )
                        END AS child
                 FROM tb_pages
                 WHERE path LIKE ? ESCAPE '\\'
             ),
             selected AS (
                 SELECT min(path) AS path
                 FROM scoped
                 GROUP BY child
             )
             SELECT page.path, page.title, page.frontmatter, page.mtime
             FROM tb_pages AS page
             JOIN selected ON selected.path = page.path
             ORDER BY page.title, page.path",
        )
        .bind(prefix)
        .bind(prefix)
        .bind(prefix)
        .bind(prefix)
        .bind(format!("{escaped_prefix}%"))
        .fetch_all(self.pool())
        .await
        .map_err(database_error)?;

        let mut summaries = Vec::with_capacity(rows.len());
        for (path, title, frontmatter_str, mtime) in rows {
            let frontmatter: serde_json::Value = serde_json::from_str(&frontmatter_str)
                .map_err(|e| StoreError::Operation(format!("invalid frontmatter JSON: {e}")))?;
            let aliases = frontmatter_aliases(&frontmatter);
            summaries.push(PageSummary {
                path,
                title,
                frontmatter,
                mtime,
                aliases,
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
            let frontmatter: serde_json::Value = serde_json::from_str(&frontmatter_str)
                .map_err(|e| StoreError::Operation(format!("invalid frontmatter JSON: {e}")))?;
            let aliases = frontmatter_aliases(&frontmatter);
            Ok(Some(PageSummary {
                path,
                title,
                frontmatter,
                mtime,
                aliases,
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

        let terms: Vec<&str> = query.split_whitespace().collect();
        if terms.is_empty() {
            return Ok(Vec::new());
        }

        let mut builder = sqlx::QueryBuilder::<sqlx::Sqlite>::new(
            "SELECT path, title, body FROM tb_pages WHERE ",
        );

        for (i, term) in terms.iter().enumerate() {
            if i > 0 {
                builder.push(" AND ");
            }
            let escaped = term
                .replace('\\', "\\\\")
                .replace('%', "\\%")
                .replace('_', "\\_");
            let pattern = format!("%{escaped}%");

            match request.scope {
                SearchScope::Title => {
                    builder.push("(path LIKE ");
                    builder.push_bind(pattern.clone());
                    builder.push(" ESCAPE '\\' OR title LIKE ");
                    builder.push_bind(pattern);
                    builder.push(" ESCAPE '\\')");
                }
                SearchScope::Body => {
                    builder.push("body LIKE ");
                    builder.push_bind(pattern);
                    builder.push(" ESCAPE '\\'");
                }
                SearchScope::All => {
                    builder.push("(path LIKE ");
                    builder.push_bind(pattern.clone());
                    builder.push(" ESCAPE '\\' OR title LIKE ");
                    builder.push_bind(pattern.clone());
                    builder.push(" ESCAPE '\\' OR body LIKE ");
                    builder.push_bind(pattern);
                    builder.push(" ESCAPE '\\')");
                }
            }
        }

        let mut stream = builder.build().fetch(self.pool());

        let mut hits: Vec<(f64, SearchHit)> = Vec::new();
        while let Some(row) = stream.try_next().await.map_err(database_error)? {
            let path: &str = row.try_get(0).map_err(database_error)?;
            let title: &str = row.try_get(1).map_err(database_error)?;
            let body: &str = row.try_get(2).map_err(database_error)?;

            let title_match = terms
                .iter()
                .all(|t| contains_ascii_ci(title, t) || contains_ascii_ci(path, t));
            let body_match = terms.iter().all(|t| contains_ascii_ci(body, t));

            let is_match = match request.scope {
                SearchScope::Title => title_match,
                SearchScope::Body => body_match,
                SearchScope::All => title_match || body_match,
            };

            if !is_match {
                continue;
            }

            let mut score = 0.0;
            if title_match {
                score += 10.0;
            }
            for t in &terms {
                let occurrences = count_ascii_ci(body, t);
                score += (occurrences.min(20) as f64) * 0.5;
            }

            let snip = snippet(body, &terms);

            hits.push((
                score,
                SearchHit {
                    path: path.to_string(),
                    title: title.to_string(),
                    snippet: snip,
                },
            ));
        }

        hits.sort_by(|(score_a, hit_a), (score_b, hit_b)| {
            score_b
                .partial_cmp(score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| hit_a.title.cmp(&hit_b.title))
                .then_with(|| hit_a.path.cmp(&hit_b.path))
        });

        hits.truncate(request.limit);

        Ok(hits.into_iter().map(|(_, hit)| hit).collect())
    }

    /// Link-graph resolution now lives entirely in the hot `MemoryIndex`
    /// projection (ADR-0019); the durable SQLite projection no longer
    /// tracks a relational link graph, so this always returns no backlinks.
    async fn backlinks(&self, _path: &str) -> StoreResult<Vec<Backlink>> {
        Ok(Vec::new())
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

    /// Tag resolution now lives entirely in the hot `MemoryIndex` projection
    /// (ADR-0019); the durable SQLite projection no longer maintains a tag
    /// table, so this always returns no tags.
    async fn tags(&self) -> StoreResult<Vec<TagCount>> {
        Ok(Vec::new())
    }

    /// See [`SqliteIndex::tags`]: always empty now that tags are memory-only.
    async fn pages_with_tag(&self, _tag: &str) -> StoreResult<Vec<PageSummary>> {
        Ok(Vec::new())
    }
}

/// Writes a page's content and search document only. Per ADR-0020, the
/// durable SQLite projection stores the raw body text in tb_pages.
async fn replace_page_conn(
    conn: &mut sqlx::SqliteConnection,
    page: PageIndex,
) -> StoreResult<IndexEvent> {
    let path = page.summary.path.clone();
    let frontmatter_str = page.summary.frontmatter.to_string();
    let has_mermaid_int = if page.has_mermaid { 1 } else { 0 };

    sqlx::query(
        "INSERT INTO tb_pages (path, title, body, frontmatter, has_mermaid, mtime)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT (path) DO UPDATE SET
           title = EXCLUDED.title,
           body = EXCLUDED.body,
           frontmatter = EXCLUDED.frontmatter,
           has_mermaid = EXCLUDED.has_mermaid,
           mtime = EXCLUDED.mtime",
    )
    .bind(&path)
    .bind(&page.summary.title)
    .bind(&page.body)
    .bind(&frontmatter_str)
    .bind(has_mermaid_int)
    .bind(page.summary.mtime)
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
        let aliases = vec![format!("alias-{}", path.trim_end_matches(".md"))];
        PageIndex {
            summary: PageSummary {
                path: path.to_string(),
                title: path.trim_end_matches(".md").to_string(),
                frontmatter: serde_json::json!({"status": "draft", "aliases": aliases}),
                mtime: 12345,
                aliases: aliases.clone(),
            },
            body: body.to_string(),
            links,
            tags: tags.into_iter().map(String::from).collect(),
            aliases,
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
            kind: miku_domain::LinkKind::Page,
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
        assert_eq!(hits[0].snippet, "Miku wiki");

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

        // Backlinks and tags are memory-only per ADR-0019; the durable
        // SQLite projection no longer tracks a link graph or tag table.
        assert!(store
            .backlinks("Second")
            .await
            .expect("backlinks")
            .is_empty());
        assert!(store.tags().await.expect("tags").is_empty());
        assert!(store
            .pages_with_tag("note")
            .await
            .expect("pages with tag")
            .is_empty());

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
    async fn test_list_pages_under_scopes_to_the_folder_prefix() {
        let temp_file = NamedTempFile::new().expect("failed to create temp file");
        let temp_path = temp_file.path().to_str().expect("temp path");
        let store = SqliteIndex::open(temp_path).await.expect("open store");

        store
            .replace_pages(vec![
                test_page("folder/One.md", "one", vec![], vec![]),
                test_page("folder/nested/Two.md", "two", vec![], vec![]),
                test_page("folder/nested/Three.md", "three nested", vec![], vec![]),
                test_page("other/Three.md", "three", vec![], vec![]),
                test_page("folder-sibling/Four.md", "four", vec![], vec![]),
            ])
            .await
            .expect("seed pages");

        let scoped = store
            .list_pages_under("folder/")
            .await
            .expect("list pages under folder/");
        let mut scoped_paths: Vec<_> = scoped.iter().map(|page| page.path.clone()).collect();
        scoped_paths.sort();
        assert_eq!(
            scoped_paths,
            vec![
                "folder/One.md",
                "folder/nested/Three.md",
                "folder/nested/Two.md"
            ]
        );

        let tree_pages = store
            .list_tree_pages("folder/")
            .await
            .expect("list one folder tree level");
        assert_eq!(tree_pages.len(), 2);
        assert!(tree_pages.iter().any(|page| page.path == "folder/One.md"));
        assert_eq!(
            tree_pages
                .iter()
                .filter(|page| page.path.starts_with("folder/nested/"))
                .count(),
            1
        );

        let all = store
            .list_pages_under("")
            .await
            .expect("empty prefix lists everything");
        assert_eq!(all.len(), 5);
        assert_eq!(
            store
                .list_tree_pages("")
                .await
                .expect("list root tree level")
                .len(),
            3
        );

        let none = store
            .list_pages_under("nonexistent/")
            .await
            .expect("no match still succeeds");
        assert!(none.is_empty());
    }

    #[tokio::test]
    async fn test_batch_writes() {
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

        // A prior duplicate-tag case exercised transaction rollback here via
        // a tb_tags primary-key conflict; that table is retired per
        // ADR-0019, and tb_pages/tb_pages_fts have no writer-reachable
        // conflict left to probe atomicity with.
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
        // pages_with_tag no longer reads frontmatter (tags are memory-only
        // per ADR-0019), so malformed JSON elsewhere no longer surfaces here.
        assert!(store
            .pages_with_tag("broken")
            .await
            .expect("pages_with_tag ignores frontmatter")
            .is_empty());
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

    #[tokio::test]
    async fn test_spike_sqlx_streaming_zero_copy() {
        use futures_util::TryStreamExt;
        use sqlx::Row;

        let temp_file = NamedTempFile::new().expect("failed to create temp file");
        let temp_path = temp_file.path().to_str().expect("temp path");
        let store = SqliteIndex::open(temp_path).await.expect("open store");

        store
            .replace_page(test_page(
                "Doc1.md",
                "hello world body content",
                vec![],
                vec![],
            ))
            .await
            .expect("insert doc1");
        store
            .replace_page(test_page(
                "Doc2.md",
                "something else entirely",
                vec![],
                vec![],
            ))
            .await
            .expect("insert doc2");

        let mut stream = sqlx::query("SELECT path, title, body FROM tb_pages").fetch(store.pool());

        let mut matched_paths = Vec::new();
        while let Some(row) = stream.try_next().await.expect("stream next") {
            let path: &str = row.try_get(0).expect("path str");
            let title: &str = row.try_get(1).expect("title str");
            let body: &str = row.try_get(2).expect("body str");

            if contains_ascii_ci(body, "world") {
                matched_paths.push((path.to_string(), title.to_string(), body.to_string()));
            }
        }

        assert_eq!(matched_paths.len(), 1);
        assert_eq!(matched_paths[0].0, "Doc1.md");
    }
}
