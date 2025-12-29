use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use lru::LruCache;
use serde_json::Value;
use std::num::NonZeroUsize;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::config::CacheSettings;
use crate::repositories::{CacheRepo, RedisRepo};

#[derive(Debug, Clone, Copy)]
pub enum CacheSource {
    Memory,
    Redis,
    Sqlite,
}

impl CacheSource {
    pub fn as_str(&self) -> &'static str {
        match self {
            CacheSource::Memory => "memory",
            CacheSource::Redis => "redis",
            CacheSource::Sqlite => "sqlite",
        }
    }
}

#[derive(Debug, Clone)]
struct MemoryEntry {
    value: Value,
    expires_at: DateTime<Utc>,
}

#[derive(Debug)]
pub struct CacheStats {
    pub total_requests: AtomicU64,
    pub memory_hits: AtomicU64,
    pub redis_hits: AtomicU64,
    pub sqlite_hits: AtomicU64,
}

impl CacheStats {
    pub fn new() -> Self {
        Self {
            total_requests: AtomicU64::new(0),
            memory_hits: AtomicU64::new(0),
            redis_hits: AtomicU64::new(0),
            sqlite_hits: AtomicU64::new(0),
        }
    }
}

#[derive(Clone)]
pub struct CacheService {
    settings: CacheSettings,
    memory_cache: Arc<Mutex<LruCache<String, MemoryEntry>>>,
    redis_repo: Option<RedisRepo>,
    sqlite_repo: Option<CacheRepo>,
    stats: Arc<CacheStats>,
}

impl CacheService {
    pub async fn new(settings: CacheSettings) -> Result<Self> {
        let memory_capacity = NonZeroUsize::new(settings.memory_cache_entries.max(1))
            .unwrap_or_else(|| NonZeroUsize::new(1).unwrap());
        let memory_cache = Arc::new(Mutex::new(LruCache::new(memory_capacity)));

        let redis_repo = if settings.redis_url.trim().is_empty() {
            None
        } else {
            RedisRepo::new(&settings.redis_url, settings.redis_ttl_seconds)
                .await
                .ok()
        };

        let sqlite_repo = if settings.sqlite_path.trim().is_empty() {
            None
        } else {
            CacheRepo::new(
                settings.sqlite_path.clone(),
                settings.sqlite_ttl_days,
                settings.sqlite_max_size_gb,
            )
            .ok()
        };

        Ok(Self {
            settings,
            memory_cache,
            redis_repo,
            sqlite_repo,
            stats: Arc::new(CacheStats::new()),
        })
    }

    pub fn stats(&self) -> Arc<CacheStats> {
        self.stats.clone()
    }

    pub async fn get(&self, key: &str) -> Option<(Value, CacheSource)> {
        self.stats.total_requests.fetch_add(1, Ordering::Relaxed);

        if let Some(value) = self.get_from_memory(key).await {
            self.stats.memory_hits.fetch_add(1, Ordering::Relaxed);
            return Some((value, CacheSource::Memory));
        }

        if let Some(redis_repo) = &self.redis_repo {
            if let Ok(Some(value)) = redis_repo.get(key).await {
                if let Ok(json) = serde_json::from_str::<Value>(&value) {
                    self.stats.redis_hits.fetch_add(1, Ordering::Relaxed);
                    self.set_memory(key, json.clone()).await;
                    return Some((json, CacheSource::Redis));
                }
            }
        }

        if let Some(sqlite_repo) = &self.sqlite_repo {
            let key = key.to_string();
            let repo = sqlite_repo.clone();
            if let Ok(Some(record)) = tokio::task::spawn_blocking(move || repo.get(&key))
                .await
                .ok()?
            {
                if let Ok(json) = serde_json::from_str::<Value>(&record.value_json) {
                    self.stats.sqlite_hits.fetch_add(1, Ordering::Relaxed);
                    self.set_memory(&record.key, json.clone()).await;
                    return Some((json, CacheSource::Sqlite));
                }
            }
        }

        None
    }

    pub async fn set(&self, key: &str, value: &Value) -> Result<()> {
        self.set_memory(key, value.clone()).await;

        if let Some(redis_repo) = &self.redis_repo {
            let json = serde_json::to_string(value)?;
            let _ = redis_repo.set(key, &json).await;
        }

        if let Some(sqlite_repo) = &self.sqlite_repo {
            let json = serde_json::to_string(value)?;
            let key = key.to_string();
            let repo = sqlite_repo.clone();
            let _ = tokio::task::spawn_blocking(move || repo.set(&key, &json)).await;
        }

        Ok(())
    }

    async fn get_from_memory(&self, key: &str) -> Option<Value> {
        let mut cache = self.memory_cache.lock().await;
        if let Some(entry) = cache.get(key) {
            if entry.expires_at > Utc::now() {
                return Some(entry.value.clone());
            }
        }
        cache.pop(key);
        None
    }

    async fn set_memory(&self, key: &str, value: Value) {
        let mut cache = self.memory_cache.lock().await;
        let expires_at = Utc::now() + Duration::seconds(self.settings.memory_ttl_seconds as i64);
        cache.put(
            key.to_string(),
            MemoryEntry {
                value,
                expires_at,
            },
        );
    }
}
