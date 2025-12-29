use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use rusqlite::{params, Connection};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct CacheRecord {
    pub key: String,
    pub value_json: String,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub hits: u64,
}

#[derive(Clone)]
pub struct CacheRepo {
    path: PathBuf,
    ttl_days: i64,
    max_size_bytes: u64,
}

impl CacheRepo {
    pub fn new(path: impl Into<PathBuf>, ttl_days: u32, max_size_gb: u64) -> Result<Self> {
        let path = path.into();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create sqlite cache directory: {}", parent.display())
            })?;
        }
        let repo = Self {
            path,
            ttl_days: ttl_days as i64,
            max_size_bytes: max_size_gb * 1024 * 1024 * 1024,
        };
        repo.init()?;
        Ok(repo)
    }

    fn init(&self) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS ai_cache (
                cache_key TEXT PRIMARY KEY,
                response_json TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                expires_at INTEGER NOT NULL,
                hits INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_ai_cache_expires ON ai_cache(expires_at);
            CREATE TABLE IF NOT EXISTS cache_stats (
                metric TEXT PRIMARY KEY,
                value INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            );",
        )?;
        Ok(())
    }

    pub fn get(&self, key: &str) -> Result<Option<CacheRecord>> {
        let conn = Connection::open(&self.path)?;
        let now = Utc::now().timestamp();
        let mut stmt = conn.prepare(
            "SELECT cache_key, response_json, created_at, expires_at, hits
             FROM ai_cache
             WHERE cache_key = ?1 AND expires_at > ?2",
        )?;

        let mut rows = stmt.query(params![key, now])?;
        if let Some(row) = rows.next()? {
            let hits: u64 = row.get::<_, i64>(4)? as u64 + 1;
            conn.execute(
                "UPDATE ai_cache SET hits = ?1 WHERE cache_key = ?2",
                params![hits as i64, key],
            )?;

            let created_at_ts: i64 = row.get(2)?;
            let expires_at_ts: i64 = row.get(3)?;
            let record = CacheRecord {
                key: row.get(0)?,
                value_json: row.get(1)?,
                created_at: DateTime::<Utc>::from_utc(
                    chrono::NaiveDateTime::from_timestamp_opt(created_at_ts, 0)
                        .unwrap_or_else(|| chrono::NaiveDateTime::from_timestamp_opt(0, 0).unwrap()),
                    Utc,
                ),
                expires_at: DateTime::<Utc>::from_utc(
                    chrono::NaiveDateTime::from_timestamp_opt(expires_at_ts, 0)
                        .unwrap_or_else(|| chrono::NaiveDateTime::from_timestamp_opt(0, 0).unwrap()),
                    Utc,
                ),
                hits,
            };
            Ok(Some(record))
        } else {
            Ok(None)
        }
    }

    pub fn set(&self, key: &str, value_json: &str) -> Result<()> {
        let conn = Connection::open(&self.path)?;
        let now = Utc::now();
        let expires_at = now + Duration::days(self.ttl_days);

        conn.execute(
            "INSERT INTO ai_cache (cache_key, response_json, created_at, expires_at, hits)
             VALUES (?1, ?2, ?3, ?4, 0)
             ON CONFLICT(cache_key) DO UPDATE SET
                response_json = excluded.response_json,
                created_at = excluded.created_at,
                expires_at = excluded.expires_at",
            params![
                key,
                value_json,
                now.timestamp(),
                expires_at.timestamp()
            ],
        )?;
        self.cleanup_if_needed()?;
        Ok(())
    }

    pub fn cleanup_expired(&self) -> Result<u64> {
        let conn = Connection::open(&self.path)?;
        let now = Utc::now().timestamp();
        let rows = conn.execute("DELETE FROM ai_cache WHERE expires_at <= ?1", params![now])?;
        Ok(rows as u64)
    }

    fn cleanup_if_needed(&self) -> Result<()> {
        if self.max_size_bytes == 0 {
            return Ok(());
        }
        let size = self.db_size()?;
        if size <= self.max_size_bytes {
            return Ok(());
        }

        let conn = Connection::open(&self.path)?;
        conn.execute_batch(
            "DELETE FROM ai_cache
             WHERE cache_key IN (
                SELECT cache_key FROM ai_cache ORDER BY created_at ASC LIMIT 500
             );",
        )?;
        conn.execute_batch("VACUUM;")?;
        Ok(())
    }

    fn db_size(&self) -> Result<u64> {
        if !Path::new(&self.path).exists() {
            return Ok(0);
        }
        let metadata = fs::metadata(&self.path)?;
        Ok(metadata.len())
    }
}
