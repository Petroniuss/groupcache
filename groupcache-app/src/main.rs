use anyhow::Result;
use anyhow::{anyhow, Context};
use async_trait::async_trait;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use axum::{Json, Router};
use groupcache::{GroupcacheWrapper, Key};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::env;
use std::net::SocketAddr;
use std::time::Duration;
use tower::make::Shared;
use tower::steer::Steer;
use tower::ServiceExt;
use tower_http::classify::{ServerErrorsAsFailures, SharedClassifier};
use tower_http::trace::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tower_http::LatencyUnit;
use tracing::{error, info, log, Level};

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let port = env::var("PORT")
        .unwrap_or_else(|_| "3000".to_string())
        .parse::<u16>()?;
    let addr: SocketAddr = format!("127.0.0.1:{}", port).parse()?;

    // Groupcache instance, configured to respond to requests under `addr`
    let groupcache = configure_groupcache(addr).await?;

    // Example axum app with endpoints to add groupcache peers and retrieve values from groupcache.
    let axum_app = Router::new()
        .route("/root", get(root))
        .route("/key/:key_id", get(get_key_handler))
        .route("/peer/:peer_addr", put(add_peer_handler))
        .with_state(groupcache.clone())
        .layer(trace())
        .boxed_clone();

    // Groupcache gRPC service, used for cross-peer communication if there are multiple peers in the cluster.
    let grpc_groupcache = tonic::transport::Server::builder()
        .add_service(groupcache.grpc_service())
        .into_service()
        .map_response(|r| r.map(axum::body::boxed))
        .map_err::<_, Infallible>(|_| panic!("unreachable - make the compiler happy"))
        .boxed_clone();

    // Create a service that can respond to Web and gRPC
    let http_grpc = Steer::new(
        vec![axum_app, grpc_groupcache],
        |req: &Request<Body>, _svcs: &[_]| {
            let content_type = req.headers().get(CONTENT_TYPE).map(|v| v.as_bytes());
            usize::from(
                content_type == Some(b"application/grpc")
                    || content_type == Some(b"application/grpc+proto"),
            )
        },
    );

    axum::Server::bind(&addr)
        .serve(Shared::new(http_grpc))
        .await
        .context("Failed to start axum server")?;

    Ok(())
}

struct CacheLoader {}

#[derive(Clone, Deserialize, Serialize)]
struct CachedValue {
    plain_string: String,
}

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

async fn configure_groupcache(socket: SocketAddr) -> Result<GroupcacheWrapper<CachedValue>> {
    let loader = CacheLoader {};
    let groupcache = GroupcacheWrapper::new(socket.into(), Box::new(loader));

    Ok(groupcache)
}

fn trace() -> TraceLayer<SharedClassifier<ServerErrorsAsFailures>> {
    TraceLayer::new_for_http()
        .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
        .on_request(DefaultOnRequest::new().level(Level::INFO))
        .on_response(
            DefaultOnResponse::new()
                .level(Level::INFO)
                .latency_unit(LatencyUnit::Micros),
        )
}

#[derive(Serialize)]
struct GetResponse {
    key: String,
    value: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct GetResponseFailure {
    pub key: String,
    pub error: String,
}

async fn get_key_handler(
    Path(key): Path<String>,
    State(groupcache): State<GroupcacheWrapper<CachedValue>>,
) -> Response {
    log::info!("get_rpc_handler, {}!", key);

    match groupcache.get(&key).await {
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
    }
}

async fn add_peer_handler(
    Path(peer_address): Path<String>,
    State(groupcache): State<GroupcacheWrapper<CachedValue>>,
) -> StatusCode {
    let Ok(socket) = peer_address.parse::<SocketAddr>() else {
        return StatusCode::BAD_REQUEST;
    };

    let Ok(_) = groupcache.add_peer(socket.into()).await else {
        return StatusCode::INTERNAL_SERVER_ERROR;
    };

    StatusCode::OK
}

async fn root() -> &'static str {
    "Hello, World!"
}
