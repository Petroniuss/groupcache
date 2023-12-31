//! groupcache module provides public interface for consumers of the library.
//!
//! Entire functionality is exposed via [Groupcache] struct, which needs [ValueLoader] to be constructed.
use crate::errors::GroupcacheError;
use crate::groupcache_builder::GroupcacheBuilder;
use crate::GroupcacheInner;
use async_trait::async_trait;
use groupcache_pb::GroupcacheClient;
use groupcache_pb::GroupcacheServer;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use tonic::transport::Channel;

/// Contains most of the library API.
///
/// It is an [`Arc`] wrapper around [`GroupcacheInner`] which implements the API,
/// so that applications don't have to wrap groupcache inside [`Arc`] themselves
/// in concurrent context which is target audience.
///
/// In order for groupcache peers to discover themselves application author needs to hook in some service discovery:
/// - static IP addresses of hosts running groupcache
/// - [consul](https://www.consul.io/)
/// - [kubernetes API server](https://kubernetes.io/docs/reference/command-line-tools-reference/kube-apiserver/)
/// - ..
///
/// Integration of service discovery with groupcache can be done via:
/// - [`Groupcache::set_peers`] - preferred for pull-based service discovery,
/// - [`Groupcache::add_peer`] and [`Groupcache::remove_peer`] - preferred for push-based service discovery.
///
/// There is an example showing how to use kubernetes API server for service discovery with groupcache
/// [See here](https://github.com/Petroniuss/groupcache/tree/main/examples/kubernetes-service-discovery)
#[derive(Clone)]
pub struct Groupcache<Value: ValueBounds>(pub(crate) Arc<GroupcacheInner<Value>>);

impl<Value: ValueBounds> Groupcache<Value> {
    /// In order to construct [`Groupcache`] application needs to provide:
    /// - [`GroupcachePeer`] - necessary for routing
    /// - [`ValueLoader`] implementation
    pub fn builder(
        me: GroupcachePeer,
        loader: impl ValueLoader<Value = Value> + 'static,
    ) -> GroupcacheBuilder<Value> {
        GroupcacheBuilder::new(me, Box::new(loader))
    }

    /// Provided a given `key`
    /// groupcache attempts to figure out which peer owns
    /// a given KV pair based on consistent hashing
    /// and make sure that only this peer handles loading values for this particular key
    /// unless it's `hot` value which may be cached locally.
    ///
    /// If KV is found in local `hot_cache`, cached value is returned.
    /// If KV is owned by this peer and value is cached in `main_cache` it is returned.
    /// If KV is owned by this peer it is loaded via [`ValueLoader`].
    /// If KV is owned by a different peer, gRPC request is made to this peer using address provided in [`Groupcache::add_peer`]
    /// and that peer is responsible for loading that value in a replicated set of processes.
    /// If a request to peer fails, peer tries to load value locally via [`ValueLoader`].
    /// If loading value via [`ValueLoader`] fails an error is returned.
    ///
    /// Groupcache coordinates cache fills such that only one load in one process of an entire replicated set of processes populates the cache,
    /// then multiplexes the loaded value to all callers.
    ///
    /// Caches can be customized via [`Options`].
    pub async fn get(&self, key: &str) -> Result<Value, GroupcacheError> {
        self.0.get(key).await
    }

    /// Original groupcache library only provided [`Groupcache::get`] but there are use-cases
    /// where KV pairs need to be updated but this is problematic to do in a distributed system based on consistent hashing.
    ///
    /// This library does a simple thing:
    /// - remove method removes KV pair from `main_cache` of the owner of the KV pair
    /// - and removes KV pair from `hot_cache` of this node.
    ///
    /// However removed KV pair may still be cached on other nodes in `hot_cache`.
    /// To deal with this application can either:
    /// - accept that there might be some stale values served from `hot_cache` for some time after call to `remove`.
    /// - tweak `hot_cache` in such a way that it is acceptable for the application.
    ///   For example disabling it entirely so that value is cached only on the owner node of KV pair.
    ///   Note that this will likely increase number of RPCs over the network since all requests will have to go to the owner.
    pub async fn remove(&self, key: &str) -> Result<(), GroupcacheError> {
        self.0.remove(key).await
    }

    /// service-discovery:
    ///
    /// Once in a while groupcache backend should refresh view of groupcache nodes
    /// to make sure that groupcache routes traffic evenly to all healthy nodes.
    ///
    /// This method can be used to notify groupcache about all peers in the cluster.
    /// Groupcache will figure out:
    /// - which are new and - will try to open a connection to these peers
    /// - which it already knew about - will keep connection open as is.
    /// - which it knew about but are now missing - will disconnect with such peers
    ///
    /// Instead of using this method directly in a loop,
    /// applications can implement [`crate::ServiceDiscovery`]
    /// and only provide implementation fetching state of the cluster.
    ///
    /// If it isn't possible to connect to some [`GroupcachePeer`]s
    /// this method will return an error with all the peers it failed to connect to
    /// and won't update routing table with these peers.
    /// It will however update its routing table accordingly with peers that it successfully connected with.
    ///
    /// Note that [`Groupcache::set_peers`] isn't broadcasted to other peers,
    /// and each groupcache peer needs to update its routing table via the same call.
    /// In other words this only updates local routing table, not routing table of all nodes in the cluster.
    pub async fn set_peers(&self, peers: HashSet<GroupcachePeer>) -> Result<(), GroupcacheError> {
        self.0.set_peers(peers).await
    }

    /// service-discovery:
    ///
    /// whenever application notices that there is new groupcache peer it should notify groupcache
    /// so that routing table/consistent hashing ring can be updated.
    ///
    /// If it isn't possible to connect to [`GroupcachePeer`]
    /// this method will return an error and won't update the routing table.
    ///
    /// Upon success some portion of requests will be forwarded to this peer.
    ///
    /// Note that [`Groupcache::add_peer`] isn't broadcasted to other peers,
    /// and each groupcache peer needs to update its routing table via the same call.
    /// In other words this only updates local routing table, not routing table of all nodes in the cluster.
    pub async fn add_peer(&self, peer: GroupcachePeer) -> Result<(), GroupcacheError> {
        self.0.add_peer(peer).await
    }

    /// service-discovery:
    ///
    /// whenever application notices that a groupcache peer is no longer able to serve requests because:
    /// - it is down
    /// - the server is not healthy
    /// - it has been moved to a different address via container orchestrator
    /// - it isn't reachable.
    /// so that routing table/consistent hashing ring can be updated.
    ///
    /// Requests will no longer be forwarded to this peer.
    ///
    /// Note that [`Groupcache::remove_peer`] isn't broadcasted to other peers,
    /// and each groupcache peer needs to update its routing table via the same call.
    /// In other words this only updates local routing table, not routing table of all nodes in the cluster.
    pub async fn remove_peer(&self, peer: GroupcachePeer) -> Result<(), GroupcacheError> {
        self.0.remove_peer(peer).await
    }

    /// Retrieves underlying groupcache gRPC server implementation.
    ///
    /// Library doesn't start gRPC server automatically, it's instead responsibility of an application to do so.
    /// It is done this way to allow for customisations (tracing, metrics etc), see examples.
    pub fn grpc_service(&self) -> GroupcacheServer<GroupcacheInner<Value>> {
        GroupcacheServer::from_arc(self.0.clone())
    }

    /// Returns address of this peer.
    pub fn addr(&self) -> SocketAddr {
        self.0.addr()
    }
}

/// [ValueLoader] loads a value for a particular key - which can be potentially expensive.
/// Groupcache is responsible for calling load on whichever node is responsible for a particular key and caching that value.
/// [ValueLoader::Value]s will be cached by groupcache according to passed options.
///
/// [ValueLoader::Value] cached by groupcache must satisfy [ValueBounds].
///
/// If you want to load resources of different types,
/// your implementation of load may distinguish desired type by prefix of `key` and return an enum as [ValueLoader::Value].
/// This is a deviation from original groupcache library which implemented separate groups
/// and consumers of the library could have multiple implementation of [ValueLoader].
#[async_trait]
pub trait ValueLoader: Send + Sync {
    /// Value is a type returned by load, see [ValueBounds].
    type Value: ValueBounds;

    async fn load(
        &self,
        key: &str,
    ) -> Result<Self::Value, Box<dyn std::error::Error + Send + Sync + 'static>>;
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

/// Automatically implement [`ValueBounds`] for types that satisfy the trait.
impl<T: Serialize + for<'a> Deserialize<'a> + Clone + Send + Sync + 'static> ValueBounds for T {}

/// Groupcache uses tonic to connect to its peers
pub(crate) type GroupcachePeerClient = GroupcacheClient<Channel>;

/// Wrapper around peer address, will be used to connect to groupcache peer.
/// Peer should expose [`Groupcache::grpc_service`] under this address.
///
/// Use [`GroupcachePeer::from_socket`] or [`From<SocketAddr>`] to construct.
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
