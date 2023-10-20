mod cache;

use crate::cache::{configure_groupcache, CachedValue};
use anyhow::Context;
use anyhow::Result;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, put};
use axum::{Json, Router};
use groupcache::GroupcacheWrapper;
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use std::env;
use std::net::SocketAddr;
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

    let pod_port = read_env("K8S_POD_PORT")?;
    let pod_ip = read_env("K8S_POD_IP")?;
    let pod_name = read_env("K8S_POD_NAME")?;
    let namespace = read_env("K8S_NAMESPACE")?;
    info!(
        r#" Running {}:
            K8S_POD_IP: {},
            K8S_POD_PORT: {},
            K8S_NAMESPACE: {},
        "#,
        pod_name, pod_ip, pod_port, namespace
    );

    let addr: SocketAddr = format!("{}:{}", pod_ip, pod_port).parse()?;

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

    info!("Listening on addr: {}", addr);
    axum::Server::bind(&addr)
        .serve(Shared::new(http_grpc))
        .await
        .context("Failed to start axum server")?;

    Ok(())
}

fn read_env(env_var_name: &'static str) -> Result<String> {
    env::var(env_var_name).context(format!("Failed to read: '{}' env variable", env_var_name))
}

// async fn kube() -> Result<()> {
//     let namespace = "kube-system";
//     let client = Client::try_default().await?;
//     let services: Api<Pod> = Api::namespaced(client, namespace);
//     let service_name = "etcd-minikube";
//
//     // Fetch the service object
//     let service = services.get(service_name).await?;
//
//     dbg!(service);
//     Ok(())
// }

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
