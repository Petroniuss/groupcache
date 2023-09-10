mod errors;
pub mod http;
mod routing;

use crate::errors::{DedupedGroupcacheError, GroupcacheError, InternalGroupcacheError};
use crate::routing::RoutingState;
use anyhow::Result;
use async_trait::async_trait;
use groupcache_pb::groupcache_pb::groupcache_client::GroupcacheClient;
use groupcache_pb::groupcache_pb::GetRequest;
use quick_cache::sync::Cache;
use serde::{Deserialize, Serialize};
use singleflight_async::SingleFlight;
use std::net::SocketAddr;

use std::sync::{Arc, RwLock};
use tonic::transport::Channel;
use tonic::IntoRequest;
use tracing::log;

// todo: keep public api in lib.rs move implementation to separate file.
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

pub type Key = String;

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

struct Groupcache<Value: ValueBounds> {
    me: Peer,
    routing_state: Arc<RwLock<RoutingState>>,
    single_flight_group: SingleFlight<Result<Value, DedupedGroupcacheError>>,
    cache: Cache<Key, Value>,
    loader: Box<dyn ValueLoader<Value = Value>>,
}

impl<Value: ValueBounds> Groupcache<Value> {
    pub fn new(me: Peer, loader: Box<dyn ValueLoader<Value = Value>>) -> Self {
        let routing_state = Arc::new(RwLock::new(RoutingState::with_local_peer(me)));
        let cache = Cache::new(1_000_000);
        let single_flight_group = SingleFlight::default();

        Self {
            me,
            routing_state,
            single_flight_group,
            cache,
            loader,
        }
    }

    pub async fn get(&self, key: &Key) -> core::result::Result<Value, GroupcacheError> {
        Ok(self.get_internal(key).await?)
    }

    async fn get_internal(&self, key: &Key) -> Result<Value, InternalGroupcacheError> {
        if let Some(value) = self.cache.get(key) {
            log::info!("peer {:?} serving from cache: {:?}", self.me.socket, key);
            return Ok(value);
        }

        let peer = {
            let lock = self.routing_state.read().unwrap();

            lock.peer_for_key(key)?
        };
        log::info!(
            "peer {:?} getting from peer: {:?}",
            self.me.socket,
            peer.socket
        );

        let value = self.get_dedup(key, peer).await?;
        Ok(value)
    }

    async fn get_dedup(&self, key: &Key, peer: Peer) -> Result<Value> {
        let value = self
            .single_flight_group
            .work(key, || async { self.error_wrapped_dedup(key, peer).await })
            .await?;

        Ok(value)
    }

    async fn error_wrapped_dedup(
        &self,
        key: &Key,
        peer: Peer,
    ) -> Result<Value, DedupedGroupcacheError> {
        self.dedup_get(key, peer)
            .await
            .map_err(|e| DedupedGroupcacheError(Arc::new(e)))
    }

    async fn dedup_get(&self, key: &Key, peer: Peer) -> Result<Value> {
        let value = if peer == self.me {
            let value = self.load_locally(key).await?;
            self.cache.insert(key.clone(), value.clone());
            value
        } else {
            let value = self.load_remotely(key, peer).await?;
            value
        };

        Ok(value)
    }

    async fn load_locally(&self, key: &Key) -> Result<Value, InternalGroupcacheError> {
        self.loader
            .load(key)
            .await
            .map_err(InternalGroupcacheError::Loader)
    }

    async fn load_remotely(&self, key: &Key, peer: Peer) -> Result<Value> {
        let mut client = {
            let read_lock = self.routing_state.read().unwrap();
            read_lock.client_for_peer(&peer)?
        };

        let response = client
            .get(GetRequest { key: key.clone() }.into_request())
            .await?;

        let get_response = response.into_inner();
        let bytes = get_response.value.unwrap();
        let value = rmp_serde::from_read(bytes.as_slice())?;

        Ok(value)
    }

    // todo: interesting how to have fast access to state that's often read but rarely updated:
    // Simpler option:
    //  - use RwLock
    // Other options:
    // https://www.reddit.com/r/rust/comments/vcaabk/rwlock_vs_mutex_please_tell_me_like_im_5/
    // https://www.reddit.com/r/rust/comments/vb1p6i/getting_both_a_mutable_and_immutable_reference_to/
    // https://youtu.be/s19G6n0UjsM?t=1472
    // - have a channel with updates, duplicate state to all readers and update state from channel in a non-blocking way.
    // https://crates.io/crates/arc-swap/1.6.0

    // todo: implement batch api so that one can more efficiently add a number of peers
    async fn add_peer(&self, peer: Peer) -> Result<()> {
        let contains_peer = {
            let read_lock = self.routing_state.read().unwrap();
            read_lock.contains_peer(&peer)
        };

        if contains_peer {
            return Ok(());
        }

        // todo: it should be up to the user to define whether we want to use http or https?
        // but then we'd also need to give ability to set up certs etc...
        let peer_server_address = format!("http://{}", peer.socket.clone());

        // todo: test what happens when connection is broken between peers.
        let client = GroupcacheClient::connect(peer_server_address).await?;

        let mut write_lock = self.routing_state.write().unwrap();
        write_lock.add_peer(peer, client);

        Ok(())
    }

    async fn remove_peer(&self, peer: Peer) -> Result<()> {
        let contains_peer = {
            let read_lock = self.routing_state.read().unwrap();
            read_lock.contains_peer(&peer)
        };

        if !contains_peer {
            return Ok(());
        }

        let mut write_lock = self.routing_state.write().unwrap();
        write_lock.remove_peer(peer);

        Ok(())
    }
}
