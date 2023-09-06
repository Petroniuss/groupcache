use anyhow::Result;
use anyhow::{anyhow, Context};
use async_trait::async_trait;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use axum::{Json, Router};
use groupcache::{start_grpc_server, GetResponseFailure, GroupcacheWrapper, Key, Peer};
use serde::{Deserialize, Serialize};
use std::env;
use std::net::{SocketAddr, ToSocketAddrs};
use std::time::Duration;
use tracing::{error, info, log};

#[derive(Clone, Deserialize, Serialize)]
struct CachedValue {
    plain_string: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();

    let port = env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()?;

    let groupcache_port = port;
    let axum_port = port + 5000;

    // todo: profile application
    // https://fasterthanli.me/articles/request-coalescing-in-async-rust

    // todo: prepare performance test for groupcache

    let groupcache = configure_groupcache(groupcache_port).await?;

    // todo: it should be possible to leave it up to the user to run groupcache on the same port as axum
    //      tldr; multiplex traffic to both application axum web service and groupcache.
    let _res = tokio::try_join!(
        run_groupcache(groupcache.clone()),
        axum(axum_port, groupcache.clone())
    )?;

    // todo: it should be possible to retrieve metrics from groupcache
    // todo: have a separate crate that exports metrics to prometheus

    Ok(())
}

async fn axum(port: u16, groupcache: GroupcacheWrapper<CachedValue>) -> Result<()> {
    let addr = format!("localhost:{port}")
        .to_socket_addrs()?
        .next()
        .unwrap();
    info!("Running axum on {}", addr);

    let app = Router::new()
        .route("/root", get(root))
        .route("/key/:key_id", get(get_key_handler))
        .route("/peer/:peer_addr", put(add_peer_handler))
        .with_state(groupcache);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

// todo: instead require types to be serializable to bytes using serde
// and MessagePack -> should be good enough for our purposes.
struct CacheLoader {}

#[async_trait]
impl groupcache::ValueLoader for CacheLoader {
    type Value = CachedValue;

    async fn load(
        &self,
        key: &Key,
    ) -> std::result::Result<Self::Value, Box<dyn std::error::Error + Send + Sync + 'static>> {
        use tokio::time::sleep;
        info!("Starting a long computation for {} .. about a 100ms.", key);

        sleep(Duration::from_millis(100)).await;

        return if key.contains("error") {
            Err(anyhow!("Something bad happened during loading :/").into())
        } else {
            Ok(CachedValue {
                plain_string: "bar".to_string(),
            })
        };
    }
}

async fn configure_groupcache(port: u16) -> Result<GroupcacheWrapper<CachedValue>> {
    let me = Peer {
        socket: format!("127.0.0.1:{}", port).parse()?,
    };

    let loader = CacheLoader {};

    let groupcache = GroupcacheWrapper::new(me, Box::new(loader));

    Ok(groupcache)
}

async fn run_groupcache(groupcache: GroupcacheWrapper<CachedValue>) -> Result<()> {
    start_grpc_server(groupcache)
        .await
        .context("failed to start server")?;

    Ok(())
}

#[derive(Serialize)]
struct GetResponse {
    key: String,
    value: String,
}

async fn get_key_handler(
    Path(key): Path<String>,
    State(groupcache): State<GroupcacheWrapper<CachedValue>>,
) -> Response {
    log::info!("get_rpc_handler, {}!", key);

    return match groupcache.get(&key).await {
        Ok(value) => {
            let value = value.plain_string;
            let response_body = GetResponse { key, value };
            (StatusCode::OK, Json(response_body)).into_response()
        }
        Err(error) => {
            error!("Received error from groupcache: {}", error.to_string());
            let response_body = GetResponseFailure {
                key,
                error: error.to_string(),
            };

            (StatusCode::INTERNAL_SERVER_ERROR, Json(response_body)).into_response()
        }
    };
}

async fn add_peer_handler(
    Path(peer_address): Path<String>,
    State(groupcache): State<GroupcacheWrapper<CachedValue>>,
) -> StatusCode {
    let Ok(socket) = peer_address.parse::<SocketAddr>() else {
        return StatusCode::BAD_REQUEST;
    };

    let Ok(_) = groupcache.add_peer(Peer { socket }).await else {
        return StatusCode::INTERNAL_SERVER_ERROR;
    };

    StatusCode::OK
}

// basic handler that responds with a static string
async fn root() -> &'static str {
    "Hello, World!"
}
