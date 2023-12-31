use crate::{GroupcacheInner, GroupcachePeer, ValueBounds};
use async_trait::async_trait;
use log::error;
use std::collections::HashSet;
use std::error::Error;
use std::sync::Weak;
use std::time::Duration;

/// This trait abstracts away boilerplate associated with pull-based service discovery.
///
/// groupcache periodically runs [`ServiceDiscovery::pull_instances`] and
/// compares pulled instances with the ones it already knew about
/// from previous invocations of [`ServiceDiscovery::pull_instances`].
///
/// groupcache connects with new peers and disconnects with missing peers via [`Groupcache::set_peers`].
#[async_trait]
pub trait ServiceDiscovery: Send {
    /// Pulls groupcache instances from a source-of-truth system (kubernetes API server, consul etc).
    ///
    /// Based on this function groupcache is able to update its routing table,
    /// so that it can correctly route to healthy instances and stop hitting unhealthy nodes.
    ///
    /// Returning Error from this function will be logged by groupcache
    /// but routing table will not be updated.
    async fn pull_instances(
        &self,
    ) -> Result<HashSet<GroupcachePeer>, Box<dyn Error + Send + Sync + 'static>>;

    /// Specifies duration between consecutive executions of [`ServiceDiscovery::pull_instances`].
    fn interval(&self) -> Duration {
        Duration::from_secs(10)
    }
}

pub(crate) async fn run_service_discovery<Value: ValueBounds>(
    cache: Weak<GroupcacheInner<Value>>,
    service_discovery: Box<dyn ServiceDiscovery>,
) {
    while let Some(cache) = cache.upgrade() {
        tokio::time::sleep(service_discovery.interval()).await;
        match service_discovery.pull_instances().await {
            Ok(instances) => {
                if let Err(error) = cache.set_peers(instances).await {
                    // todo: improve log message
                    error!("Error: {}", error);
                };
            }
            Err(error) => {
                error!("Error: {}", error);
            }
        }
    }
}
