use std::time::Duration;
use tonic::transport::Endpoint;

pub struct Options {
    pub(crate) main_cache_capacity: u64,
    pub(crate) hot_cache_capacity: u64,
    pub(crate) hot_cache_ttl: Duration,
    pub(crate) https: bool,
    pub(crate) grpc_endpoint_builder: Box<dyn Fn(Endpoint) -> Endpoint + Send + Sync + 'static>,
}

#[derive(Default)]
pub struct OptionsBuilder {
    main_cache_capacity: Option<u64>,
    hot_cache_capacity: Option<u64>,
    hot_cache_ttl: Option<Duration>,
    https: Option<bool>,
    grpc_endpoint_builder: Option<Box<dyn Fn(Endpoint) -> Endpoint + Send + Sync + 'static>>,
}

impl OptionsBuilder {
    pub fn new() -> Self {
        Self {
            main_cache_capacity: None,
            hot_cache_capacity: None,
            hot_cache_ttl: None,
            https: None,
            grpc_endpoint_builder: None,
        }
    }

    pub fn main_cache_capacity(mut self, capacity: u64) -> Self {
        self.main_cache_capacity = Some(capacity);
        self
    }

    pub fn hot_cache_capacity(mut self, capacity: u64) -> Self {
        self.hot_cache_capacity = Some(capacity);
        self
    }

    pub fn hot_cache_ttl(mut self, ttl: Duration) -> Self {
        self.hot_cache_ttl = Some(ttl);
        self
    }

    pub fn https(mut self, value: bool) -> Self {
        self.https = Some(value);
        self
    }

    pub fn grpc_endpoint_builder(
        mut self,
        builder: Box<dyn Fn(Endpoint) -> Endpoint + Send + Sync + 'static>,
    ) -> Self {
        self.grpc_endpoint_builder = Some(builder);
        self
    }

    pub fn build(self) -> Options {
        Options::from(self)
    }
}

impl Default for Options {
    fn default() -> Self {
        Self {
            main_cache_capacity: 100_000,
            hot_cache_capacity: 10_000,
            hot_cache_ttl: Duration::from_secs(30),
            https: false,
            grpc_endpoint_builder: Box::new(|e| e.timeout(Duration::from_secs(2))),
        }
    }
}

impl From<OptionsBuilder> for Options {
    fn from(builder: OptionsBuilder) -> Self {
        let default = Options::default();

        Self {
            main_cache_capacity: builder
                .main_cache_capacity
                .unwrap_or(default.main_cache_capacity),
            hot_cache_capacity: builder
                .hot_cache_capacity
                .unwrap_or(default.hot_cache_capacity),
            hot_cache_ttl: builder.hot_cache_ttl.unwrap_or(default.hot_cache_ttl),
            https: builder.https.unwrap_or(default.https),
            grpc_endpoint_builder: builder
                .grpc_endpoint_builder
                .unwrap_or(default.grpc_endpoint_builder),
        }
    }
}
