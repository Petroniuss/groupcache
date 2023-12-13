use std::collections::HashSet;
use std::error::Error;
use std::time::Duration;

use async_trait::async_trait;
use log::error;

use crate::{Groupcache, GroupcachePeer, ValueBounds};

#[async_trait]
pub trait ServiceDiscovery: Send {
    async fn initialize(&mut self) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
        Ok(())
    }
    async fn instances(
        &self,
    ) -> Result<Vec<GroupcachePeer>, Box<dyn Error + Send + Sync + 'static>>;
    fn delay(&self) -> Duration {
        Duration::from_secs(10)
    }
}
pub(crate) async fn run_service_discovery<Value: ValueBounds>(
    cache: Groupcache<Value>,
    mut service_discovery: Box<dyn ServiceDiscovery>,
) {
    service_discovery.initialize().await.unwrap();
    let mut current = HashSet::<GroupcachePeer>::default(); // TODO: remove

    loop {
        tokio::time::sleep(service_discovery.delay()).await;
        match service_discovery.instances().await {
            Ok(instances) => {
                let new = HashSet::from_iter(instances.to_owned());

                for instance in current.iter() {
                    if !new.contains(instance) {
                        cache.remove_peer(*instance).await.expect("");
                    }
                }

                for instance in instances {
                    if !current.contains(&instance) {
                        cache.add_peer(instance).await.expect("");
                    }
                }

                current = new;
            }
            Err(error) => {
                error!("Error: {}", error);
            }
        }
    }
}
