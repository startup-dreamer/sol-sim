use crate::{ForkId, ForkInfo};
use anyhow::Result;
use redis::{AsyncCommands, Client};

#[derive(Clone)]
pub struct Storage {
    client: Client,
}

impl Storage {
    pub fn new(redis_url: &str) -> Result<Self> {
        Ok(Self {
            client: Client::open(redis_url)?,
        })
    }

    pub async fn save_fork(&self, fork: &ForkInfo) -> Result<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = format!("fork:{}", fork.fork_id);
        let value = serde_json::to_string(fork)?;
        conn.set_ex::<_, _, ()>(&key, value, 900).await?; // 15 minutes TTL
        Ok(())
    }

    pub async fn get_fork(&self, fork_id: &ForkId) -> Result<Option<ForkInfo>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = format!("fork:{}", fork_id);
        let value: Option<String> = conn.get(&key).await?;
        match value {
            Some(json) => Ok(Some(serde_json::from_str(&json)?)),
            None => Ok(None),
        }
    }

    /// Refresh TTL and update expires_at to now + 15 minutes. Returns updated ForkInfo if present.
    pub async fn refresh_fork(&self, fork_id: &ForkId) -> Result<Option<ForkInfo>> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = format!("fork:{}", fork_id);
        let value: Option<String> = conn.get(&key).await?;
        if let Some(json) = value {
            let mut info: ForkInfo = serde_json::from_str(&json)?;
            info.expires_at = chrono::Utc::now() + chrono::Duration::minutes(15);
            let new_json = serde_json::to_string(&info)?;
            conn.set_ex::<_, _, ()>(&key, new_json, 900).await?;
            Ok(Some(info))
        } else {
            Ok(None)
        }
    }

    pub async fn delete_fork(&self, fork_id: &ForkId) -> Result<()> {
        let mut conn = self.client.get_multiplexed_async_connection().await?;
        let key = format!("fork:{}", fork_id);
        conn.del::<_, ()>(&key).await?;
        Ok(())
    }
}
