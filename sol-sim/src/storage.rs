use crate::{ForkId, ForkInfo};
use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct Storage {
    forks: Arc<RwLock<HashMap<ForkId, ForkInfo>>>,
}

impl Storage {
    pub fn new() -> Self {
        Self {
            forks: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn save_fork(&self, fork: &ForkInfo) -> Result<()> {
        let mut forks = self.forks.write().await;
        forks.insert(fork.fork_id.clone(), fork.clone());
        Ok(())
    }

    pub async fn get_fork(&self, fork_id: &ForkId) -> Result<Option<ForkInfo>> {
        let forks = self.forks.read().await;
        Ok(forks.get(fork_id).cloned())
    }

    /// Refresh TTL and update expires_at to now + 15 minutes. Returns updated ForkInfo if present.
    pub async fn refresh_fork(&self, fork_id: &ForkId) -> Result<Option<ForkInfo>> {
        let mut forks = self.forks.write().await;
        if let Some(info) = forks.get_mut(fork_id) {
            info.expires_at = chrono::Utc::now() + chrono::Duration::minutes(15);
            Ok(Some(info.clone()))
        } else {
            Ok(None)
        }
    }

    pub async fn delete_fork(&self, fork_id: &ForkId) -> Result<()> {
        let mut forks = self.forks.write().await;
        forks.remove(fork_id);
        Ok(())
    }
}
