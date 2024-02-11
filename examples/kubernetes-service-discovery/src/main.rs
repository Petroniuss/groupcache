#![doc = include_str!("../readme.md")]
mod cache;
mod k8s;

use crate::cache::CachedValue;
use crate::k8s::Kubernetes;
use anyhow::Context;
use anyhow::Result;
use axum::extract::{Path, State};
use axum::http::header::CONTENT_TYPE;
use axum::http::{Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{any, get};
use axum::{Json, Router};
use groupcache::Groupcache;
use kube::Client;
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

/// kubernetes-service-discovery example
///
/// This example shows how to run a simple backend with groupcache with multiple instances on k8s.
/// Kubernetes API server is used for service discovery.
/// A simple endpoint is exposed `/key/:key_id` that loads a value for :key_id from groupcache,
/// which mimics a fetch from database lasting 100ms (see [cache::MockResourceLoader]).
///
/// If you want to run it locally, see readme.
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

    // Prometheus metrics
    let (prometheus_layer, metric_handle) = axum_prometheus::PrometheusMetricLayer::pair();

    // Groupcache instance, configured to respond to requests under `addr`
    // It doesn't by itself start a gRPC server, this is done later.
    let loader = cache::MockResourceLoader {};
    let client = Client::try_default().await?;

    let groupcache = Groupcache::builder(addr.into(), loader)
        .service_discovery(Kubernetes::builder().client(client).build())
        .build();

    // Example axum app with endpoint to retrieve value from groupcache.
    let axum_app = Router::new()
        .route("/", get(hello))
        .route("/hello", get(hello))
        .route("/key/:key_id", get(get_key_handler))
        .with_state(groupcache.clone())
        .route("/metrics", get(|| async move { metric_handle.render() }))
        .route("/*fallback", any(handler_404))
        .layer(prometheus_layer)
        .layer(trace())
        .boxed_clone();

    // Groupcache gRPC service, used for cross-peer communication if there are multiple peers in the cluster.
    let grpc_groupcache = tonic::transport::Server::builder()
        .add_service(groupcache.grpc_service())
        .into_service()
        .map_response(|r| r.map(axum::body::boxed))
        .map_err::<_, Infallible>(|_| panic!("unreachable - make the compiler happy"))
        .boxed_clone();

    // Create a service that can respond to Web and gRPC depending on content type header.
    // This is needed because groupcache itself communicates with its peers over gRPC.
    // Alternatively, grpc server could be run on a separate port from the application.
    let http_grpc = Steer::new(
        vec![axum_app, grpc_groupcache],
        |req: &Request<_>, _svcs: &[_]| {
            let content_type = req.headers().get(CONTENT_TYPE).map(|v| v.as_bytes());
            usize::from(
                content_type == Some(b"application/grpc")
                    || content_type == Some(b"application/grpc+proto"),
            )
        },
    );

    info!("Listening on addr: {}", addr);
    let bind_addr = format!("0.0.0.0:{}", pod_port).parse()?;
    axum::Server::bind(&bind_addr)
        .serve(Shared::new(http_grpc))
        .await
        .context("Failed to start axum server")?;

    Ok(())
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

/// Simple HTTP handler that forwards request to groupcache and returns a JSON response.
async fn get_key_handler(
    Path(key): Path<String>,
    State(groupcache): State<Groupcache<CachedValue>>,
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

async fn handler_404() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "nothing to see here\n")
}

/// Hello world endpoint
async fn hello() -> &'static str {
    "Hello from groupcache-powered-backend-service!\n"
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

fn read_env(env_var_name: &'static str) -> Result<String> {
    env::var(env_var_name).context(format!("Failed to read: '{}' env variable", env_var_name))
}
