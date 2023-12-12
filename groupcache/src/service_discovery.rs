use crate::GroupcachePeer;
use async_trait::async_trait;
use std::error::Error;

#[async_trait]
pub trait ServiceDiscovery: Send {
    async fn instances(&self) -> Result<Vec<GroupcachePeer>, Box<dyn Error>>;
}
