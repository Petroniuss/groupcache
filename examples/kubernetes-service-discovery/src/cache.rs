use anyhow::{anyhow, Result};
use async_trait::async_trait;
use groupcache::{GroupcacheWrapper, Key};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::Duration;
use tracing::info;

/// [CacheLoader] implements [groupcache::ValueLoader]
///
/// In this example no state is necessary,
/// but typically [groupcache::ValueLoader] would store a reference to whatever resource the cache was protecting (database, external API etc).
pub struct CacheLoader;

#[derive(Clone, Deserialize, Serialize)]
pub struct CachedValue {
    pub plain_string: String,
}

#[async_trait]
impl groupcache::ValueLoader for CacheLoader {
    type Value = CachedValue;

    async fn load(
        &self,
        key: &Key,
    ) -> std::result::Result<Self::Value, Box<dyn std::error::Error + Send + Sync + 'static>> {
        use tokio::time::sleep;
        info!("Starting a long computation for {} .. about a 100ms.", key);
        sleep(Duration::from_millis(100)).await;

        return if key.contains("error") {
            Err(anyhow!("Something bad happened during loading :/").into())
        } else {
            Ok(CachedValue {
                plain_string: format!("Computed Value: {}", key),
            })
        };
    }
}

pub async fn configure_groupcache(socket: SocketAddr) -> Result<GroupcacheWrapper<CachedValue>> {
    let loader = CacheLoader {};
    let groupcache = GroupcacheWrapper::new(socket.into(), Box::new(loader));

    Ok(groupcache)
}
