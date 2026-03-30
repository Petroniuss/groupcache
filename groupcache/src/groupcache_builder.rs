use crate::groupcache::ValueBounds;
use crate::options::Options;
use crate::service_discovery::run_service_discovery;
use crate::{Groupcache, GroupcacheInner, GroupcachePeer, ServiceDiscovery, ValueLoader};
use moka::future::Cache;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;
use tonic::transport::Endpoint;

/// Type alias for the cache key used by groupcache.
///
/// `Arc<str>` is used instead of `String` to avoid heap allocations when
/// cloning keys across the main cache, hot cache, and singleflight group.
pub type CacheKey = Arc<str>;

/// Allows to build groupcache instance with customized caches, timeouts etc
pub struct GroupcacheBuilder<Value: ValueBounds> {
    me: GroupcachePeer,
    loader: Box<dyn ValueLoader<Value = Value>>,
    cancel: CancellationToken,
    options: Options<Value>,
}

impl<Value: ValueBounds> GroupcacheBuilder<Value> {
    pub(crate) fn new(
        me: GroupcachePeer,
        loader: Box<impl ValueLoader<Value = Value> + Sized + 'static>,
        cancel: CancellationToken,
    ) -> Self {
        Self {
            me,
            loader,
            cancel,
            options: Options::default(),
        }
    }

    /// Sets main_cache for groupcache.
    ///
    /// main_cache is the cache for items that this peer is the owner of.
    /// There is one owner of a given key in a set of peers
    /// forming hash ring and this peer stores values in main_cache.
    ///
    /// By default, main_cache stores up to 100k items with no TTL.
    /// This setter allows to customize this cache (eviction policy, size etc.)
    pub fn main_cache(mut self, main_cache: Cache<CacheKey, Value>) -> Self {
        self.options.main_cache = main_cache;
        self
    }

    /// Sets hot_cache for groupcache.
    ///
    /// The hot_cache is the cache for items that seem popular
    /// enough to replicate to this node, even though it's not the owner.
    /// This however may lead to inconsistencies when expiring values (see [`Groupcache::remove`]).
    ///
    /// By default, hot_cache stores up to 10k items and expires after 30s.
    /// Depending on use_case you may either disable hot_cache or tweak time_to_live.
    pub fn hot_cache(mut self, hot_cache: Cache<CacheKey, Value>) -> Self {
        self.options.hot_cache = hot_cache;
        self
    }

    /// When connecting to peers, use https instead of http.
    ///
    /// Note that this requires you to configure server running [`Groupcache::grpc_service`] to handle TLS.
    /// Look into tonic/axum documentation how to do so.
    ///
    /// By default, http is used.
    pub fn https(mut self) -> Self {
        self.options.https = true;
        self
    }

    /// Set the timeout for gRPC requests to peer nodes.
    ///
    /// This controls how long a `get()` or `remove()` call waits for a
    /// response from the key's owner before failing. Default: 10 seconds.
    ///
    /// For datacenter deployments, a shorter timeout (e.g., 500ms) with
    /// fast failover to local load is recommended.
    pub fn grpc_timeout(mut self, timeout: std::time::Duration) -> Self {
        self.options.grpc_timeout = timeout;
        self
    }

    /// Allows to customize HTTP/2 channels for peer-to-peer connections.
    ///
    /// For most use cases, [`grpc_timeout`](Self::grpc_timeout) is sufficient.
    /// This method provides full control over the tonic `Endpoint`.
    pub fn grpc_endpoint_builder(
        mut self,
        builder: impl Fn(Endpoint) -> Endpoint + Send + Sync + 'static,
    ) -> Self {
        self.options.grpc_endpoint_builder = Box::new(builder);
        self
    }

    /// Enable streaming invalidation.
    ///
    /// When enabled, key owners push invalidation events to all peers via
    /// persistent gRPC streams. Peers clear their hot cache entries in
    /// near-realtime instead of waiting for TTL expiration.
    ///
    /// This also enables:
    /// - Hot cache flush on stream disconnect (prevents stale data after network issues)
    /// - Selective cache eviction on membership changes (peers joining/leaving)
    ///
    /// By default, streaming invalidation is disabled for backwards compatibility.
    pub fn enable_invalidation_streaming(mut self) -> Self {
        self.options.invalidation.enabled = true;
        self
    }

    /// Enable automatic pull-based service discovery via [`ServiceDiscovery`]
    ///
    /// By default, automatic service is disabled, useful when:
    ///     - deployed on kubernetes (see examples)
    ///     - registered in consul etc
    pub fn service_discovery(mut self, service_discovery: impl ServiceDiscovery + 'static) -> Self {
        self.options.service_discovery = Some(Box::new(service_discovery));
        self
    }

    pub fn build(mut self) -> Groupcache<Value> {
        let service_discovery = self.options.service_discovery.take();
        let cache = Groupcache(Arc::new(GroupcacheInner::new(
            self.me,
            self.loader,
            self.cancel.clone(),
            self.options,
        )));

        if let Some(service_discovery) = service_discovery {
            let handle = tokio::spawn(run_service_discovery(
                Arc::downgrade(&cache.0),
                service_discovery,
                self.cancel.clone(),
            ));
            cache.0.set_service_discovery_abort(handle.abort_handle());
        }

        cache
    }
}
