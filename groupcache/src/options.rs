use crate::ValueBounds;
use moka::future::Cache;
use std::time::Duration;
use tonic::transport::Endpoint;

static DEFAULT_HOT_CACHE_MAX_CAPACITY: u64 = 10_000;
static DEFAULT_HOT_CACHE_TIME_TO_LIVE: Duration = Duration::from_secs(30);
static DEFAULT_GRPC_CLIENT_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

/// [`Options`] are used to customize groupcache.
///
/// In order to construct [`Options`] use [`OptionsBuilder`].
pub struct Options<Value: ValueBounds> {
    pub(crate) main_cache: Cache<String, Value>,
    pub(crate) hot_cache: Cache<String, Value>,
    pub(crate) grpc_endpoint_builder: Box<dyn Fn(Endpoint) -> Endpoint + Send + Sync + 'static>,
    pub(crate) https: bool,
}

/// [`OptionsBuilder`] builds [`Options`].
/// See available methods to see what can be tweaked.
pub struct OptionsBuilder<Value: ValueBounds> {
    main_cache: Option<Cache<String, Value>>,
    hot_cache: Option<Cache<String, Value>>,
    grpc_endpoint_builder: Option<Box<dyn Fn(Endpoint) -> Endpoint + Send + Sync + 'static>>,
    https: bool,
}

impl<Value: ValueBounds> OptionsBuilder<Value> {
    /// Constructs [`OptionsBuilder`] with default values.
    ///
    /// Not all values have to bet set, unset values will use [`Options::default`].
    pub fn new() -> Self {
        Self {
            main_cache: None,
            hot_cache: None,
            grpc_endpoint_builder: None,
            https: false,
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
        self.main_cache = Some(main_cache);
        self
    }

    /// Sets hot_cache for groupcache.
    ///
    /// The hot_cache is the cache for items that seem popular
    /// enough to replicate to this node, even though it's not the owner.
    /// This however may lead to inconsistencies when expiring values (see [`crate::GroupcacheWrapper::remove`]).
    ///
    /// By default hot_cache stores up to 10k items and expires after 30s.
    /// Depending on use_case you may either disable hot_cache or tweak time_to_live.
    pub fn hot_cache(mut self, hot_cache: Cache<String, Value>) -> Self {
        self.hot_cache = Some(hot_cache);
        self
    }

    /// Allows to customize HTTP/2 channels for peer-to-peer connections.
    ///
    /// By default, request timeout is set to 10 seconds.
    pub fn grpc_endpoint_builder(
        mut self,
        builder: Box<dyn Fn(Endpoint) -> Endpoint + Send + Sync + 'static>,
    ) -> Self {
        self.grpc_endpoint_builder = Some(builder);
        self
    }

    /// When connecting to peers, use https instead of http.
    ///
    /// Note that this requires you to configure server running [`crate::GroupcacheWrapper::grpc_service`] to handle TLS.
    /// Look into tonic/axum documentation how to do so.
    ///
    /// By default http is used.
    pub fn https(mut self) -> Self {
        self.https = true;
        self
    }

    pub fn build(self) -> Options<Value> {
        Options::from(self)
    }
}

impl<Value: ValueBounds> Default for OptionsBuilder<Value> {
    fn default() -> Self {
        OptionsBuilder::new()
    }
}

impl<Value: ValueBounds> Default for Options<Value> {
    fn default() -> Self {
        let main_cache = Cache::<String, Value>::builder().build();

        let hot_cache = Cache::<String, Value>::builder()
            .max_capacity(DEFAULT_HOT_CACHE_MAX_CAPACITY)
            .time_to_live(DEFAULT_HOT_CACHE_TIME_TO_LIVE)
            .build();

        let grpc_endpoint_builder =
            Box::new(|e: Endpoint| e.timeout(DEFAULT_GRPC_CLIENT_REQUEST_TIMEOUT));

        Self {
            main_cache,
            hot_cache,
            grpc_endpoint_builder,
            https: false,
        }
    }
}

impl<Value: ValueBounds> From<OptionsBuilder<Value>> for Options<Value> {
    fn from(builder: OptionsBuilder<Value>) -> Self {
        let default = Options::default();

        Self {
            main_cache: builder.main_cache.unwrap_or(default.main_cache),
            hot_cache: builder.hot_cache.unwrap_or(default.hot_cache),
            grpc_endpoint_builder: builder
                .grpc_endpoint_builder
                .unwrap_or(default.grpc_endpoint_builder),
            https: builder.https,
        }
    }
}
