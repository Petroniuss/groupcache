use async_trait::async_trait;
use groupcache::{Groupcache, ValueLoader};
use moka::future::CacheBuilder;
use std::error::Error;
use std::net::SocketAddr;
use std::time::Duration;
use tonic::transport::Endpoint;

mod common;

struct DummyLoader {}

#[async_trait]
impl ValueLoader for DummyLoader {
    type Value = u32;
    async fn load(
        &self,
        _key: &str,
    ) -> Result<Self::Value, Box<dyn Error + Send + Sync + 'static>> {
        Ok(42)
    }
}

#[test]
fn builder_api_test() {
    let main_cache = CacheBuilder::default().max_capacity(100).build();
    let hot_cache = CacheBuilder::default()
        .max_capacity(10)
        .time_to_live(Duration::from_secs(10))
        .build();
    let loader = DummyLoader {};
    let me: SocketAddr = "127.0.0.1:8080".parse().unwrap();

    let _groupcache = Groupcache::builder(me.into(), loader)
        .main_cache(main_cache)
        .hot_cache(hot_cache)
        .grpc_endpoint_builder(Box::new(|endpoint: Endpoint| {
            endpoint.timeout(Duration::from_secs(2))
        }))
        .https()
        .build();
}
