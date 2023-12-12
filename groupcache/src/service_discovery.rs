use crate::GroupcachePeer;
use async_trait::async_trait;

#[async_trait]
pub trait ServiceDiscovery {
    async fn instances(&self) -> Result<Vec<GroupcachePeer>, ServiceDiscoveryError>;
}

#[derive(Debug)]
pub enum ServiceDiscoveryError {}
