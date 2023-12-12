use std::error::Error;
use std::time::Duration;

use async_trait::async_trait;

use crate::GroupcachePeer;

#[async_trait]
pub trait ServiceDiscovery: Send {
    async fn initialize(&mut self) -> Result<(), Box<dyn Error>>;
    async fn instances(&self) -> Result<Vec<GroupcachePeer>, Box<dyn Error>>;
    fn delay(&self) -> Duration;
}
