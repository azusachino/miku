//! Durable local index backed by the Rust-built Turso database engine.

use async_trait::async_trait;
use miku_domain::{
    Backlink, IndexCapabilities, IndexEvent, IndexReader, IndexWriter, PageIndex, PageSummary,
    SearchHit, SearchRequest, StoreError, StoreResult, TagCount, UnlinkedMention,
};
use miku_index_memory::MemoryIndex;
use std::sync::Arc;
use tokio::sync::Mutex;
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

/// A local durable index using the Rust-built Turso engine.
#[derive(Clone)]
pub struct TursoIndex {
    connection: Arc<Mutex<Connection>>,
    memory: MemoryIndex,
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
            .execute(CREATE_SEARCH, ())
            .await
            .map_err(driver_error)?;

        let memory = MemoryIndex::new();
        let mut rows = connection
            .query("SELECT payload FROM miku_page_index ORDER BY path", ())
            .await
            .map_err(driver_error)?;
        while let Some(row) = rows.next().await.map_err(driver_error)? {
            let payload = text_value(&row.get_value(0).map_err(driver_error)?)?;
            let page = serde_json::from_str::<PageIndex>(&payload).map_err(|error| {
                StoreError::Operation(format!("decode local page projection: {error}"))
            })?;
            memory.replace_page(page).await?;
        }

        Ok(Self {
            connection: Arc::new(Mutex::new(connection)),
            memory,
        })
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
        self.memory.list_pages().await
    }

    async fn page(&self, path: &str) -> StoreResult<Option<PageSummary>> {
        self.memory.page(path).await
    }

    async fn search(&self, request: SearchRequest) -> StoreResult<Vec<SearchHit>> {
        if request.query.trim().is_empty() || request.limit == 0 {
            return Ok(Vec::new());
        }
        if request.scope == miku_domain::SearchScope::Title {
            return self.memory.search(request).await;
        }

        let connection = self.connection.lock().await;
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
        self.memory.backlinks(path).await
    }

    async fn unlinked_mentions(&self, path: &str) -> StoreResult<Vec<UnlinkedMention>> {
        self.memory.unlinked_mentions(path).await
    }

    async fn tags(&self) -> StoreResult<Vec<TagCount>> {
        self.memory.tags().await
    }

    async fn pages_with_tag(&self, tag: &str) -> StoreResult<Vec<PageSummary>> {
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
        for page in pages {
            self.memory.replace_page(page).await?;
        }
        Ok(events)
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
    async fn commits_batches_and_reopens_a_disk_database() {
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
                .replace_pages(vec![
                    page("Today.md", "A durable note"),
                    page("Next.md", "Another note"),
                ])
                .await
                .expect("write batch");
        }
        let reopened = TursoIndex::open(&path_string)
            .await
            .expect("reopen disk index");
        assert_eq!(reopened.list_pages().await.expect("list pages").len(), 2);
        let hits = reopened
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
}
