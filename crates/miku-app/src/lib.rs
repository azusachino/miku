//! Application-facing index API.
//!
//! HTTP handlers should depend on [`IndexApi`] rather than a database driver.
//! The indexer can retain the writer half while read-only routes retain the
//! reader half, which keeps backend selection and cache composition outside
//! the request layer.

use miku_domain::{
    Backlink, IndexCapabilities, IndexEvent, IndexReader, IndexStore, IndexWriter, PageIndex,
    PageSummary, SearchHit, SearchRequest, StoreResult, TagCount, UnlinkedMention,
};
use std::sync::Arc;
#[cfg(feature = "valkey")]
use tokio::sync::Mutex;

/// Explicitly selected deployment tier and primary index.
#[derive(Debug, Clone)]
pub enum IndexConfig {
    /// Disposable in-process backend for tests and temporary runs.
    Memory,
    /// Durable local Turso backend.
    Turso { path: String },
    /// Durable Postgres backend for the scale tier.
    Postgres { database_url: String },
    /// Postgres primary with an optional Valkey read-through cache.
    #[cfg(feature = "valkey")]
    PostgresValkey {
        database_url: String,
        valkey_url: String,
        namespace: String,
    },
}

/// Backend-neutral operations consumed by Miku's HTTP and indexing layers.
#[derive(Clone)]
pub struct IndexApi {
    reader: Arc<dyn IndexReader>,
    writer: Arc<dyn IndexWriter>,
}

impl IndexApi {
    /// Compose the API from one concrete backend.
    pub fn from_store<S>(store: Arc<S>) -> Self
    where
        S: IndexStore + 'static,
    {
        Self {
            reader: store.clone(),
            writer: store,
        }
    }

    /// Compose a read path and write path independently.
    pub fn from_parts(reader: Arc<dyn IndexReader>, writer: Arc<dyn IndexWriter>) -> Self {
        Self { reader, writer }
    }

    /// Expose the mutation half to the filesystem indexer.
    #[must_use]
    pub fn writer(&self) -> Arc<dyn IndexWriter> {
        Arc::clone(&self.writer)
    }

    /// Expose the read half to the filesystem reconciler.
    #[must_use]
    pub fn reader(&self) -> Arc<dyn IndexReader> {
        Arc::clone(&self.reader)
    }

    /// Return backend capabilities for health/configuration reporting.
    pub async fn capabilities(&self) -> StoreResult<IndexCapabilities> {
        self.reader.capabilities().await
    }

    /// List indexed pages in backend-defined deterministic order.
    pub async fn list_pages(&self) -> StoreResult<Vec<PageSummary>> {
        self.reader.list_pages().await
    }

    /// Load one page summary.
    pub async fn page(&self, path: &str) -> StoreResult<Option<PageSummary>> {
        self.reader.page(path).await
    }

    /// Search indexed content.
    pub async fn search(&self, request: SearchRequest) -> StoreResult<Vec<SearchHit>> {
        self.reader.search(request).await
    }

    /// Find pages linking to the requested page.
    pub async fn backlinks(&self, path: &str) -> StoreResult<Vec<Backlink>> {
        self.reader.backlinks(path).await
    }

    /// Find plain-text mentions of the requested page.
    pub async fn unlinked_mentions(&self, path: &str) -> StoreResult<Vec<UnlinkedMention>> {
        self.reader.unlinked_mentions(path).await
    }

    /// Return tag counts.
    pub async fn tags(&self) -> StoreResult<Vec<TagCount>> {
        self.reader.tags().await
    }

    /// List pages carrying one exact normalized tag.
    pub async fn pages_with_tag(&self, tag: &str) -> StoreResult<Vec<PageSummary>> {
        self.reader.pages_with_tag(tag).await
    }

    /// Atomically replace one indexed page projection.
    pub async fn replace_page(&self, page: PageIndex) -> StoreResult<IndexEvent> {
        self.writer.replace_page(page).await
    }

    /// Delete one indexed page projection.
    pub async fn delete_page(&self, path: &str) -> StoreResult<IndexEvent> {
        self.writer.delete_page(path).await
    }
}

/// Compose one backend without exposing driver types to routes.
pub async fn compose_index(config: IndexConfig) -> StoreResult<IndexApi> {
    match config {
        IndexConfig::Memory => {
            #[cfg(feature = "memory")]
            {
                Ok(IndexApi::from_store(Arc::new(
                    miku_index_memory::MemoryIndex::new(),
                )))
            }
            #[cfg(not(feature = "memory"))]
            {
                Err(missing_feature("memory", "memory"))
            }
        }
        IndexConfig::Turso { path } => {
            #[cfg(feature = "turso")]
            {
                let store = miku_index_turso::TursoIndex::open(&path).await?;
                Ok(IndexApi::from_store(Arc::new(store)))
            }
            #[cfg(not(feature = "turso"))]
            {
                let _ = path;
                Err(missing_feature("Turso", "turso"))
            }
        }
        IndexConfig::Postgres { database_url } => {
            #[cfg(feature = "postgres")]
            {
                let pool = sqlx::PgPool::connect(&database_url)
                    .await
                    .map_err(|error| miku_domain::StoreError::Unavailable(error.to_string()))?;
                Ok(IndexApi::from_store(Arc::new(
                    miku_index_postgres::PostgresIndex::new(pool),
                )))
            }
            #[cfg(not(feature = "postgres"))]
            {
                let _ = database_url;
                Err(missing_feature("Postgres", "postgres"))
            }
        }
        #[cfg(feature = "valkey")]
        IndexConfig::PostgresValkey {
            database_url,
            valkey_url,
            namespace,
        } => {
            #[cfg(feature = "postgres")]
            {
                let pool = sqlx::PgPool::connect(&database_url)
                    .await
                    .map_err(|error| miku_domain::StoreError::Unavailable(error.to_string()))?;
                let primary: Arc<dyn IndexStore> =
                    Arc::new(miku_index_postgres::PostgresIndex::new(pool));
                let cache = miku_cache_valkey::ValkeyCache::connect(&valkey_url, namespace)
                    .await
                    .map_err(|error| miku_domain::StoreError::Unavailable(error.to_string()))?;
                let cache = Arc::new(Mutex::new(cache));
                return Ok(IndexApi::from_parts(
                    Arc::new(CachedIndexReader {
                        primary: primary.clone(),
                        cache: cache.clone(),
                    }),
                    Arc::new(CachedIndexWriter { primary, cache }),
                ));
            }
            #[cfg(not(feature = "postgres"))]
            {
                let _ = (database_url, valkey_url, namespace);
                Err(missing_feature("Postgres", "postgres"))
            }
        }
    }
}

#[cfg(feature = "valkey")]
struct CachedIndexReader {
    primary: Arc<dyn IndexReader>,
    cache: Arc<Mutex<miku_cache_valkey::ValkeyCache>>,
}

#[cfg(feature = "valkey")]
impl CachedIndexReader {
    async fn read_through<T, F>(&self, key: String, load: F) -> StoreResult<T>
    where
        T: serde::Serialize + serde::de::DeserializeOwned,
        F: std::future::Future<Output = StoreResult<T>>,
    {
        if let Ok(Some(value)) = self.cache.lock().await.get_json(&key).await {
            return Ok(value);
        }
        let value = load.await?;
        let _ = self.cache.lock().await.set_json(&key, &value, 60).await;
        Ok(value)
    }
}

#[cfg(feature = "valkey")]
#[async_trait::async_trait]
impl IndexReader for CachedIndexReader {
    async fn capabilities(&self) -> StoreResult<IndexCapabilities> {
        self.primary.capabilities().await
    }

    async fn list_pages(&self) -> StoreResult<Vec<PageSummary>> {
        self.read_through("list_pages".to_string(), self.primary.list_pages())
            .await
    }

    async fn page(&self, path: &str) -> StoreResult<Option<PageSummary>> {
        self.read_through(format!("page:{path}"), self.primary.page(path))
            .await
    }

    async fn search(&self, request: SearchRequest) -> StoreResult<Vec<SearchHit>> {
        let key = format!(
            "search:{}",
            serde_json::to_string(&request).unwrap_or_default()
        );
        self.read_through(key, self.primary.search(request)).await
    }

    async fn backlinks(&self, path: &str) -> StoreResult<Vec<Backlink>> {
        self.read_through(format!("backlinks:{path}"), self.primary.backlinks(path))
            .await
    }

    async fn unlinked_mentions(&self, path: &str) -> StoreResult<Vec<UnlinkedMention>> {
        self.read_through(
            format!("unlinked_mentions:{path}"),
            self.primary.unlinked_mentions(path),
        )
        .await
    }

    async fn tags(&self) -> StoreResult<Vec<TagCount>> {
        self.read_through("tags".to_string(), self.primary.tags())
            .await
    }

    async fn pages_with_tag(&self, tag: &str) -> StoreResult<Vec<PageSummary>> {
        self.read_through(format!("tag:{tag}"), self.primary.pages_with_tag(tag))
            .await
    }
}

#[cfg(feature = "valkey")]
struct CachedIndexWriter {
    primary: Arc<dyn IndexStore>,
    cache: Arc<Mutex<miku_cache_valkey::ValkeyCache>>,
}

#[cfg(feature = "valkey")]
#[async_trait::async_trait]
impl IndexWriter for CachedIndexWriter {
    async fn replace_page(&self, page: PageIndex) -> StoreResult<IndexEvent> {
        let result = self.primary.replace_page(page).await?;
        let _ = self.cache.lock().await.clear().await;
        Ok(result)
    }

    async fn delete_page(&self, path: &str) -> StoreResult<IndexEvent> {
        let result = self.primary.delete_page(path).await?;
        let _ = self.cache.lock().await.clear().await;
        Ok(result)
    }
}

#[cfg(any(
    not(feature = "memory"),
    not(feature = "turso"),
    not(feature = "postgres")
))]
fn missing_feature(backend: &str, feature: &str) -> miku_domain::StoreError {
    miku_domain::StoreError::Unsupported(format!(
        "{backend} backend requires the `{feature}` Cargo feature"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use miku_domain::{PageIndex, PageSummary, SearchScope};
    use miku_index_memory::MemoryIndex;

    #[tokio::test]
    async fn facade_composes_reader_and_writer() {
        let store = Arc::new(MemoryIndex::new());
        let api = IndexApi::from_store(store);
        api.replace_page(PageIndex {
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
        .expect("write page");

        let hits = api
            .search(SearchRequest {
                query: "note".to_string(),
                scope: SearchScope::Body,
                limit: 10,
            })
            .await
            .expect("search pages");
        assert_eq!(hits.len(), 1);
    }
}
