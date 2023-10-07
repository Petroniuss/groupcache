mod errors;
mod groupcache;
pub mod http;
mod routing;

use crate::errors::GroupcacheError;
use crate::groupcache::Groupcache;
use anyhow::Result;
use async_trait::async_trait;
use groupcache_pb::groupcache_pb::groupcache_client::GroupcacheClient;
use groupcache_pb::groupcache_pb::groupcache_server::GroupcacheServer;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
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

    pub fn grpc_service(&self) -> GroupcacheServer<Groupcache<Value>> {
        GroupcacheServer::from_arc(self.0.clone())
    }

    pub fn addr(&self) -> SocketAddr {
        self.0.me.socket
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

#[derive(Default)]
pub struct GroupcacheOptions {
    pub cache_capacity: Option<u64>,
    pub hot_cache_capacity: Option<u64>,
    pub hot_cache_timeout_seconds: Option<Duration>,
}

pub(crate) struct Options {
    pub(crate) cache_capacity: u64,
    pub(crate) hot_cache_capacity: u64,
    pub(crate) hot_cache_ttl: Duration,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            cache_capacity: 100_000,
            hot_cache_capacity: 10_000,
            hot_cache_ttl: Duration::from_secs(30),
        }
    }
}

impl From<GroupcacheOptions> for Options {
    fn from(value: GroupcacheOptions) -> Self {
        let default = Options::default();

        let cache_capacity = value.cache_capacity.unwrap_or(default.cache_capacity);

        let hot_cache_capacity = value
            .hot_cache_capacity
            .unwrap_or(default.hot_cache_capacity);

        let hot_cache_timeout_seconds = value
            .hot_cache_timeout_seconds
            .unwrap_or(default.hot_cache_ttl);

        Self {
            cache_capacity,
            hot_cache_capacity,
            hot_cache_ttl: hot_cache_timeout_seconds,
        }
    }
}

impl<Value: ValueBounds> GroupcacheWrapper<Value> {
    pub fn new(me: Peer, loader: Box<dyn ValueLoader<Value = Value>>) -> Self {
        GroupcacheWrapper::new_with_options(me, loader, GroupcacheOptions::default())
    }

    pub fn new_with_options(
        me: Peer,
        loader: Box<dyn ValueLoader<Value = Value>>,
        options: GroupcacheOptions,
    ) -> Self {
        let groupcache = Groupcache::new(me, loader, options.into());
        Self(Arc::new(groupcache))
    }
}
