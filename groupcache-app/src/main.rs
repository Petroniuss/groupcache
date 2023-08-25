use std::env;
use std::net::ToSocketAddrs;
use std::sync::Arc;
use anyhow::Context;
use anyhow::Result;
use axum::extract::{Path, State};
use axum::{Json, Router};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use serde::{Deserialize, Serialize};
use tracing::{info, log};
use groupcache::{GetResponseFailure, Groupcache, Peer, start_grpc_server};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let port = env::var("PORT")
        .unwrap_or("3000".to_string())
        .parse::<u16>()?;


    let groupcache_port = port;
    let axum_port = port + 5000;

    let groupcache = configure_groupcache(groupcache_port).await?;

    let res =
        tokio::try_join!(run_groupcache(groupcache.clone()), axum(axum_port, groupcache.clone()))?;

    Ok(())
}

async fn axum(port: u16, groupcache: Arc<Groupcache>) -> Result<()> {
    let addr = format!("localhost:{port}").to_socket_addrs()?.next().unwrap();
    info!("Running axum on {}", addr);

    let app = Router::new()
        .route("/root", get(root))
        .route("/get/:key_id", get(get_key_handler))
        .with_state(groupcache);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

async fn configure_groupcache(port: u16) -> Result<Arc<Groupcache>> {
    let me = Peer {
        socket: format!("127.0.0.1:{}", port).parse()?,
    };

    let transport = groupcache::ReqwestTransport::new();
    let groupcache = Arc::new(groupcache::Groupcache::new(
        me, Box::new(transport))
    );

    Ok(groupcache)
}

async fn run_groupcache(groupcache: Arc<Groupcache>) -> Result<()> {
    start_grpc_server(groupcache)
        .await
        .context("failed to start server")?;

    Ok(())
}

#[derive(Serialize)]
struct GetResponse {
    key: String,
    value: String
}

async fn get_key_handler(
    Path(key): Path<String>,
    State(groupcache): State<Arc<Groupcache>>,
) -> Response {
    log::info!("get_rpc_handler, {}!", key);

    return match groupcache.get(&key).await {
        Ok(value) => {
            let value = String::from_utf8(value).unwrap();
            let response_body = GetResponse {
                key,
                value,
            };
            (StatusCode::OK, Json(response_body)).into_response()
        }
        Err(error) => {
            let response_body = GetResponseFailure {
                key,
                error: error.to_string(),
            };

            (StatusCode::INTERNAL_SERVER_ERROR, Json(response_body)).into_response()
        }
    };
}
// basic handler that responds with a static string
async fn root() -> &'static str {
    "Hello, World!"
}
