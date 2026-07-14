//! Application-facing index API.
//!
//! HTTP handlers should depend on [`IndexApi`] rather than a database driver.
//! The indexer can retain the writer half while read-only routes retain the
//! reader half, which keeps backend selection and cache composition outside
//! the request layer.

use miku_domain::{
    Backlink, IndexCapabilities, IndexEvent, IndexReader, IndexStore, IndexWriter, MentionRecord,
    PageIndex, PageSummary, SearchHit, SearchRequest, StoreResult, TagCount,
};
use std::sync::Arc;
#[cfg(feature = "valkey")]
use tokio::sync::Mutex;

/// Explicitly selected deployment tier and primary index.
#[derive(Debug, Clone)]
pub enum RuntimeConfig {
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

/// Resolve the runtime selected by the process environment.
pub fn resolve_runtime() -> StoreResult<RuntimeConfig> {
    let backend = std::env::var("MIKU_INDEX_BACKEND").unwrap_or_else(|_| "turso".to_string());
    let runtime = match backend.as_str() {
        "memory" => RuntimeConfig::Memory,
        "turso" => RuntimeConfig::Turso {
            path: std::env::var("MIKU_INDEX_PATH")
                .unwrap_or_else(|_| "miku_docs/.miku-index.turso".to_string()),
        },
        "postgres" => RuntimeConfig::Postgres {
            database_url: required_env("DATABASE_URL")?,
        },
        "postgres-valkey" => {
            #[cfg(all(feature = "postgres", feature = "valkey"))]
            {
                RuntimeConfig::PostgresValkey {
                    database_url: required_env("DATABASE_URL")?,
                    valkey_url: required_env("VALKEY_URL")?,
                    namespace: std::env::var("MIKU_VALKEY_NAMESPACE")
                        .unwrap_or_else(|_| "miku".to_string()),
                }
            }
            #[cfg(not(all(feature = "postgres", feature = "valkey")))]
            {
                return Err(missing_feature("Postgres + Valkey", "postgres,valkey"));
            }
        },
        other => {
            return Err(miku_domain::StoreError::Unsupported(format!(
                "MIKU_INDEX_BACKEND must be `memory`, `turso`, `postgres`, or `postgres-valkey`; got {other}"
            )))
        }
    };

    if runtime_enabled(&runtime) {
        Ok(runtime)
    } else {
        Err(missing_feature(
            runtime_name(&runtime),
            runtime_feature(&runtime),
        ))
    }
}

fn required_env(name: &str) -> StoreResult<String> {
    std::env::var(name).map_err(|_| {
        miku_domain::StoreError::Unavailable(format!("{name} is required for the selected runtime"))
    })
}

const fn runtime_name(runtime: &RuntimeConfig) -> &'static str {
    match runtime {
        RuntimeConfig::Memory => "memory",
        RuntimeConfig::Turso { .. } => "Turso",
        RuntimeConfig::Postgres { .. } => "Postgres",
        #[cfg(feature = "valkey")]
        RuntimeConfig::PostgresValkey { .. } => "Postgres + Valkey",
    }
}

const fn runtime_feature(runtime: &RuntimeConfig) -> &'static str {
    match runtime {
        RuntimeConfig::Memory => "memory",
        RuntimeConfig::Turso { .. } => "turso",
        RuntimeConfig::Postgres { .. } => "postgres",
        #[cfg(feature = "valkey")]
        RuntimeConfig::PostgresValkey { .. } => "postgres,valkey",
    }
}

const fn runtime_enabled(runtime: &RuntimeConfig) -> bool {
    match runtime {
        RuntimeConfig::Memory => cfg!(feature = "memory"),
        RuntimeConfig::Turso { .. } => cfg!(feature = "turso"),
        RuntimeConfig::Postgres { .. } => cfg!(feature = "postgres"),
        #[cfg(feature = "valkey")]
        RuntimeConfig::PostgresValkey { .. } => cfg!(all(feature = "postgres", feature = "valkey")),
    }
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
    pub async fn mentions_for_target(&self, path: &str) -> StoreResult<Vec<MentionRecord>> {
        let mentions = self.reader.mentions_for_target(path).await?;
        if mentions.is_empty() && !path.ends_with(".md") {
            return self.reader.mentions_for_target(&format!("{path}.md")).await;
        }
        Ok(mentions)
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
pub async fn compose_index(config: RuntimeConfig) -> StoreResult<IndexApi> {
    match config {
        RuntimeConfig::Memory => {
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
        RuntimeConfig::Turso { path } => {
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
        RuntimeConfig::Postgres { database_url } => {
            #[cfg(feature = "postgres")]
            {
                let pool = connect_postgres(&database_url).await?;
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
        RuntimeConfig::PostgresValkey {
            database_url,
            valkey_url,
            namespace,
        } => {
            #[cfg(feature = "postgres")]
            {
                let pool = connect_postgres(&database_url).await?;
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

#[cfg(feature = "postgres")]
async fn connect_postgres(database_url: &str) -> StoreResult<sqlx::PgPool> {
    let pool = sqlx::PgPool::connect(database_url)
        .await
        .map_err(|error| miku_domain::StoreError::Unavailable(error.to_string()))?;
    sqlx::migrate!("../miku-index-postgres/migrations")
        .run(&pool)
        .await
        .map_err(|error| miku_domain::StoreError::Unavailable(error.to_string()))?;
    Ok(pool)
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

    async fn mentions_for_target(&self, path: &str) -> StoreResult<Vec<MentionRecord>> {
        self.read_through(
            format!("mentions_for_target:{path}"),
            self.primary.mentions_for_target(path),
        )
        .await
    }

    async fn mentions_ready(&self) -> StoreResult<bool> {
        self.primary.mentions_ready().await
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

    async fn replace_mentions_for_source(
        &self,
        source_path: &str,
        mentions: Vec<MentionRecord>,
    ) -> StoreResult<()> {
        let result = self
            .primary
            .replace_mentions_for_source(source_path, mentions)
            .await;
        let _ = self.cache.lock().await.clear().await;
        result
    }

    async fn delete_mentions_for_source(&self, source_path: &str) -> StoreResult<()> {
        let result = self.primary.delete_mentions_for_source(source_path).await;
        let _ = self.cache.lock().await.clear().await;
        result
    }

    async fn delete_mentions_for_target(&self, target_path: &str) -> StoreResult<()> {
        let result = self.primary.delete_mentions_for_target(target_path).await;
        let _ = self.cache.lock().await.clear().await;
        result
    }

    async fn mark_mentions_ready(&self) -> StoreResult<()> {
        let result = self.primary.mark_mentions_ready().await;
        let _ = self.cache.lock().await.clear().await;
        result
    }
}

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
            signals: Default::default(),
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
