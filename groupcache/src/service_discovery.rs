use std::collections::HashSet;
use std::error::Error;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use log::error;

use crate::{Groupcache, GroupcachePeer, ValueBounds};

#[async_trait]
pub trait ServiceDiscovery: Send {
    async fn instances(&self) -> Result<Vec<GroupcachePeer>, Box<dyn Error + Send + Sync + 'static>>;
    fn delay(&self) -> Duration {
        Duration::from_secs(10)
    }
}

pub(crate) async fn run_service_discovery<Value: ValueBounds>(
    cache: Groupcache<Value>,
    mut service_discovery: Box<dyn ServiceDiscovery>,
) {
    let week_cache = Arc::downgrade(&cache.0);
    drop(cache);

    while let Some(cache) = week_cache.upgrade() {
        tokio::time::sleep(service_discovery.delay()).await;
        match service_discovery.instances().await {
            Ok(instances) => {
                if let Err(error) = cache.set_peers(instances).await {
                    error!("Error: {}", error);
                };
            }
            Err(error) => {
                error!("Error: {}", error);
            }
        }
    }
}
