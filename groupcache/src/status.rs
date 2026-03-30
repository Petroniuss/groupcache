//! Health and status reporting for groupcache.

/// Health state of the groupcache instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthState {
    /// All systems operational, at least one peer connected.
    Healthy,
    /// Running but no peers connected yet.
    /// Normal during startup or for single-node deployments.
    NoPeers,
    /// Shutting down — cancellation token was cancelled.
    ShuttingDown,
}

/// Snapshot of the groupcache instance's current state.
///
/// Returned by [`Groupcache::status()`](crate::Groupcache::status).
/// The library reports state; the application decides policy.
///
/// # Example
///
/// ```no_run
/// # async fn example(groupcache: groupcache::Groupcache<String>) {
/// use groupcache::status::HealthState;
///
/// let status = groupcache.status();
/// match status.health {
///     HealthState::Healthy | HealthState::NoPeers => {
///         // respond 200 OK to health check
///     }
///     HealthState::ShuttingDown => {
///         // respond 503 Service Unavailable
///     }
/// }
/// # }
/// ```
#[derive(Debug, Clone)]
pub struct Status {
    /// Overall health state.
    pub health: HealthState,
    /// Number of connected remote peers (excludes self).
    pub peer_count: usize,
    /// Approximate number of entries in the main cache (keys this node owns).
    pub main_cache_size: u64,
    /// Approximate number of entries in the hot cache (replicated keys).
    pub hot_cache_size: u64,
}
