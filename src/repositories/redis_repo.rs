use anyhow::Result;
use redis::aio::ConnectionManager;
use redis::AsyncCommands;

#[derive(Clone)]
pub struct RedisRepo {
    manager: ConnectionManager,
    ttl_seconds: u64,
}

impl RedisRepo {
    pub async fn new(redis_url: &str, ttl_seconds: u64) -> Result<Self> {
        let client = redis::Client::open(redis_url)?;
        let manager = client.get_connection_manager().await?;
        Ok(Self {
            manager,
            ttl_seconds,
        })
    }

    pub async fn get(&self, key: &str) -> Result<Option<String>> {
        let mut conn = self.manager.clone();
        let value: Option<String> = conn.get(key).await?;
        Ok(value)
    }

    pub async fn set(&self, key: &str, value: &str) -> Result<()> {
        let mut conn = self.manager.clone();
        if self.ttl_seconds > 0 {
            conn.set_ex::<_, _, ()>(key, value, self.ttl_seconds)
                .await?;
        } else {
            conn.set::<_, _, ()>(key, value).await?;
        }
        Ok(())
    }
}
