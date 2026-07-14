//! Optional Valkey cache primitives.
//!
//! Valkey is deliberately a cache boundary, not an [`IndexStore`]. Callers
//! decide whether a cache miss reads the authoritative backend and must treat
//! every cache operation as best-effort.

use redis::aio::ConnectionManager;

/// A namespaced asynchronous Valkey cache client.
#[derive(Clone)]
pub struct ValkeyCache {
    connection: ConnectionManager,
    namespace: String,
}

impl ValkeyCache {
    /// Connect to Valkey without hiding connection failures.
    pub async fn connect(url: &str, namespace: impl Into<String>) -> redis::RedisResult<Self> {
        let client = redis::Client::open(url)?;
        let connection = client.get_connection_manager().await?;
        Ok(Self {
            connection,
            namespace: namespace.into(),
        })
    }

    fn key(&self, key: &str) -> String {
        format!("{}:{key}", self.namespace)
    }

    /// Read a cached JSON value. A missing key is represented by `None`.
    pub async fn get_json<T>(&mut self, key: &str) -> redis::RedisResult<Option<T>>
    where
        T: serde::de::DeserializeOwned,
    {
        let value: Option<String> = redis::cmd("GET")
            .arg(self.key(key))
            .query_async(&mut self.connection)
            .await?;
        value
            .map(|payload| {
                serde_json::from_str(&payload).map_err(|error| {
                    redis::RedisError::from((
                        redis::ErrorKind::TypeError,
                        "invalid cached JSON",
                        error.to_string(),
                    ))
                })
            })
            .transpose()
    }

    /// Store a JSON value for a bounded lifetime.
    pub async fn set_json<T>(
        &mut self,
        key: &str,
        value: &T,
        ttl_seconds: u64,
    ) -> redis::RedisResult<()>
    where
        T: serde::Serialize,
    {
        let payload = serde_json::to_string(value).map_err(|error| {
            redis::RedisError::from((
                redis::ErrorKind::TypeError,
                "failed to encode cached JSON",
                error.to_string(),
            ))
        })?;
        redis::cmd("SETEX")
            .arg(self.key(key))
            .arg(ttl_seconds)
            .arg(payload)
            .query_async::<()>(&mut self.connection)
            .await
    }

    /// Invalidate one cache entry.
    pub async fn delete(&mut self, key: &str) -> redis::RedisResult<()> {
        redis::cmd("DEL")
            .arg(self.key(key))
            .query_async::<()>(&mut self.connection)
            .await
    }

    /// Invalidate every entry owned by this cache namespace.
    pub async fn clear(&mut self) -> redis::RedisResult<()> {
        let pattern = format!("{}:*", self.namespace);
        let mut cursor = 0_u64;
        loop {
            let (next, keys): (u64, Vec<String>) = redis::cmd("SCAN")
                .arg(cursor)
                .arg("MATCH")
                .arg(&pattern)
                .arg("COUNT")
                .arg(128)
                .query_async(&mut self.connection)
                .await?;
            if !keys.is_empty() {
                redis::cmd("DEL")
                    .arg(keys)
                    .query_async::<()>(&mut self.connection)
                    .await?;
            }
            cursor = next;
            if cursor == 0 {
                return Ok(());
            }
        }
    }
}
