use anyhow::Result;
use async_trait::async_trait;
use groupcache::Groupcache;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::time::sleep;
use tracing::info;

/// This example is rather trivial and shows how groupcache deduplicates concurrent requests.
/// You'd typically run groupcache on multiple instances, see other examples how to do it.
#[tokio::main]
async fn main() -> Result<()> {
    setup_logging();

    // groupcache needs address of this instance - required for routing requests.
    // we don't need to spin http server in our simplified example
    // since all requests will be served by this instance
    let addr: SocketAddr = format!("{}:{}", "127.0.0.1", "8080").parse()?;

    // need to tell groupcache how to load values
    let loader = ComputeProtectedValue;

    // we crate groupcache with only a single peer - this process
    let groupcache = Groupcache::builder(addr.into(), loader).build();

    // we make 3 concurrent requests for hot key
    let key = "some-hot-requested-key";
    let mut handles = Vec::new();
    for i in 0..3 {
        let g = groupcache.clone();
        let handle = tokio::spawn(async move {
            info!(
                "{}. Trying to compute the value concurrently for {}",
                i, key
            );
            g.get(key).await
        });

        handles.push(handle);
    }

    // And here we wait for the results...
    // logs will tell you that we've computed this value only once.
    for (i, handle) in handles.into_iter().enumerate() {
        let v = handle.await??;
        info!("{}. Computation completed with: '{}'", i, v.plain_string);
    }

    Ok(())
}

// We need to derive Serialize/Deserialize
// since we'd usually need to transfer this data across the network.
#[derive(Clone, Deserialize, Serialize)]
pub struct CachedValue {
    pub plain_string: String,
}

pub struct ComputeProtectedValue;

#[async_trait]
impl groupcache::ValueLoader for ComputeProtectedValue {
    type Value = CachedValue;

    // Logic for loading cached values
    async fn load(
        &self,
        key: &str,
    ) -> std::result::Result<Self::Value, Box<dyn std::error::Error + Send + Sync + 'static>> {
        info!("Starting a long computation for {} .. about a 100ms.", key);
        sleep(Duration::from_millis(100)).await;

        Ok(CachedValue {
            plain_string: format!("Plain value for key: '{}'", key),
        })
    }
}

fn setup_logging() {
    tracing_subscriber::fmt().pretty().init();
}
