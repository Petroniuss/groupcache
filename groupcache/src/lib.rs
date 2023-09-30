mod errors;
mod groupcache;
pub mod http;
mod routing;

use crate::errors::GroupcacheError;
use crate::groupcache::Groupcache;
use anyhow::Result;
use async_trait::async_trait;
use groupcache_pb::groupcache_pb::groupcache_client::GroupcacheClient;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tonic::transport::Channel;

#[derive(Clone)]
pub struct GroupcacheWrapper<Value: ValueBounds>(Arc<Groupcache<Value>>);

impl<Value: ValueBounds> GroupcacheWrapper<Value> {
    pub async fn get(&self, key: &Key) -> core::result::Result<Value, GroupcacheError> {
        self.0.get(key).await
    }

    pub async fn add_peer(&self, peer: Peer) -> Result<()> {
        self.0.add_peer(peer).await
    }

    pub async fn remove_peer(&self, peer: Peer) -> Result<()> {
        self.0.remove_peer(peer).await
    }
}

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct Peer {
    socket: SocketAddr,
}

impl From<SocketAddr> for Peer {
    fn from(value: SocketAddr) -> Self {
        Self { socket: value }
    }
}

pub trait ValueBounds: Serialize + for<'a> Deserialize<'a> + Clone + Send + Sync + 'static {}

impl<T: Serialize + for<'a> Deserialize<'a> + Clone + Send + Sync + 'static> ValueBounds for T {}

pub type Key = str;

#[async_trait]
pub trait ValueLoader: Send + Sync {
    type Value: ValueBounds;

    async fn load(
        &self,
        key: &Key,
    ) -> std::result::Result<Self::Value, Box<dyn std::error::Error + Send + Sync + 'static>>;
}

type PeerClient = GroupcacheClient<Channel>;

impl<Value: ValueBounds> GroupcacheWrapper<Value> {
    pub fn new(me: Peer, loader: Box<dyn ValueLoader<Value = Value>>) -> Self {
        let groupcache = Groupcache::new(me, loader);
        Self(Arc::new(groupcache))
    }
}
