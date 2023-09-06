mod errors;

use crate::errors::{DedupedGroupcacheError, GroupcacheError, InternalGroupcacheError};
use anyhow::{anyhow, Context, Result};
use async_trait::async_trait;
use groupcache_pb::groupcache_pb::groupcache_client::GroupcacheClient;
use groupcache_pb::groupcache_pb::{groupcache_server, GetRequest, GetResponse};
use hashring::HashRing;
use quick_cache::sync::Cache;
use serde::{Deserialize, Serialize};
use singleflight_async::SingleFlight;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use tonic::transport::Channel;
use tonic::{IntoRequest, Request, Response, Status};
use tracing::log::error;
use tracing::{info, log};

static VNODES_PER_PEER: i32 = 40;

type PeerClient = GroupcacheClient<Channel>;

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
pub struct Peer {
    pub socket: SocketAddr,
}

#[derive(Debug, Copy, Clone, Hash, PartialEq, Eq)]
struct VNode {
    id: usize,
    addr: SocketAddr,
}

impl VNode {
    fn new(addr: SocketAddr, id: usize) -> Self {
        VNode { id, addr }
    }

    fn vnodes_for_peer(peer: &Peer, num: i32) -> Vec<VNode> {
        let mut vnodes = Vec::new();
        for i in 0..num {
            let vnode = VNode::new(peer.socket, i as usize);

            vnodes.push(vnode);
        }
        vnodes
    }

    fn as_peer(&self) -> Peer {
        Peer { socket: self.addr }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetResponseFailure {
    pub key: String,
    pub error: String,
}

struct RoutingState {
    peers: HashMap<Peer, ConnectedPeer>,
    ring: HashRing<VNode>,
}

impl RoutingState {
    fn peer_for_key(&self, key: &Key) -> Result<Peer> {
        let vnode = self.ring.get(key).context("ring can't be empty")?;

        Ok(vnode.as_peer())
    }

    fn client_for_peer(&self, peer: &Peer) -> Result<PeerClient> {
        match self.peers.get(peer) {
            Some(peer) => Ok(peer.client.clone()),
            None => Err(anyhow!("peer not found")),
        }
    }

    fn add_peer(&mut self, peer: Peer, client: PeerClient) {
        let vnodes = VNode::vnodes_for_peer(&peer, VNODES_PER_PEER);
        for vnode in vnodes {
            self.ring.add(vnode);
        }
        self.peers.insert(peer, ConnectedPeer { client });
    }

    fn contains_peer(&self, peer: &Peer) -> bool {
        self.peers.contains_key(peer)
    }
}

struct ConnectedPeer {
    client: PeerClient,
}

pub struct Groupcache<Value: ValueBounds> {
    me: Peer,
    routing_state: Arc<RwLock<RoutingState>>,
    single_flight_group: SingleFlight<Result<Value, DedupedGroupcacheError>>,
    cache: Cache<Key, Value>,
    // todo: this can use static dispatch I presume.
    loader: Box<dyn ValueLoader<Value = Value>>,
}

pub type Key = String;

#[async_trait]
impl<Value: ValueBounds> groupcache_server::Groupcache for Groupcache<Value> {
    async fn get(
        &self,
        request: Request<GetRequest>,
    ) -> std::result::Result<Response<GetResponse>, Status> {
        let payload = request.into_inner();
        info!("get key:{}", payload.key);

        match self.get(&payload.key).await {
            Ok(value) => {
                let result = rmp_serde::to_vec(&value);
                match result {
                    Ok(bytes) => Ok(Response::new(GetResponse { value: Some(bytes) })),
                    Err(err) => {
                        error!(
                            "Error during computing value for key: {}, err: {}",
                            payload.key, err
                        );
                        Err(Status::internal(err.to_string()))
                    }
                }
            }
            Err(err) => Err(Status::internal(err.to_string())),
        }
    }
}

pub async fn start_grpc_server<Value: ValueBounds>(
    groupcache: Arc<Groupcache<Value>>,
) -> Result<()> {
    let addr = groupcache.me.socket;
    info!("Groupcache server listening on {}", addr);

    tonic::transport::Server::builder()
        .add_service(groupcache_server::GroupcacheServer::from_arc(groupcache))
        .serve(addr)
        .await?;

    Ok(())
}

pub trait ValueBounds: Serialize + for<'a> Deserialize<'a> + Clone + Send + Sync + 'static {}
impl<T: Serialize + for<'a> Deserialize<'a> + Clone + Send + Sync + 'static> ValueBounds for T {}

#[async_trait]
pub trait ValueLoader: Send + Sync {
    type Value: ValueBounds;

    async fn load(
        &self,
        key: &Key,
    ) -> std::result::Result<Self::Value, Box<dyn std::error::Error + Send + Sync + 'static>>;
}

impl<Value: ValueBounds> Groupcache<Value> {
    pub fn new(me: Peer, loader: Box<dyn ValueLoader<Value = Value>>) -> Self {
        let ring = {
            let mut ring = HashRing::new();
            let vnodes = VNode::vnodes_for_peer(&me, VNODES_PER_PEER);
            for vnode in vnodes {
                ring.add(vnode)
            }

            ring
        };

        let cache = Cache::new(1_000_000);
        let peers = HashMap::new();

        let guarded_shared_state = Arc::new(RwLock::new(RoutingState { peers, ring }));

        Self {
            me,
            routing_state: guarded_shared_state,
            single_flight_group: SingleFlight::new(),
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

    // todo: I don't like this error wrapping - figure this out - provide simple error type for the user?
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

    async fn load_locally(&self, key: &Key) -> Result<Value> {
        // todo: map this to a custom error type
        let v = self
            .loader
            .load(key)
            .await
            .map_err(InternalGroupcacheError::Loader)?;
        Ok(v)
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
    pub async fn add_peer(&self, peer: Peer) -> Result<()> {
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
}

#[cfg(test)]
mod tests {

    #[test]
    fn it_works() {}
}
