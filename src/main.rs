use std::env;
use std::sync::Arc;
use anyhow::Context;
use anyhow::Result;
use tracing::log::info;
use groupcache::{Peer, start_server};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let port = env::var("PORT").unwrap_or("3000".to_string());
    let me = Peer {
        socket: format!("127.0.0.1:{}", port).parse()?,
    };
    info!("starting server on {}", me.socket);

    let transport = groupcache::ReqwestTransport::new();
    let groupcache = Arc::new(groupcache::Groupcache::new(
        me, Box::new(transport))
    );
    start_server(groupcache)
        .await
        .context("failed to start server")?;

    Ok(())
}

// basic handler that responds with a static string
async fn root() -> &'static str {
    "Hello, World!"
}
