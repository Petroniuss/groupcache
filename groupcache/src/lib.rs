extern crate serde;
extern crate anyhow;

use std::error::Error;
use std::future::Future;
use std::net::IpAddr;
use serde::{Deserialize, Serialize};
use anyhow::Result;

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

pub trait GroupCacheBuilder<S: GroupCache> {
    fn with_max_size(&mut self, max_bytes: u64);

    fn with_value_retriever<AsyncFn, Fut>(&mut self, retriever: AsyncFn)
        where
            AsyncFn: Fn() -> Fut,
            Fut: Future<Output=Result<String>>
    ;

    fn with_tokio_axum_transport(&mut self);

    fn with_custom_transport(&mut self, transport: impl Transport);

    fn build() -> S;
}


pub trait GroupCache {
    fn get(&self, key: &str) -> Result<String>;

    fn set(&self, key: &str, value: &str) -> Result<()>;

    // or set_peers
    fn add_peer(&self, node: &Peer) -> Result<()>;

    fn remove_peer(&self, node: &Peer) -> Result<()>;
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {}
}
