use anyhow::Result;
use async_trait::async_trait;
use groupcache::moka::future::CacheBuilder;
use groupcache::Groupcache;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

use std::net::SocketAddr;
use std::time::Duration;

use tokio::time::sleep;
use tonic::transport::Server;
use tower_http::classify::{ServerErrorsAsFailures, SharedClassifier};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tower_http::LatencyUnit;
use tracing::{info, Level};

/// In this example we simulate a distributed system
/// by running multiple servers inside a single process.
#[tokio::main]
async fn main() -> Result<()> {
    setup_logging();

    // Here we spawn two instances listening on these sockets.
    let addr1 = "127.0.0.1:8081".parse::<SocketAddr>()?;
    let addr2 = "127.0.0.1:8082".parse::<SocketAddr>()?;

    let server_1 = spawn_groupcache_instance_on_addr(addr1).await?;
    let server_2 = spawn_groupcache_instance_on_addr(addr2).await?;

    // We now perform manual service discovery:
    // we notify instance_1 about instance_2 and vice versa
    let peers_1 = HashSet::from([addr2.into()]);
    server_1.set_peers(peers_1).await?;

    let peers_2 = HashSet::from([addr1.into()]);
    server_2.set_peers(peers_2).await?;

    // If you run the following example,
    // you'll notice that the system computes values for 'some-key' only once:
    // the request goes from server_1 -> server_2 (via gRPC)
    // where the value is computed and cached for subsequent access.
    let _v = server_1.get("some-key").await?;

    // When we perform this request we get value of out of cache.
    let _v = server_2.get("some-key").await?;

    Ok(())
}

// We need to derive Serialize/Deserialize
// since we need to transfer this data across the network.
#[derive(Clone, Deserialize, Serialize)]
pub struct CachedValue {
    pub plain_string: String,
}

pub struct ComputeProtectedValue {
    addr: String,
}

#[async_trait]
impl groupcache::ValueLoader for ComputeProtectedValue {
    type Value = CachedValue;

    // Logic for loading cached values
    async fn load(
        &self,
        key: &str,
    ) -> std::result::Result<Self::Value, Box<dyn std::error::Error + Send + Sync + 'static>> {
        info!(
            "Starting a long computation on {} for {} .. about a 100ms.",
            self.addr, key
        );
        sleep(Duration::from_millis(100)).await;

        Ok(CachedValue {
            plain_string: format!("Plain value for key: '{}'", key),
        })
    }
}

type ExampleGroupcache = Groupcache<CachedValue>;

pub async fn spawn_groupcache_instance_on_addr(addr: SocketAddr) -> Result<ExampleGroupcache> {
    let loader = ComputeProtectedValue {
        addr: addr.to_string(),
    };

    let groupcache = Groupcache::builder(addr.into(), loader)
        .hot_cache(
            CacheBuilder::default()
                .time_to_live(Duration::from_secs(10))
                .build(),
        )
        .build();

    let server = groupcache.grpc_service();
    tokio::spawn(async move {
        Server::builder()
            .layer(trace())
            .add_service(server)
            .serve(addr)
            .await
            .unwrap();
    });

    Ok(groupcache)
}

fn setup_logging() {
    tracing_subscriber::fmt().pretty().init();
}

/// gives us nice access logs
fn trace() -> TraceLayer<SharedClassifier<ServerErrorsAsFailures>> {
    TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
        .on_request(DefaultOnRequest::new().level(Level::INFO))
        .on_response(
            DefaultOnResponse::new()
                .level(Level::INFO)
                .latency_unit(LatencyUnit::Millis),
        )
}
