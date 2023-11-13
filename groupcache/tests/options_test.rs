use groupcache::OptionsBuilder;
use moka::future::CacheBuilder;
use std::time::Duration;

mod common;

#[test]
fn options_api_test() {
    let main_cache = CacheBuilder::default().max_capacity(100).build();
    let hot_cache = CacheBuilder::default()
        .max_capacity(10)
        .time_to_live(Duration::from_secs(10))
        .build();

    let _options = OptionsBuilder::<String>::default()
        .main_cache(main_cache)
        .hot_cache(hot_cache)
        .grpc_endpoint_builder(Box::new(|endpoint| {
            endpoint.timeout(Duration::from_secs(2))
        }))
        .https()
        .build();
}
