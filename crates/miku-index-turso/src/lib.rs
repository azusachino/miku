//! Durable local index backed by the Rust-built Turso database engine.

use async_trait::async_trait;
use miku_domain::{
    Backlink, IndexCapabilities, IndexEvent, IndexReader, IndexWriter, MentionRecord, PageIndex,
    PageSummary, SearchHit, SearchRequest, StoreError, StoreResult, TagCount,
};
use miku_index_memory::MemoryIndex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::{Mutex, OnceCell};
use turso::{transaction::Transaction, Builder, Connection, Value};

const CREATE_PAGES: &str = "CREATE TABLE IF NOT EXISTS miku_page_index (
    path TEXT PRIMARY KEY NOT NULL,
    payload TEXT NOT NULL,
    title TEXT NOT NULL DEFAULT '',
    body TEXT NOT NULL DEFAULT '',
    mtime INTEGER NOT NULL
)";
const CREATE_SEARCH: &str =
    "CREATE INDEX IF NOT EXISTS miku_page_search ON miku_page_index USING fts (title, body)";
const CREATE_META: &str = "CREATE TABLE IF NOT EXISTS miku_index_meta (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
)";
const CREATE_MENTIONS: &str = "CREATE TABLE IF NOT EXISTS miku_mention_index (
    target_path TEXT NOT NULL,
    source_path TEXT NOT NULL,
    source_title TEXT NOT NULL,
    matched_text TEXT NOT NULL,
    snippet TEXT NOT NULL,
    PRIMARY KEY (target_path, source_path, matched_text)
)";
const CREATE_MENTIONS_TARGET: &str =
    "CREATE INDEX IF NOT EXISTS miku_mention_target ON miku_mention_index (target_path)";
const SEARCH_READY_KEY: &str = "fts_ready";
const SEARCH_READY_VERSION: &str = "1";
const MENTIONS_READY_KEY: &str = "mentions_ready";
const MENTIONS_READY_VERSION: &str = "2";

/// A local durable index using the Rust-built Turso engine.
#[derive(Clone)]
pub struct TursoIndex {
    connection: Arc<Mutex<Connection>>,
    memory: MemoryIndex,
    search_available: Arc<AtomicBool>,
    mentions_ready: Arc<AtomicBool>,
    hydrated: Arc<OnceCell<()>>,
}

impl TursoIndex {
    /// Open a local Turso database and load projections into the read model.
    pub async fn open(path: &str) -> StoreResult<Self> {
        let database = Builder::new_local(path)
            .experimental_index_method(true)
            .build()
            .await
            .map_err(driver_error)?;
        let connection = database.connect().map_err(driver_error)?;
        connection
            .execute(CREATE_PAGES, ())
            .await
            .map_err(driver_error)?;
        connection
            .execute(CREATE_META, ())
            .await
            .map_err(driver_error)?;
        connection
            .execute(CREATE_MENTIONS, ())
            .await
            .map_err(driver_error)?;
        connection
            .execute(CREATE_MENTIONS_TARGET, ())
            .await
            .map_err(driver_error)?;
        let mut search_rows = connection
            .query(
                "SELECT value FROM miku_index_meta WHERE key = ?1 LIMIT 1",
                [SEARCH_READY_KEY.to_string()],
            )
            .await
            .map_err(driver_error)?;
        let search_ready = search_rows
            .next()
            .await
            .map_err(driver_error)?
            .and_then(|row| {
                row.get_value(0)
                    .ok()
                    .and_then(|value| text_value(&value).ok())
            })
            .is_some_and(|value| value == SEARCH_READY_VERSION);
        let mut mention_rows = connection
            .query(
                "SELECT value FROM miku_index_meta WHERE key = ?1 LIMIT 1",
                [MENTIONS_READY_KEY.to_string()],
            )
            .await
            .map_err(driver_error)?;
        let mentions_ready = mention_rows
            .next()
            .await
            .map_err(driver_error)?
            .and_then(|row| {
                row.get_value(0)
                    .ok()
                    .and_then(|value| text_value(&value).ok())
            })
            .is_some_and(|value| value == "1");

        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            memory: MemoryIndex::new(),
            search_available: Arc::new(AtomicBool::new(search_ready)),
            mentions_ready: Arc::new(AtomicBool::new(mentions_ready)),
            hydrated: Arc::new(OnceCell::new()),
        })
    }

    async fn ensure_hydrated(&self) -> StoreResult<()> {
        let connection = Arc::clone(&self.connection);
        let memory = self.memory.clone();
        self.hydrated
            .get_or_try_init(|| async move {
                let connection = connection.lock().await;
                let mut rows = connection
                    .query("SELECT payload FROM miku_page_index ORDER BY path", ())
                    .await
                    .map_err(driver_error)?;
                let mut pages = Vec::new();
                while let Some(row) = rows.next().await.map_err(driver_error)? {
                    let payload = text_value(&row.get_value(0).map_err(driver_error)?)?;
                    let page = serde_json::from_str::<PageIndex>(&payload).map_err(|error| {
                        StoreError::Operation(format!("decode local page projection: {error}"))
                    })?;
                    pages.push(page);
                }
                memory.replace_pages(pages).await?;
                let mut rows = connection
                    .query(
                        "SELECT target_path, source_path, source_title, matched_text, snippet
                         FROM miku_mention_index ORDER BY target_path, source_path, matched_text",
                        (),
                    )
                    .await
                    .map_err(driver_error)?;
                let mut grouped = std::collections::BTreeMap::<String, Vec<MentionRecord>>::new();
                while let Some(row) = rows.next().await.map_err(driver_error)? {
                    let mention = MentionRecord {
                        target_path: text_value(&row.get_value(0).map_err(driver_error)?)?,
                        source_path: text_value(&row.get_value(1).map_err(driver_error)?)?,
                        source_title: text_value(&row.get_value(2).map_err(driver_error)?)?,
                        matched_text: text_value(&row.get_value(3).map_err(driver_error)?)?,
                        snippet: text_value(&row.get_value(4).map_err(driver_error)?)?,
                    };
                    grouped
                        .entry(mention.source_path.clone())
                        .or_default()
                        .push(mention);
                }
                for (source, mentions) in grouped {
                    memory
                        .replace_mentions_for_source(&source, mentions)
                        .await?;
                }
                Ok(())
            })
            .await
            .map(|_| ())
    }

    async fn persist(&self, page: &PageIndex) -> StoreResult<()> {
        let connection = self.connection.lock().await;
        let mut transaction = connection
            .unchecked_transaction()
            .await
            .map_err(driver_error)?;
        persist_in_transaction(&mut transaction, page).await?;
        transaction.commit().await.map_err(driver_error)?;
        Ok(())
    }
}

async fn persist_in_transaction(
    transaction: &mut Transaction<'_>,
    page: &PageIndex,
) -> StoreResult<()> {
    let payload = serde_json::to_string(page)
        .map_err(|error| StoreError::Operation(format!("encode local page projection: {error}")))?;
    transaction
        .execute(
            "INSERT INTO miku_page_index (path, payload, title, body, mtime)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(path) DO UPDATE SET payload = excluded.payload,
               title = excluded.title, body = excluded.body, mtime = excluded.mtime",
            turso::params![
                page.summary.path.clone(),
                payload,
                page.summary.title.clone(),
                page.body.clone(),
                page.summary.mtime
            ],
        )
        .await
        .map_err(driver_error)?;
    Ok(())
}

fn driver_error(error: impl std::fmt::Display) -> StoreError {
    StoreError::Unavailable(error.to_string())
}

fn text_value(value: &Value) -> StoreResult<String> {
    value
        .as_text()
        .cloned()
        .ok_or_else(|| StoreError::Operation("expected text value from Turso".to_string()))
}

#[async_trait]
impl IndexReader for TursoIndex {
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
        self.ensure_hydrated().await?;
        self.memory.list_pages().await
    }

    async fn page(&self, path: &str) -> StoreResult<Option<PageSummary>> {
        self.ensure_hydrated().await?;
        self.memory.page(path).await
    }

    async fn search(&self, request: SearchRequest) -> StoreResult<Vec<SearchHit>> {
        if request.query.trim().is_empty() || request.limit == 0 {
            return Ok(Vec::new());
        }
        if request.scope == miku_domain::SearchScope::Title {
            self.ensure_hydrated().await?;
            return self.memory.search(request).await;
        }

        if !self.search_available.load(Ordering::Acquire) {
            if self.connection.try_lock().is_err() {
                return self.memory.search(request).await;
            }
            self.ensure_hydrated().await?;
            return self.memory.search(request).await;
        }
        if self.connection.try_lock().is_err() {
            return self.memory.search(request).await;
        }
        self.ensure_hydrated().await?;
        let Ok(connection) = self.connection.try_lock() else {
            // Reconciliation may hold the single Turso connection while
            // Tantivy commits. The HTTP read path must stay responsive; the
            // in-process projection may be briefly stale, but it is safe to
            // serve from it while the durable index is busy.
            return self.memory.search(request).await;
        };
        let mut rows = connection
            .query(
                "SELECT path, title FROM miku_page_index
                 WHERE fts_match(title, body, ?1) LIMIT ?2",
                turso::params![request.query.trim().to_lowercase(), request.limit as i64],
            )
            .await
            .map_err(driver_error)?;
        let mut hits = Vec::new();
        while let Some(row) = rows.next().await.map_err(driver_error)? {
            hits.push(SearchHit {
                path: text_value(&row.get_value(0).map_err(driver_error)?)?,
                title: text_value(&row.get_value(1).map_err(driver_error)?)?,
                snippet: String::new(),
            });
        }
        Ok(hits)
    }

    async fn backlinks(&self, path: &str) -> StoreResult<Vec<Backlink>> {
        self.ensure_hydrated().await?;
        self.memory.backlinks(path).await
    }

    async fn mentions_for_target(&self, path: &str) -> StoreResult<Vec<MentionRecord>> {
        self.ensure_hydrated().await?;
        self.memory.mentions_for_target(path).await
    }

    async fn mentions_ready(&self) -> StoreResult<bool> {
        Ok(self.mentions_ready.load(Ordering::Acquire))
    }

    async fn tags(&self) -> StoreResult<Vec<TagCount>> {
        self.ensure_hydrated().await?;
        self.memory.tags().await
    }

    async fn pages_with_tag(&self, tag: &str) -> StoreResult<Vec<PageSummary>> {
        self.ensure_hydrated().await?;
        self.memory.pages_with_tag(tag).await
    }
}

#[async_trait]
impl IndexWriter for TursoIndex {
    async fn replace_page(&self, page: PageIndex) -> StoreResult<IndexEvent> {
        let path = page.summary.path.clone();
        self.persist(&page).await?;
        self.memory.replace_page(page).await?;
        Ok(IndexEvent::PageIndexed { path })
    }

    async fn replace_pages(&self, pages: Vec<PageIndex>) -> StoreResult<Vec<IndexEvent>> {
        if pages.is_empty() {
            return Ok(Vec::new());
        }
        if pages.len() > 1 && self.search_available.swap(false, Ordering::AcqRel) {
            let connection = self.connection.lock().await;
            connection
                .execute("DROP INDEX IF EXISTS miku_page_search", ())
                .await
                .map_err(driver_error)?;
            connection
                .execute(
                    "INSERT INTO miku_index_meta (key, value) VALUES (?1, '0')
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                    [SEARCH_READY_KEY.to_string()],
                )
                .await
                .map_err(driver_error)?;
        }
        let connection = self.connection.lock().await;
        let mut transaction = connection
            .unchecked_transaction()
            .await
            .map_err(driver_error)?;
        for page in &pages {
            persist_in_transaction(&mut transaction, page).await?;
        }
        transaction.commit().await.map_err(driver_error)?;

        let events = pages
            .iter()
            .map(|page| IndexEvent::PageIndexed {
                path: page.summary.path.clone(),
            })
            .collect();
        self.memory.replace_pages(pages).await?;
        Ok(events)
    }

    async fn rebuild_search_index(&self) -> StoreResult<()> {
        let connection = self.connection.lock().await;
        connection
            .execute(CREATE_SEARCH, ())
            .await
            .map_err(driver_error)?;
        connection
            .execute(
                "INSERT INTO miku_index_meta (key, value) VALUES (?1, '1')
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [SEARCH_READY_KEY.to_string()],
            )
            .await
            .map_err(driver_error)?;
        self.search_available.store(true, Ordering::Release);
        Ok(())
    }

    async fn replace_mentions_for_source(
        &self,
        source_path: &str,
        mentions: Vec<MentionRecord>,
    ) -> StoreResult<()> {
        let connection = self.connection.lock().await;
        let transaction = connection
            .unchecked_transaction()
            .await
            .map_err(driver_error)?;
        transaction
            .execute(
                "DELETE FROM miku_mention_index WHERE source_path = ?1",
                [source_path.to_string()],
            )
            .await
            .map_err(driver_error)?;
        for mention in &mentions {
            transaction
                .execute(
                    "INSERT INTO miku_mention_index
                     (target_path, source_path, source_title, matched_text, snippet)
                     VALUES (?1, ?2, ?3, ?4, ?5)
                     ON CONFLICT(target_path, source_path, matched_text) DO UPDATE SET
                       source_title = excluded.source_title, snippet = excluded.snippet",
                    turso::params![
                        mention.target_path.clone(),
                        mention.source_path.clone(),
                        mention.source_title.clone(),
                        mention.matched_text.clone(),
                        mention.snippet.clone(),
                    ],
                )
                .await
                .map_err(driver_error)?;
        }
        transaction.commit().await.map_err(driver_error)?;
        drop(connection);
        self.memory
            .replace_mentions_for_source(source_path, mentions)
            .await
    }

    async fn replace_mentions_for_sources(
        &self,
        entries: Vec<(String, Vec<MentionRecord>)>,
    ) -> StoreResult<()> {
        let connection = self.connection.lock().await;
        let transaction = connection
            .unchecked_transaction()
            .await
            .map_err(driver_error)?;
        let mut delete_source = transaction
            .prepare("DELETE FROM miku_mention_index WHERE source_path = ?1")
            .await
            .map_err(driver_error)?;
        let mut insert_mention = transaction
            .prepare(
                "INSERT INTO miku_mention_index
                 (target_path, source_path, source_title, matched_text, snippet)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(target_path, source_path, matched_text) DO UPDATE SET
                   source_title = excluded.source_title, snippet = excluded.snippet",
            )
            .await
            .map_err(driver_error)?;
        for (source_path, mentions) in &entries {
            delete_source
                .execute([source_path.clone()])
                .await
                .map_err(driver_error)?;
            for mention in mentions {
                insert_mention
                    .execute(turso::params![
                        mention.target_path.clone(),
                        mention.source_path.clone(),
                        mention.source_title.clone(),
                        mention.matched_text.clone(),
                        mention.snippet.clone(),
                    ])
                    .await
                    .map_err(driver_error)?;
            }
        }
        transaction.commit().await.map_err(driver_error)?;
        drop(connection);
        self.memory.replace_mentions_for_sources(entries).await
    }

    async fn delete_mentions_for_source(&self, source_path: &str) -> StoreResult<()> {
        let connection = self.connection.lock().await;
        connection
            .execute(
                "DELETE FROM miku_mention_index WHERE source_path = ?1",
                [source_path.to_string()],
            )
            .await
            .map_err(driver_error)?;
        drop(connection);
        self.memory.delete_mentions_for_source(source_path).await
    }

    async fn delete_mentions_for_target(&self, target_path: &str) -> StoreResult<()> {
        let connection = self.connection.lock().await;
        connection
            .execute(
                "DELETE FROM miku_mention_index WHERE target_path = ?1",
                [target_path.to_string()],
            )
            .await
            .map_err(driver_error)?;
        drop(connection);
        self.memory.delete_mentions_for_target(target_path).await
    }

    async fn delete_mentions_for_targets(&self, target_paths: Vec<String>) -> StoreResult<()> {
        if target_paths.is_empty() {
            return Ok(());
        }
        let connection = self.connection.lock().await;
        let transaction = connection
            .unchecked_transaction()
            .await
            .map_err(driver_error)?;
        for target_path in &target_paths {
            transaction
                .execute(
                    "DELETE FROM miku_mention_index WHERE target_path = ?1",
                    [target_path.clone()],
                )
                .await
                .map_err(driver_error)?;
        }
        transaction.commit().await.map_err(driver_error)?;
        drop(connection);
        self.memory.delete_mentions_for_targets(target_paths).await
    }

    async fn mark_mentions_ready(&self) -> StoreResult<()> {
        let connection = self.connection.lock().await;
        connection
            .execute(
                "INSERT INTO miku_index_meta (key, value) VALUES (?1, ?2)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                [
                    MENTIONS_READY_KEY.to_string(),
                    MENTIONS_READY_VERSION.to_string(),
                ],
            )
            .await
            .map_err(driver_error)?;
        self.mentions_ready.store(true, Ordering::Release);
        Ok(())
    }

    async fn delete_page(&self, path: &str) -> StoreResult<IndexEvent> {
        let connection = self.connection.lock().await;
        let transaction = connection
            .unchecked_transaction()
            .await
            .map_err(driver_error)?;
        transaction
            .execute(
                "DELETE FROM miku_page_index WHERE path = ?1",
                [path.to_string()],
            )
            .await
            .map_err(driver_error)?;
        transaction.commit().await.map_err(driver_error)?;
        drop(connection);
        self.delete_mentions_for_source(path).await?;
        self.delete_mentions_for_target(path).await?;
        self.memory.delete_page(path).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use miku_domain::{IndexReader, IndexWriter, PageIndex, PageSummary};

    fn page(path: &str, body: &str) -> PageIndex {
        PageIndex {
            summary: PageSummary {
                path: path.to_string(),
                title: path.trim_end_matches(".md").to_string(),
                frontmatter: serde_json::json!({}),
                mtime: 1,
            },
            body: body.to_string(),
            links: Vec::new(),
            tags: Vec::new(),
            aliases: Vec::new(),
            has_mermaid: false,
            signals: Default::default(),
        }
    }

    #[tokio::test]
    async fn persists_and_searches_a_projection() {
        let store = TursoIndex::open(":memory:")
            .await
            .expect("open local index");
        store
            .replace_page(page("Today.md", "Miku note"))
            .await
            .expect("write projection");
        let hits = store
            .search(SearchRequest {
                query: "note".to_string(),
                scope: miku_domain::SearchScope::Body,
                limit: 10,
            })
            .await
            .expect("search local index");
        assert_eq!(hits.len(), 1);
    }

    #[tokio::test]
    async fn accepts_title_case_terms_and_serializes_concurrent_driver_access() {
        let store = TursoIndex::open(":memory:")
            .await
            .expect("open local index");
        let pages = vec![page("Miku.md", "A note about Miku")];
        let (write, read) = tokio::join!(
            store.replace_pages(pages),
            store.search(SearchRequest {
                query: "Miku".to_string(),
                scope: miku_domain::SearchScope::Body,
                limit: 10,
            })
        );
        write.expect("concurrent write");
        read.expect("concurrent read");
    }

    #[tokio::test]
    async fn search_falls_back_without_waiting_for_a_durable_write() {
        let store = TursoIndex::open(":memory:")
            .await
            .expect("open local index");
        store
            .replace_page(page("Miku.md", "A note about Miku"))
            .await
            .expect("write projection");

        let _connection = store.connection.lock().await;
        let hits = store
            .search(SearchRequest {
                query: "Miku".to_string(),
                scope: miku_domain::SearchScope::Body,
                limit: 10,
            })
            .await
            .expect("fallback search");
        assert_eq!(hits.len(), 1);
    }

    #[tokio::test]
    async fn bulk_projection_rebuilds_search_once_after_writes() {
        let store = TursoIndex::open(":memory:")
            .await
            .expect("open local index");
        store
            .replace_pages(vec![
                page("First.md", "bulk alpha"),
                page("Second.md", "bulk beta"),
            ])
            .await
            .expect("write bulk projection");

        let fallback_hits = store
            .search(SearchRequest {
                query: "bulk".to_string(),
                scope: miku_domain::SearchScope::Body,
                limit: 10,
            })
            .await
            .expect("search in-memory bulk projection");
        assert_eq!(fallback_hits.len(), 2);

        store
            .rebuild_search_index()
            .await
            .expect("rebuild durable search index");
        let durable_hits = store
            .search(SearchRequest {
                query: "alpha".to_string(),
                scope: miku_domain::SearchScope::Body,
                limit: 10,
            })
            .await
            .expect("search rebuilt index");
        assert_eq!(durable_hits.len(), 1);
    }

    #[tokio::test]
    async fn reopens_and_resumes_a_partial_disk_projection() {
        let path = std::env::temp_dir().join(format!(
            "miku-turso-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        let path_string = path.to_string_lossy().into_owned();
        {
            let store = TursoIndex::open(&path_string)
                .await
                .expect("open disk index");
            store
                .replace_page(page("Today.md", "A durable note"))
                .await
                .expect("write first projection");
        }
        {
            let reopened = TursoIndex::open(&path_string)
                .await
                .expect("reopen disk index");
            assert_eq!(reopened.list_pages().await.expect("list pages").len(), 1);
            reopened
                .replace_page(page("Next.md", "Another note"))
                .await
                .expect("resume with second projection");
        }
        let resumed = TursoIndex::open(&path_string)
            .await
            .expect("reopen resumed index");
        assert_eq!(resumed.list_pages().await.expect("list pages").len(), 2);
        let hits = resumed
            .search(SearchRequest {
                query: "durable".to_string(),
                scope: miku_domain::SearchScope::Body,
                limit: 10,
            })
            .await
            .expect("search reopened index");
        assert_eq!(hits.len(), 1);
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test]
    async fn rehydrates_derived_mentions_after_restart() {
        let path = std::env::temp_dir().join(format!(
            "miku-turso-mentions-{}-{}.db",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock")
                .as_nanos()
        ));
        let path_string = path.to_string_lossy().into_owned();
        {
            let store = TursoIndex::open(&path_string)
                .await
                .expect("open disk index");
            store
                .replace_page(page("Index.md", "Target"))
                .await
                .expect("write target");
            store
                .replace_page(page("Source.md", "Index"))
                .await
                .expect("write source");
            store
                .replace_mentions_for_source(
                    "Source.md",
                    vec![MentionRecord {
                        target_path: "Index.md".to_string(),
                        source_path: "Source.md".to_string(),
                        source_title: "Source".to_string(),
                        matched_text: "Index".to_string(),
                        snippet: "Index".to_string(),
                    }],
                )
                .await
                .expect("write mention");
        }
        let reopened = TursoIndex::open(&path_string)
            .await
            .expect("reopen disk index");
        assert_eq!(
            reopened
                .mentions_for_target("Index.md")
                .await
                .expect("read mention")
                .len(),
            1
        );
        reopened
            .delete_mentions_for_target("Index.md")
            .await
            .expect("delete target mentions");
        assert!(reopened
            .mentions_for_target("Index.md")
            .await
            .expect("read deleted mention")
            .is_empty());
        let _ = std::fs::remove_file(path);
    }
}
