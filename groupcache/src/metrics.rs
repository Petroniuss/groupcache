//! metrics module contains metric names along with description what each metric counts.
//!
//!
//! Metrics are exported via `metrics` create.
//! It's up to the application to ingest these metrics, some options:
//! - `metrics-exporter-prometheus`,
//! - `axum-prometheus`.
//!
//! For meaning of each metric, see below.

/// Any GET request, including from peers
pub(crate) const METRIC_GET_TOTAL: &str = "groupcache_get_total";

/// GETs that came over the network from peers
pub(crate) const METRIC_GET_SERVER_REQUESTS_TOTAL: &str = "groupcache_get_server_requests_total";

/// Local cache hit (without going over the network or loading a value using [crate::groupcache::ValueLoader])
pub(crate) const METRIC_LOCAL_CACHE_HIT_TOTAL: &str = "groupcache_local_cache_hit_total";

/// Total calls to [`crate::groupcache::ValueLoader::load`]
pub(crate) const METRIC_LOCAL_LOAD_TOTAL: &str = "groupcache_local_load_total";

/// Total number of failures of [`crate::groupcache::ValueLoader::load`]
pub(crate) const METRIC_LOCAL_LOAD_ERROR_TOTAL: &str = "groupcache_local_load_errors";

/// Total number of remote GETs:
/// - peer is not the owner and needs to make HTTP request to the owner for a given key.
pub(crate) const METRIC_REMOTE_LOAD_TOTAL: &str = "groupcache_remote_load_total";

/// Total number of remote GET failures.
pub(crate) const METRIC_REMOTE_LOAD_ERROR: &str = "groupcache_remote_load_errors";
