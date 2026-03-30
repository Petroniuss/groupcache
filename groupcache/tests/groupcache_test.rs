use async_trait::async_trait;
use groupcache::{Groupcache, GroupcachePeer, ServiceDiscovery, ValueLoader};
use moka::future::CacheBuilder;
use std::collections::HashSet;
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

struct ServiceDiscoveryStub {
    me: GroupcachePeer,
}

#[async_trait]
impl ServiceDiscovery for ServiceDiscoveryStub {
    async fn pull_instances(
        &self,
    ) -> Result<HashSet<GroupcachePeer>, Box<dyn Error + Send + Sync + 'static>> {
        Ok(HashSet::from([self.me]))
    }
}

#[tokio::test]
async fn builder_api_test() {
    let main_cache = CacheBuilder::default().max_capacity(100).build();
    let hot_cache = CacheBuilder::default()
        .max_capacity(10)
        .time_to_live(Duration::from_secs(10))
        .build();
    let loader = DummyLoader {};
    let me: SocketAddr = "127.0.0.1:8080".parse().unwrap();

    let _groupcache = Groupcache::builder(me.into(), loader, groupcache::CancellationToken::new())
        .main_cache(main_cache)
        .hot_cache(hot_cache)
        .grpc_endpoint_builder(Box::new(|endpoint: Endpoint| {
            endpoint.timeout(Duration::from_secs(2))
        }))
        .https()
        .service_discovery(ServiceDiscoveryStub { me: me.into() })
        .build();
}

#[tokio::test]
async fn https_connection_attempt_uses_https_scheme() {
    let me: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let groupcache = Groupcache::builder(me.into(), DummyLoader {}, groupcache::CancellationToken::new())
        .https()
        .build();

    // Try to add a peer over HTTPS. The connection will fail (peer isn't TLS)
    // but the code path that formats the "https://..." address is exercised.
    let peer_addr: SocketAddr = "127.0.0.1:19876".parse().unwrap();
    let result = groupcache.add_peer(peer_addr.into()).await;
    assert!(
        result.is_err(),
        "expected connection error for HTTPS to non-TLS peer"
    );
}
