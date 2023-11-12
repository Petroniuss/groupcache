use crate::ValueBounds;
use moka::future::Cache;
use std::time::Duration;
use tonic::transport::Endpoint;

pub struct Options<Value: ValueBounds> {
    pub(crate) main_cache: Cache<String, Value>,
    pub(crate) hot_cache: Cache<String, Value>,
    pub(crate) grpc_endpoint_builder: Box<dyn Fn(Endpoint) -> Endpoint + Send + Sync + 'static>,
    pub(crate) https: bool,
}

pub struct OptionsBuilder<Value: ValueBounds> {
    main_cache: Option<Cache<String, Value>>,
    hot_cache: Option<Cache<String, Value>>,
    grpc_endpoint_builder: Option<Box<dyn Fn(Endpoint) -> Endpoint + Send + Sync + 'static>>,
    https: Option<bool>,
}

impl<Value: ValueBounds> Default for OptionsBuilder<Value> {
    fn default() -> Self {
        OptionsBuilder::new()
    }
}

impl<Value: ValueBounds> OptionsBuilder<Value> {
    pub fn new() -> Self {
        Self {
            main_cache: None,
            hot_cache: None,
            grpc_endpoint_builder: None,
            https: None,
        }
    }

    pub fn main_cache(mut self, main_cache: Cache<String, Value>) -> Self {
        self.main_cache = Some(main_cache);
        self
    }

    pub fn hot_cache(mut self, hot_cache: Cache<String, Value>) -> Self {
        self.hot_cache = Some(hot_cache);
        self
    }

    pub fn grpc_endpoint_builder(
        mut self,
        builder: Box<dyn Fn(Endpoint) -> Endpoint + Send + Sync + 'static>,
    ) -> Self {
        self.grpc_endpoint_builder = Some(builder);
        self
    }

    pub fn https(mut self, value: bool) -> Self {
        self.https = Some(value);
        self
    }

    pub fn build(self) -> Options<Value> {
        Options::from(self)
    }
}

impl<Value: ValueBounds> Default for Options<Value> {
    fn default() -> Self {
        let main_cache = Cache::<String, Value>::builder().build();

        let hot_cache = Cache::<String, Value>::builder()
            .max_capacity(10_000)
            .time_to_live(Duration::from_secs(30))
            .build();

        let grpc_endpoint_builder = Box::new(|e: Endpoint| e.timeout(Duration::from_secs(10)));

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
            https: builder.https.unwrap_or(default.https),
        }
    }
}
