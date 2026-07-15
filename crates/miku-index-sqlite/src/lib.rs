//! SQLite implementation of Miku's backend-neutral index contract.

use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;
use std::time::Duration;

use miku_domain::{
    Backlink, IndexCapabilities, IndexEvent, IndexReader, IndexWriter, MentionRecord, PageIndex,
    PageSummary, SearchHit, SearchRequest, StoreError, StoreResult, TagCount,
};

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
        todo!("list_pages")
    }

    async fn page(&self, _path: &str) -> StoreResult<Option<PageSummary>> {
        todo!("page")
    }

    async fn search(&self, _request: SearchRequest) -> StoreResult<Vec<SearchHit>> {
        todo!("search")
    }

    async fn backlinks(&self, _path: &str) -> StoreResult<Vec<Backlink>> {
        todo!("backlinks")
    }

    async fn mentions_for_target(&self, _path: &str) -> StoreResult<Vec<MentionRecord>> {
        todo!("mentions_for_target")
    }

    async fn mentions_ready(&self) -> StoreResult<bool> {
        todo!("mentions_ready")
    }

    async fn tags(&self) -> StoreResult<Vec<TagCount>> {
        todo!("tags")
    }

    async fn pages_with_tag(&self, _tag: &str) -> StoreResult<Vec<PageSummary>> {
        todo!("pages_with_tag")
    }
}

#[async_trait]
impl IndexWriter for SqliteIndex {
    async fn replace_page(&self, _page: PageIndex) -> StoreResult<IndexEvent> {
        todo!("replace_page")
    }

    async fn delete_page(&self, _path: &str) -> StoreResult<IndexEvent> {
        todo!("delete_page")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

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
}
