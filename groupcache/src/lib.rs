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
use tonic::transport::{Channel, Endpoint};

#[derive(Clone)]
pub struct GroupcacheWrapper<Value: ValueBounds>(Arc<Groupcache<Value>>);

impl<Value: ValueBounds> GroupcacheWrapper<Value> {
    pub async fn get(&self, key: &Key) -> core::result::Result<Value, GroupcacheError> {
        self.0.get(key).await
    }

    pub async fn remove(&self, key: &Key) -> core::result::Result<(), GroupcacheError> {
        self.0.remove(key).await
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
    pub(crate) socket: SocketAddr,
}

impl From<SocketAddr> for Peer {
    fn from(value: SocketAddr) -> Self {
        Self { socket: value }
    }
}

pub trait ValueBounds: Serialize + for<'a> Deserialize<'a> + Clone + Send + Sync + 'static {}

impl<T: Serialize + for<'a> Deserialize<'a> + Clone + Send + Sync + 'static> ValueBounds for T {}

pub type Key = str;

/// [ValueLoader]
///
/// Logic for loading a value for a particular key - which can be potentially expensive.
/// Groupcache is responsible for calling load on whichever node is responsible for a particular key and caching that value.
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
pub struct OptionsBuilder {
    pub main_cache_capacity: Option<u64>,
    pub hot_cache_capacity: Option<u64>,
    pub hot_cache_ttl: Option<Duration>,
    pub https: Option<bool>,
    pub grpc_endpoint_builder: Option<Box<dyn Fn(Endpoint) -> Endpoint + Send + Sync + 'static>>,
}

pub(crate) struct Options {
    pub(crate) main_cache_capacity: u64,
    pub(crate) hot_cache_capacity: u64,
    pub(crate) hot_cache_ttl: Duration,
    pub(crate) https: bool,
    pub(crate) grpc_endpoint_builder: Box<dyn Fn(Endpoint) -> Endpoint + Send + Sync + 'static>,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            main_cache_capacity: 100_000,
            hot_cache_capacity: 10_000,
            hot_cache_ttl: Duration::from_secs(30),
            https: false,
            grpc_endpoint_builder: Box::new(|e| e.timeout(Duration::from_secs(2))),
        }
    }
}

impl From<OptionsBuilder> for Options {
    fn from(builder: OptionsBuilder) -> Self {
        let default = Options::default();

        let cache_capacity = builder
            .main_cache_capacity
            .unwrap_or(default.main_cache_capacity);

        let hot_cache_capacity = builder
            .hot_cache_capacity
            .unwrap_or(default.hot_cache_capacity);

        let hot_cache_timeout_seconds = builder.hot_cache_ttl.unwrap_or(default.hot_cache_ttl);

        let https = builder.https.unwrap_or(default.https);

        let grpc_endpoint_builder = builder
            .grpc_endpoint_builder
            .unwrap_or(default.grpc_endpoint_builder);

        Self {
            main_cache_capacity: cache_capacity,
            hot_cache_capacity,
            hot_cache_ttl: hot_cache_timeout_seconds,
            https,
            grpc_endpoint_builder,
        }
    }
}

impl<Value: ValueBounds> GroupcacheWrapper<Value> {
    pub fn new(me: Peer, loader: Box<dyn ValueLoader<Value = Value>>) -> Self {
        GroupcacheWrapper::new_with_options(me, loader, OptionsBuilder::default())
    }

    pub fn new_with_options(
        me: Peer,
        loader: Box<dyn ValueLoader<Value = Value>>,
        options: OptionsBuilder,
    ) -> Self {
        let groupcache = Groupcache::new(me, loader, options.into());
        Self(Arc::new(groupcache))
    }
}
