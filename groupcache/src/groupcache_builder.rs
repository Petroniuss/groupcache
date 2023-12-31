use crate::groupcache::ValueBounds;
use crate::options::Options;
use crate::service_discovery::run_service_discovery;
use crate::{Groupcache, GroupcacheInner, GroupcachePeer, ServiceDiscovery, ValueLoader};
use moka::future::Cache;
use std::sync::Arc;
use tonic::transport::Endpoint;

/// Allows to build groupcache instance with customized caches, timeouts etc
pub struct GroupcacheBuilder<Value: ValueBounds> {
    me: GroupcachePeer,
    loader: Box<dyn ValueLoader<Value = Value>>,
    options: Options<Value>,
}

impl<Value: ValueBounds> GroupcacheBuilder<Value> {
    pub(crate) fn new(
        me: GroupcachePeer,
        loader: Box<impl ValueLoader<Value = Value> + Sized + 'static>,
    ) -> Self {
        Self {
            me,
            loader,
            options: Options::default(),
        }
    }

    /// Sets main_cache for groupcache.
    ///
    /// main_cache is the cache for items that this peer is the owner of.
    /// There is one owner of a given key in a set of peers
    /// forming hash ring and this peer stores values in main_cache.
    ///
    /// By default,
    /// [moka] cache is used and this setter allows to customize this cache (eviction policy, size etc)
    pub fn main_cache(mut self, main_cache: Cache<String, Value>) -> Self {
        self.options.main_cache = main_cache;
        self
    }

    /// Sets hot_cache for groupcache.
    ///
    /// The hot_cache is the cache for items that seem popular
    /// enough to replicate to this node, even though it's not the owner.
    /// This however may lead to inconsistencies when expiring values (see [`crate::groupcache::Groupcache::remove`]).
    ///
    /// By default hot_cache stores up to 10k items and expires after 30s.
    /// Depending on use_case you may either disable hot_cache or tweak time_to_live.
    pub fn hot_cache(mut self, hot_cache: Cache<String, Value>) -> Self {
        self.options.hot_cache = hot_cache;
        self
    }

    /// When connecting to peers, use https instead of http.
    ///
    /// Note that this requires you to configure server running [`crate::groupcache::Groupcache::grpc_service`] to handle TLS.
    /// Look into tonic/axum documentation how to do so.
    ///
    /// By default http is used.
    pub fn https(mut self) -> Self {
        self.options.https = true;
        self
    }

    /// Allows to customize HTTP/2 channels for peer-to-peer connections.
    ///
    /// By default, request timeout is set to 10 seconds.
    pub fn grpc_endpoint_builder(
        mut self,
        builder: impl Fn(Endpoint) -> Endpoint + Send + Sync + 'static,
    ) -> Self {
        self.options.grpc_endpoint_builder = Box::new(builder);
        self
    }

    /// Enable automatic service discovery.
    ///
    /// By default automatic service is disabled.
    pub fn service_discovery(mut self, service_discovery: impl ServiceDiscovery + 'static) -> Self {
        self.options.service_discovery = Some(Box::new(service_discovery));
        self
    }

    pub fn build(mut self) -> Groupcache<Value> {
        let service_discovery = self.options.service_discovery.take();
        let cache = Groupcache(Arc::new(GroupcacheInner::new(
            self.me,
            self.loader,
            self.options,
        )));

        if let Some(service_discovery) = service_discovery {
            tokio::spawn(run_service_discovery(
                Arc::downgrade(&cache.0),
                service_discovery,
            ));
        }

        cache
    }
}
