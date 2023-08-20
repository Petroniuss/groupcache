use std::env;
use std::net::{IpAddr, Ipv4Addr, ToSocketAddrs};
use std::os::unix::net::SocketAddr;
use std::sync::Arc;
use anyhow::Context;
use anyhow::Result;
use axum::Router;
use axum::routing::get;
use tracing::log::info;
use groupcache::{Peer, start_grpc_server};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let port = env::var("PORT")
        .unwrap_or("3000".to_string())
        .parse::<u16>()?;


    let groupcache_port = port;
    let axum_port = port + 5000;

    let res =
        tokio::try_join!(groupcache(groupcache_port), axum(axum_port))?;

    Ok(())
}

async fn axum(port: u16) -> Result<()> {
    let addr = format!("localhost:{port}").to_socket_addrs()?.next().unwrap();

    let app = Router::new()
        .route("/get/:key_id", get(root));

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

async fn groupcache(port: u16) -> Result<()> {
    let me = Peer {
        socket: format!("127.0.0.1:{}", port).parse()?,
    };

    let transport = groupcache::ReqwestTransport::new();
    let groupcache = Arc::new(groupcache::Groupcache::new(
        me, Box::new(transport))
    );
    start_grpc_server(groupcache)
        .await
        .context("failed to start server")?;

    Ok(())
}

// basic handler that responds with a static string
async fn root() -> &'static str {
    "Hello, World!"
}
