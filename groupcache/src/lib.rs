extern crate serde;
extern crate anyhow;
extern crate async_trait;
extern crate quick_cache;

use std::collections::HashMap;
use groupcache_pb::groupcache_pb::groupcache_server;
use std::error::Error;
use std::future::Future;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use serde::{Deserialize, Serialize};
use anyhow::{anyhow, bail, Context, Result};
use async_trait::async_trait;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use hashring::HashRing;
use tracing::{info, log};
use tracing::log::{error, log};
use quick_cache::sync::Cache;
use tonic::{IntoRequest, Request, Status};
use tonic::transport::Channel;
use groupcache_pb::groupcache_pb::groupcache_client::GroupcacheClient;


static VNODES_PER_PEER: i32 = 10;

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
        VNode {
            id,
            addr,
        }
    }

    fn vnodes_for_peer(peer: &Peer, num: i32) -> Vec<VNode> {
        let mut vnodes = Vec::new();
        for i in 0..num {
            let vnode = VNode::new(peer.socket.clone(), i as usize);
            vnodes.push(vnode);
        }
        vnodes
    }

    fn to_peer(&self) -> Peer {
        return Peer {
            socket: self.addr.clone()
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetRequest {
    key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetResponse {
    key: String,
    value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GetResponseFailure {
    pub key: String,
    pub error: String,
}

#[async_trait]
pub trait Transport: Send + Sync {
    async fn get_rpc(&self, peer: &Peer, req: &GetRequest) -> Result<GetResponse>;
}

pub struct ReqwestTransport {
    client: reqwest::Client,
}

impl ReqwestTransport {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }
}

#[async_trait]
impl Transport for ReqwestTransport {
    async fn get_rpc(&self, peer: &Peer, req: &GetRequest) -> Result<GetResponse> {
        let addr = peer.socket.to_string();
        let response = self.client
            .get(format!("http://{}/get/{}", addr, req.key))
            .send()
            .await?;

        let status = response.status();
        if status != StatusCode::OK {
            let body = response.json::<GetResponseFailure>().await?;
            bail!("bad status code: {}, {:?}", status, body);
        }

        let response = response.json::<GetResponse>().await?;

        Ok((response))
    }
}


#[async_trait]
trait Retriever {
    async fn retrieve(&self, key: &str) -> Result<String>;
}

struct SharedPeerState {
    peers: HashMap<Peer, ConnectedPeer>,
    ring: HashRing<VNode>,
}

impl SharedPeerState {

    fn peer_for_key(&self, key: &Key) -> Result<Peer> {
        let vnode = self.ring.get(key)
            .context("ring can't be empty")?;

        Ok(vnode.to_peer())
    }

    fn client_for_peer(&self, peer: &Peer) -> Result<PeerClient> {
        match self.peers.get(peer) {
            Some(peer) => { Ok(peer.client.clone()) }
            None => Err(anyhow!("peer not found"))
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

pub struct Groupcache {
    me: Peer,
    guarded_shared_state: Arc<RwLock<SharedPeerState>>,
    cache: Cache<Key, Value>,
    loader: Box<dyn ValueLoader>,
}

pub type Value = Vec<u8>;
pub type Key = String;

#[async_trait]
impl groupcache_server::Groupcache for Groupcache {
    async fn get(&self, request: Request<groupcache_pb::groupcache_pb::GetRequest>) ->
    std::result::Result<tonic::Response<groupcache_pb::groupcache_pb::GetResponse>, Status> {
        let payload = request.into_inner();
        info!("get key:{}", payload.key);

        match self.load_locally(&payload.key).await {
            Ok(value) => {
                Ok(tonic::Response::new(groupcache_pb::groupcache_pb::GetResponse {
                    value: Some(value)
                }))
            }
            Err(err) => {
                error!("Error during computing for key: {}, err: {}", payload.key, err);
                Err(Status::internal(err.to_string()))
            }
        }
    }
}


pub async fn start_grpc_server(
    groupcache: Arc<Groupcache>,
) -> Result<()> {
    let addr = groupcache.me.socket.clone();
    info!("Groupcache server listening on {}", addr);

    tonic::transport::Server::builder()
        .add_service(groupcache_server::GroupcacheServer::from_arc(groupcache))
        .serve(addr)
        .await?;

    Ok(())
}


#[async_trait]
pub trait ValueLoader: Send + Sync {
    async fn load(&self, key: &Key) -> std::result::Result<Value, Box<dyn std::error::Error + Send + Sync + 'static>>;
}


impl Groupcache {
    pub fn new(me: Peer,
               loader: Box<dyn ValueLoader>) -> Self {
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

        let guarded_shared_state = Arc::new(RwLock::new(SharedPeerState {
            peers,
            ring,
        }));

        Self {
            me,
            guarded_shared_state,
            cache,
            loader,
        }
    }

    pub async fn get(&self, key: &Key) -> Result<Value> {
        if let Some(value) = self.cache.get(key) {
            return Ok(value);
        }

        let peer = {
            let lock = self.guarded_shared_state
                .read().unwrap();

            lock
                .ring
                .get(&key)
                .context("no node found")?
                .to_peer()
        };
        log::info!("peer {:?} getting from peer: {:?}", self.me.socket, peer.socket);

        // todo: implement single-flight.
        // question: should be single-flight be implemented only on a local get
        // or also on remote gets?
        // Also for remote gets so that we don't incur unnecessary network overhead.
        let value =
            if peer == self.me {
                let value = self.load_locally(key).await?;
                self.cache.insert(key.clone(), value.clone());
                value
            } else {
                let mut client = {
                    let read_lock = self.guarded_shared_state.read().unwrap();
                    read_lock.client_for_peer(&peer)?
                };

                let response = client.get(GetRequest {
                    key: key.clone(),
                }.into_request()).await?;

                let get_response = response.into_inner();

                Ok(get_response.value)

            };

        Ok(value)
    }

    async fn load_locally(&self, key: &Key) -> Result<Value> {
        let v = self.loader.load(key).await.map_err(anyhow::Error::msg)?;
        Ok(v)
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
            let read_lock = self.guarded_shared_state
                .read()
                .unwrap();

            read_lock.contains_peer(&peer)
        };

        if contains_peer {
            return Ok(())
        }

        // todo: it should be up to the user to define whether we want to use http or https?
        // but then we'd also need to give ability to set up certs etc...
        let peer_server_address =
            format!("http://{}", peer.socket.clone().to_string());

        let client = GroupcacheClient::connect(peer_server_address).await?;

        let mut write_lock = self.guarded_shared_state
            .write()
            .unwrap();

        write_lock.add_peer(peer, client);
        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {}
}
