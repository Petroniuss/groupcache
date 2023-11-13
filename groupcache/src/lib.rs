mod errors;
mod groupcache;
mod http;
mod metrics;
mod options;
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
use tonic::transport::Channel;

// expose Options
pub use options::{Options, OptionsBuilder};

// todo: consider moving it to a separate file.
// think about exporting external modules:
//    - options
//    - groupcache (interface, not internal)

#[derive(Clone)]
pub struct GroupcacheWrapper<Value: ValueBounds>(Arc<Groupcache<Value>>);

impl<Value: ValueBounds> GroupcacheWrapper<Value> {
    pub async fn get(&self, key: &Key) -> core::result::Result<Value, GroupcacheError> {
        self.0.get(key).await
    }

    pub async fn remove(&self, key: &Key) -> core::result::Result<(), GroupcacheError> {
        self.0.remove(key).await
    }

    pub async fn add_peer(&self, peer: GroupcachePeer) -> Result<()> {
        self.0.add_peer(peer).await
    }

    pub async fn remove_peer(&self, peer: GroupcachePeer) -> Result<()> {
        self.0.remove_peer(peer).await
    }

    pub fn grpc_service(&self) -> GroupcacheServer<Groupcache<Value>> {
        GroupcacheServer::from_arc(self.0.clone())
    }

    pub fn addr(&self) -> SocketAddr {
        self.0.me.socket
    }

    pub fn new(me: GroupcachePeer, loader: Box<dyn ValueLoader<Value = Value>>) -> Self {
        GroupcacheWrapper::new_with_options(me, loader, Options::default())
    }

    pub fn new_with_options(
        me: GroupcachePeer,
        loader: Box<dyn ValueLoader<Value = Value>>,
        options: Options<Value>,
    ) -> Self {
        let groupcache = Groupcache::new(me, loader, options);
        Self(Arc::new(groupcache))
    }
}

/// [ValueLoader]
///
/// Loads a value for a particular key - which can be potentially expensive.
/// Groupcache is responsible for calling load on whichever node is responsible for a particular key and caching that value.
/// [ValueLoader::Value]s will be cached by groupcache according to passed options.
///
/// [ValueLoader::Value] cached by groupcache must satisfy [ValueBounds].
///
/// If you want to load resources of different types,
/// your implementation of load may distinguish desired type by prefix of [Key] and return an enum.
/// This is a deviation from original groupcache library which implemented separate groups.
#[async_trait]
pub trait ValueLoader: Send + Sync {
    type Value: ValueBounds;

    async fn load(
        &self,
        key: &Key,
    ) -> std::result::Result<Self::Value, Box<dyn std::error::Error + Send + Sync + 'static>>;
}

/// [ValueLoader::Value] cached by groupcache must satisfy [ValueBounds]:
/// - serializable/deserializable: because they're sent over the network,
/// - cloneable: because value is loaded once and then multiplexed to all callers via clone,
/// - Send + Sync + 'static: because they're shared across potentially many threads.
///
/// Typical data structs should automatically conform to this trait.
/// ```
///     use serde::{Deserialize, Serialize};
///     #[derive(Clone, Deserialize, Serialize)]
///     struct DatabaseEntity {
///         id: String,
///         value: String,
///     }
/// ```
///
/// For small datastructures plain struct should suffice but if cached [ValueLoader::Value]
/// was large enough it might be worth it to wrap it inside [Arc] so that cached values are
/// are stored in memory only once and reference the same piece of data.
///
/// ```
///     use std::sync::Arc;
///     use serde::{Deserialize, Serialize};
///
///     #[derive(Clone, Deserialize, Serialize)]
///     struct Wrapped (Arc<Entity>);
///
///     #[derive(Clone, Deserialize, Serialize)]
///     struct Entity {
///         id: String,
///         value: String,
///     }
/// ```
pub trait ValueBounds: Serialize + for<'a> Deserialize<'a> + Clone + Send + Sync + 'static {}

/// Automatically implement ValueBounds for types that satisfy the trait.
impl<T: Serialize + for<'a> Deserialize<'a> + Clone + Send + Sync + 'static> ValueBounds for T {}

// todo: look whether this any useful..
/// Groupcache caches values by [Key] which is plain str.
pub type Key = str;

type GroupcachePeerClient = GroupcacheClient<Channel>;

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct GroupcachePeer {
    pub(crate) socket: SocketAddr,
}

impl GroupcachePeer {
    pub fn from_socket(value: SocketAddr) -> Self {
        From::from(value)
    }
}

impl From<SocketAddr> for GroupcachePeer {
    fn from(value: SocketAddr) -> Self {
        Self { socket: value }
    }
}
