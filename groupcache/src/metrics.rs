//! Metric names emitted by groupcache via the [`metrics`](https://docs.rs/metrics) facade.
//!
//! Groupcache records counters using the `metrics` crate. To collect them,
//! install a metrics recorder in your application. Common options:
//!
//! - [`metrics-exporter-prometheus`](https://docs.rs/metrics-exporter-prometheus)
//! - [`axum-prometheus`](https://docs.rs/axum-prometheus)
//!
//! All metric names are exported as public constants so applications can
//! reference them in dashboards, alerts, or tests.
//!
//! # Example
//!
//! ```no_run
//! // Install a Prometheus recorder at startup:
//! // let builder = metrics_exporter_prometheus::PrometheusBuilder::new();
//! // builder.install().expect("failed to install recorder");
//!
//! // Then groupcache metrics are automatically collected.
//! // Use the constants to build queries:
//! let hit_rate_query = format!(
//!     "rate({}[5m]) / rate({}[5m])",
//!     groupcache::metrics::LOCAL_CACHE_HIT_TOTAL,
//!     groupcache::metrics::GET_TOTAL,
//! );
//! ```

// ── Core operation counters ─────────────────────────────────────────

/// Total number of get operations (cache hits + misses + remote fetches).
pub const GET_TOTAL: &str = "groupcache_get_total";

/// Total number of get requests received from remote peers via gRPC.
pub const GET_SERVER_REQUESTS_TOTAL: &str = "groupcache_get_server_requests_total";

/// Total number of local cache hits (main cache or hot cache).
/// No network call or loader invocation was needed.
pub const LOCAL_CACHE_HIT_TOTAL: &str = "groupcache_local_cache_hit_total";

/// Total number of times the [`ValueLoader`](crate::ValueLoader) was called.
pub const LOCAL_LOAD_TOTAL: &str = "groupcache_local_load_total";

/// Total number of [`ValueLoader`](crate::ValueLoader) failures.
pub const LOCAL_LOAD_ERROR_TOTAL: &str = "groupcache_local_load_errors";

/// Total number of remote fetches (gRPC calls to the key's owner).
pub const REMOTE_LOAD_TOTAL: &str = "groupcache_remote_load_total";

/// Total number of remote fetch failures.
pub const REMOTE_LOAD_ERROR_TOTAL: &str = "groupcache_remote_load_errors";

/// Total number of remove (invalidation) operations initiated.
pub const REMOVE_TOTAL: &str = "groupcache_remove_total";

// ── Streaming invalidation counters ─────────────────────────────────

/// Total number of invalidation events broadcast by this node (as key owner).
/// Only incremented when streaming invalidation is enabled.
pub const INVALIDATION_BROADCAST_TOTAL: &str = "groupcache_invalidation_broadcast_total";

/// Total number of invalidation events received from remote peers via stream.
pub const INVALIDATION_RECEIVED_TOTAL: &str = "groupcache_invalidation_received_total";

/// Total number of invalidation stream disconnects.
/// Each disconnect triggers a full hot cache flush as a safety measure.
pub const INVALIDATION_STREAM_DISCONNECT_TOTAL: &str =
    "groupcache_invalidation_stream_disconnect_total";

// ── Internal aliases (preserve old names for use inside the crate) ──

pub(crate) use GET_TOTAL as METRIC_GET_TOTAL;
pub(crate) use GET_SERVER_REQUESTS_TOTAL as METRIC_GET_SERVER_REQUESTS_TOTAL;
pub(crate) use LOCAL_CACHE_HIT_TOTAL as METRIC_LOCAL_CACHE_HIT_TOTAL;
pub(crate) use LOCAL_LOAD_TOTAL as METRIC_LOCAL_LOAD_TOTAL;
pub(crate) use LOCAL_LOAD_ERROR_TOTAL as METRIC_LOCAL_LOAD_ERROR_TOTAL;
pub(crate) use REMOTE_LOAD_TOTAL as METRIC_REMOTE_LOAD_TOTAL;
pub(crate) use REMOTE_LOAD_ERROR_TOTAL as METRIC_REMOTE_LOAD_ERROR;
pub(crate) use REMOVE_TOTAL as METRIC_REMOVE_TOTAL;
pub(crate) use INVALIDATION_BROADCAST_TOTAL as METRIC_INVALIDATION_BROADCAST_TOTAL;
pub(crate) use INVALIDATION_RECEIVED_TOTAL as METRIC_INVALIDATION_RECEIVED_TOTAL;
pub(crate) use INVALIDATION_STREAM_DISCONNECT_TOTAL as METRIC_INVALIDATION_STREAM_DISCONNECT_TOTAL;
