extern crate serde;
extern crate anyhow;
extern crate async_trait;

use std::error::Error;
use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::str::FromStr;
use std::sync::Arc;
use serde::{Deserialize, Serialize};
use anyhow::Result;
use async_trait::async_trait;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::{Json, Router};
use axum::routing::get;
use hashring::HashRing;
use tracing::log;
use tracing::log::log;

static VNODES_PER_PEER: i32 = 10;

// let's start with defining a couple of traits.
pub struct Peer {
    pub socket: SocketAddr,
}

#[derive(Debug, Copy, Clone, Hash, PartialEq)]
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

pub trait Transport {
    fn get_rpc(&self, peer: &Peer, req: &GetRequest) -> Result<GetResponse>;
}


#[async_trait]
trait Retriever {
    async fn retrieve(&self, key: &str) -> Result<String>;
}

pub struct Groupcache {
    me: Peer,
    ring: HashRing<VNode>,
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
    pub fn new(me: Peer) -> Self {
        let mut ring= HashRing::new();

        let vnodes = VNode::vnodes_for_peer(&me, VNODES_PER_PEER);
        for vnode in vnodes {
            ring.add(vnode)
        }

        Self {
            me,
            ring
        }
    }


    fn get(&self, key: &str) -> Result<String> {
        Err(anyhow::anyhow!("not implemented"))
    }
    // or set_peers
    fn add_peer(&self, node: &Peer) -> Result<()> {
        todo!();
    }

    fn remove_peer(&self, node: &Peer) -> Result<()> {
        todo!();
    }
}

async fn get_rpc_handler(
    Path(key_id): Path<String>,
    State(groupcache): State<Arc<Groupcache>>,
) -> Response {
    log::info!("get_rpc_handler, {}!", key_id);

    return match groupcache.get(&key_id) {
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
