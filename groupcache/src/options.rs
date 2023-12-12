use crate::service_discovery::ServiceDiscovery;
use crate::ValueBounds;
use moka::future::Cache;
use std::time::Duration;
use tonic::transport::Endpoint;

static DEFAULT_HOT_CACHE_MAX_CAPACITY: u64 = 10_000;
static DEFAULT_HOT_CACHE_TIME_TO_LIVE: Duration = Duration::from_secs(30);
static DEFAULT_GRPC_CLIENT_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

pub(crate) struct Options<Value: ValueBounds> {
    pub(crate) main_cache: Cache<String, Value>,
    pub(crate) hot_cache: Cache<String, Value>,
    pub(crate) grpc_endpoint_builder: Box<dyn Fn(Endpoint) -> Endpoint + Send + Sync + 'static>,
    pub(crate) https: bool,
    pub(crate) service_discovery: Option<Box<dyn ServiceDiscovery + Send + Sync + 'static>>,
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
            service_discovery: None,
        }
    }
}
