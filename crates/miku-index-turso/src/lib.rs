//! Durable local Turso/SQLite-compatible implementation of Miku's index contract.

use async_trait::async_trait;
use miku_domain::{
    Backlink, IndexCapabilities, IndexEvent, IndexReader, IndexWriter, PageIndex, PageSummary,
    SearchHit, SearchRequest, StoreError, StoreResult, TagCount, UnlinkedMention,
};
use miku_index_memory::MemoryIndex;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;

const SCHEMA: &str = "CREATE TABLE IF NOT EXISTS miku_page_index (
    path TEXT PRIMARY KEY NOT NULL,
    payload TEXT NOT NULL,
    mtime INTEGER NOT NULL
);";

/// A local durable index using a SQLite-compatible driver.
#[derive(Clone)]
pub struct TursoIndex {
    connection: SqlitePool,
    memory: MemoryIndex,
}

impl TursoIndex {
    /// Open a local database and load projections into the read model.
    pub async fn open(path: &str) -> StoreResult<Self> {
        let options = if path == ":memory:" {
            SqliteConnectOptions::from_str("sqlite::memory:")
        } else {
            Ok(SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true)
                .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
                .busy_timeout(std::time::Duration::from_secs(5)))
        }
        .map_err(driver_error)?;
        let connection = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await
            .map_err(driver_error)?;
        sqlx::query(SCHEMA)
            .execute(&connection)
            .await
            .map_err(driver_error)?;

        let memory = MemoryIndex::new();
        let rows =
            sqlx::query_as::<_, (String,)>("SELECT payload FROM miku_page_index ORDER BY path")
                .fetch_all(&connection)
                .await
                .map_err(driver_error)?;
        for (payload,) in rows {
            let page = serde_json::from_str::<PageIndex>(&payload).map_err(|error| {
                StoreError::Operation(format!("decode local page projection: {error}"))
            })?;
            memory.replace_page(page).await?;
        }

        Ok(Self { connection, memory })
    }

    async fn persist(&self, page: &PageIndex) -> StoreResult<()> {
        let payload = serde_json::to_string(page).map_err(|error| {
            StoreError::Operation(format!("encode local page projection: {error}"))
        })?;
        sqlx::query(
            "INSERT INTO miku_page_index (path, payload, mtime) VALUES (?1, ?2, ?3)
             ON CONFLICT(path) DO UPDATE SET payload = excluded.payload, mtime = excluded.mtime",
        )
        .bind(&page.summary.path)
        .bind(payload)
        .bind(page.summary.mtime)
        .execute(&self.connection)
        .await
        .map_err(driver_error)?;
        Ok(())
    }
}

fn driver_error(error: impl std::fmt::Display) -> StoreError {
    StoreError::Unavailable(error.to_string())
}

#[async_trait]
impl IndexReader for TursoIndex {
    async fn capabilities(&self) -> StoreResult<IndexCapabilities> {
        Ok(IndexCapabilities {
            durable: true,
            full_text_search: false,
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
        self.memory.search(request).await
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

    async fn delete_page(&self, path: &str) -> StoreResult<IndexEvent> {
        sqlx::query("DELETE FROM miku_page_index WHERE path = ?1")
            .bind(path)
            .execute(&self.connection)
            .await
            .map_err(driver_error)?;
        self.memory.delete_page(path).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use miku_domain::{IndexReader, IndexWriter, PageIndex, PageSummary};

    #[tokio::test]
    async fn persists_a_projection_in_a_local_database() {
        let store = TursoIndex::open(":memory:")
            .await
            .expect("open local index");
        store
            .replace_page(PageIndex {
                summary: PageSummary {
                    path: "Today.md".to_string(),
                    title: "Today".to_string(),
                    frontmatter: serde_json::json!({}),
                    mtime: 1,
                },
                body: "A note".to_string(),
                links: Vec::new(),
                tags: Vec::new(),
                aliases: Vec::new(),
                has_mermaid: false,
            })
            .await
            .expect("write projection");
        assert_eq!(store.list_pages().await.expect("list pages").len(), 1);
    }
}
