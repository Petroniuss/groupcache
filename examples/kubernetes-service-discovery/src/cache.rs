use anyhow::anyhow;
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::info;

/// [MockResourceLoader] implements [groupcache::ValueLoader]
///
/// In this example no state is necessary,
/// but typically [groupcache::ValueLoader] would store a reference to whatever resource
/// the cache was protecting (database, external API etc).
pub struct MockResourceLoader;

#[derive(Clone, Deserialize, Serialize)]
pub struct CachedValue {
    pub plain_string: String,
}

#[async_trait]
impl groupcache::ValueLoader for MockResourceLoader {
    type Value = CachedValue;

    async fn load(
        &self,
        key: &str,
    ) -> Result<Self::Value, Box<dyn std::error::Error + Send + Sync + 'static>> {
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
