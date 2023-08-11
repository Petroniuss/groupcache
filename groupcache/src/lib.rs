extern crate serde;
extern crate anyhow;
extern crate async_trait;
extern crate quick_cache;

use std::error::Error;
use std::future::Future;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use serde::{Deserialize, Serialize};
use anyhow::{bail, Context, Result};
use async_trait::async_trait;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use axum::routing::get;
use hashring::HashRing;
use tracing::log;
use tracing::log::log;
use quick_cache::sync::Cache;


static VNODES_PER_PEER: i32 = 10;

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
    key: String,
    error: String,
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

pub struct Groupcache {
    me: Peer,
    peers: RwLock<Vec<Peer>>,
    ring: Arc<RwLock<HashRing<VNode>>>,
    cache: Cache<String, String>,
    transport: Box<dyn Transport>,
}

pub async fn start_server(
    groupcache: Arc<Groupcache>,
) -> Result<()> {
    let app = Router::new()
        .route("/get/:key_id", get(get_rpc_handler))
        .with_state(groupcache);

    axum::Server::bind(&SocketAddr::from(([127, 0, 0, 1], 3000)))
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

impl Groupcache {
    pub fn new(me: Peer, transport: Box<dyn Transport>) -> Self {
        let ring = {
            let mut ring = HashRing::new();
            let vnodes = VNode::vnodes_for_peer(&me, VNODES_PER_PEER);
            for vnode in vnodes {
                ring.add(vnode)
            }

            Arc::new(RwLock::new(ring))
        };

        let cache = Cache::<String, String>::new(1_000_000);
        let peers = RwLock::new(vec![me.clone()]);

        Self {
            me,
            peers,
            ring,
            cache,
            transport,
        }
    }

    async fn get(&self, key: &str) -> Result<String> {
        let peer = {
            let lock = self.ring.read()
                .unwrap();

            let vnode = lock
                .get(&key)
                .context("no node found")?;
            Peer { socket: vnode.addr.clone() }
        };

        let GetResponse { key, value } = self.transport.get_rpc(&peer, &GetRequest {
            key: key.to_string(),
        }).await.context("failed to retrieve kv from peer")?;

        self.cache.insert(key, value.clone());
        Ok(value)
    }

    fn add_peer(&self, peer: Peer) -> Result<()> {
        if !self.peers.read().unwrap().contains(&peer) {
            self.peers.write().unwrap().push(peer.clone());
            let vnodes = VNode::vnodes_for_peer(&peer, VNODES_PER_PEER);
            let mut lock = self.ring.write().unwrap();
            for vnode in vnodes {
                lock.add(vnode);
            }
        } else {
            bail!("peer already exists");
        }

        todo!();
    }
}

async fn get_rpc_handler(
    Path(key_id): Path<String>,
    State(groupcache): State<Arc<Groupcache>>,
) -> Response {
    log::info!("get_rpc_handler, {}!", key_id);

    return match groupcache.get(&key_id).await {
        Ok(value) => {
            let response_body = GetResponse {
                key: key_id,
                value,
            };
            (StatusCode::OK, Json(response_body)).into_response()
        }
        Err(error) => {
            let response_body = GetResponseFailure {
                key: key_id,
                error: error.to_string(),
            };

            (StatusCode::INTERNAL_SERVER_ERROR, Json(response_body)).into_response()
        }
    };
}

pub struct GroupcacheBuilderImpl {
    max_bytes: u64,
    retriever: Box<dyn Retriever>,
    transport: Box<dyn Transport>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {}
}
