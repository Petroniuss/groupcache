use std::sync::Arc;
use anyhow::Context;
use anyhow::Result;
use groupcache::start_server;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let groupcache = Arc::new(groupcache::Groupcache::new());
    start_server(groupcache)
        .await
        .context("failed to start server")?;

    Ok(())
}

// basic handler that responds with a static string
async fn root() -> &'static str {
    "Hello, World!"
}
