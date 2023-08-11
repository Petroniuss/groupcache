extern crate serde;
extern crate anyhow;
extern crate async_trait;

use std::error::Error;
use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use serde::{Deserialize, Serialize};
use anyhow::Result;
use async_trait::async_trait;
use axum::Router;

// let's start with defining a couple of traits.
pub struct Peer {
    ip: IpAddr,
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

pub trait Transport {
    fn get_rpc(&self, peer: &Peer, req: &GetRequest) -> Result<GetResponse>;
}


#[async_trait]
trait Retriever {
    async fn retrieve(&self, key: &str) -> Result<String>;
}

pub struct Groupcache {
    port: u16,
    base_url: String,
}

impl Groupcache {
    async fn start_server(&self) -> Result<()> {
        // todo: make sure this is called only once.
        let app = Router::new()
            .with_state(1337);

        axum::Server::bind(&SocketAddr::from(([127, 0, 0, 1], self.port)))
            .serve(app.into_make_service())
            .await?;

        Ok(())
    }

    fn get(&self, key: &str) -> Result<String> {
        todo!()
    }
    // or set_peers
    fn add_peer(&self, node: &Peer) -> Result<()> {
        todo!();
    }

    fn remove_peer(&self, node: &Peer) -> Result<()> {
        todo!();
    }
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
