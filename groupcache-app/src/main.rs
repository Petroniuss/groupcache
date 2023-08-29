use anyhow::Context;
use anyhow::Result;
use async_trait::async_trait;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use axum::{Json, Router};
use groupcache::{start_grpc_server, GetResponseFailure, Groupcache, Key, Peer};
use serde::Serialize;
use std::env;
use std::error::Error;
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::Arc;
use std::time::Duration;
use tracing::{info, log};

#[derive(Clone)]
struct Value {
    v: String
}

impl Into<Vec<u8>> for Value {
    fn into(self) -> Vec<u8> {
        self.v.into_bytes()
    }
}

impl From<Vec<u8>> for Value {
    fn from(value: Vec<u8>) -> Self {
        Value {
            v: String::from_utf8(value).unwrap()
        }
    }

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

async fn axum(port: u16, groupcache: Arc<Groupcache<Value>>) -> Result<()> {
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

struct CacheLoader {}

#[async_trait]
impl groupcache::ValueLoader for CacheLoader {
    type Value = Value;

    async fn load(
        &self,
        key: &Key,
    ) -> std::result::Result<Self::Value, Box<dyn Error + Send + Sync + 'static>> {
        use tokio::time::sleep;
        info!("Starting a long computation.. about a 100ms.");

        sleep(Duration::from_millis(100)).await;

        Ok(Value { v: format!("{}-v", key) })
    }
}

async fn configure_groupcache(port: u16) -> Result<Arc<Groupcache<Value>>> {
    let me = Peer {
        socket: format!("127.0.0.1:{}", port).parse()?,
    };

    let loader = CacheLoader {};

    // todo: groupcache should be already wrapped within an Arc.
    // user shouldn't have to do this :)
    let groupcache = Arc::new(Groupcache::new(me, Box::new(loader)));

    Ok(groupcache)
}

async fn run_groupcache(groupcache: Arc<Groupcache<Value>>) -> Result<()> {
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
    State(groupcache): State<Arc<Groupcache<Value>>>,
) -> Response {
    log::info!("get_rpc_handler, {}!", key);

    return match groupcache.get(&key).await {
        Ok(value) => {
            let value = value.v;
            let response_body = GetResponse { key, value };
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

async fn add_peer_handler(
    Path(peer_address): Path<String>,
    State(groupcache): State<Arc<Groupcache<Value>>>,
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
