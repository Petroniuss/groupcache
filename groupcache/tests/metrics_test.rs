#![allow(clippy::items_after_test_module)]
mod common;
use anyhow::{Context, Result};
use common::*;
use metrics::{
    Counter, CounterFn, Gauge, Histogram, Key, KeyName, Metadata, Recorder, SharedString, Unit,
};
use pretty_assertions::assert_eq;
use rstest::{fixture, rstest};
use std::cell::RefCell;
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::runtime::{Builder, Runtime};

#[fixture]
pub fn runtime() -> Runtime {
    Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("should be able to stat a runtime")
}

#[fixture]
pub fn recorder() -> MockRecorder {
    MockRecorder::new()
}

pub async fn groupcache() -> TestGroupcache {
    single_instance()
        .await
        .expect("running single groupcache instance should succeed")
}

pub async fn groupcache_tuple() -> (TestGroupcache, TestGroupcache) {
    two_connected_instances()
        .await
        .expect("running two groupcache instances should succeed")
}

#[rstest]
#[case::get("groupcache_get_total")]
#[case::local_load("groupcache_local_load_total")]
pub fn metrics_on_fresh_local_load(
    recorder: MockRecorder,
    runtime: Runtime,
    #[case] metric_name: &str,
) -> Result<()> {
    metrics::with_local_recorder(&recorder, || {
        let _ = runtime.block_on(async move {
            let groupcache = groupcache().await;
            groupcache.get("foo").await
        });

        let counter_value = recorder
            .counter_value(metric_name)
            .context(format!("metric '{}' should be set", metric_name))
            .unwrap();

        assert_eq!(1, counter_value);
    });

    Ok(())
}

#[rstest]
#[case::load_error("groupcache_get_total")]
#[case::load_error("groupcache_local_load_errors")]
pub fn metrics_on_load_failure(
    recorder: MockRecorder,
    runtime: Runtime,
    #[case] metric_name: &str,
) -> Result<()> {
    metrics::with_local_recorder(&recorder, || {
        runtime.block_on(async move {
            let groupcache = groupcache().await;
            let res = groupcache.get("error").await;
            assert!(res.is_err());
        });

        let counter_value = recorder
            .counter_value(metric_name)
            .context(format!("metric '{}' should be set", metric_name))?;

        assert_eq!(1, counter_value);
        Ok(())
    })
}

#[rstest]
#[case::get("groupcache_get_total", 2)]
#[case::local_load("groupcache_local_load_total", 1)]
#[case::cache_hit("groupcache_local_cache_hit_total", 1)]
pub fn metrics_on_cached_local_load(
    recorder: MockRecorder,
    runtime: Runtime,
    #[case] metric_name: &str,
    #[case] expected_count: u64,
) -> Result<()> {
    metrics::with_local_recorder(&recorder, || {
        runtime.block_on(async move {
            let groupcache = groupcache().await;
            let _ = groupcache.get("same-key").await.expect("should succeed");
            let _ = groupcache.get("same-key").await.expect("should succeed");
        });

        let counter_value = recorder
            .counter_value(metric_name)
            .context(format!("metric '{}' should be set", metric_name))?;

        assert_eq!(expected_count, counter_value);
        Ok(())
    })
}

#[rstest]
#[case::get("groupcache_get_total", 2)]
#[case::get_server_reqs("groupcache_get_server_requests_total", 1)]
#[case::remote_loads("groupcache_remote_load_total", 1)]
pub fn metrics_on_remote_load_test(
    recorder: MockRecorder,
    runtime: Runtime,
    #[case] metric_name: &str,
    #[case] expected_count: u64,
) -> Result<()> {
    metrics::with_local_recorder(&recorder, || {
        runtime.block_on(async move {
            let _groupcache = groupcache().await;
            let (instance_one, instance_two) = groupcache_tuple().await;
            let key = key_owned_by_instance(instance_two.clone());
            successful_get(&key, Some("2"), instance_one.clone()).await;
        });

        let counter_value = recorder
            .counter_value(metric_name)
            .context(format!("metric '{}' should be set", metric_name))?;

        assert_eq!(
            expected_count, counter_value,
            "metric: {} was {}",
            metric_name, counter_value
        );

        Ok(())
    })
}

#[rstest]
#[case::get("groupcache_get_total", 2)]
#[case::get_server_reqs("groupcache_get_server_requests_total", 1)]
#[case::remote_loads("groupcache_remote_load_total", 1)]
#[case::remote_load_errors("groupcache_remote_load_errors", 1)]
pub fn metrics_on_remote_load_failure(
    recorder: MockRecorder,
    runtime: Runtime,
    #[case] metric_name: &str,
    #[case] expected_count: u64,
) -> Result<()> {
    metrics::with_local_recorder(&recorder, || {
        runtime.block_on(async move {
            let (instance_one, instance_two) = groupcache_tuple().await;
            let key = error_key_on_instance(instance_two.clone());

            let res = instance_one.get(&key).await;
            assert!(res.is_err(), "get should fail");
        });

        let counter_value = recorder
            .counter_value(metric_name)
            .context(format!("metric '{}' should be set", metric_name))?;

        assert_eq!(
            expected_count, counter_value,
            "metric: {} was {}",
            metric_name, counter_value
        );

        Ok(())
    })
}

pub struct MockRecorder {
    registered_counters: RefCell<HashMap<String, Arc<MockCounter>>>,
}

impl MockRecorder {
    fn new() -> Self {
        Self {
            registered_counters: RefCell::new(Default::default()),
        }
    }

    fn counter_value(&self, key: &str) -> Option<u64> {
        let counters = self.registered_counters.borrow();
        return counters.get(key).map(|c| {
            let guard = c.count.lock().unwrap();
            *guard
        });
    }
}

#[derive(Clone)]
struct MockCounter {
    count: Arc<Mutex<u64>>,
}

impl MockCounter {
    fn new() -> Self {
        Self {
            count: Arc::new(Mutex::new(0)),
        }
    }
}

impl CounterFn for MockCounter {
    fn increment(&self, value: u64) {
        let mut guard = self.count.lock().unwrap();
        *guard += value;
    }

    fn absolute(&self, value: u64) {
        let mut guard = self.count.lock().unwrap();
        *guard = value;
    }
}

impl Recorder for MockRecorder {
    fn describe_counter(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn describe_gauge(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn describe_histogram(&self, _key: KeyName, _unit: Option<Unit>, _description: SharedString) {}

    fn register_counter(&self, key: &Key, _metadata: &Metadata<'_>) -> Counter {
        match self
            .registered_counters
            .borrow_mut()
            .entry(key.name().to_string())
        {
            Entry::Occupied(e) => {
                let mock_counter = e.get().clone();
                Counter::from(mock_counter)
            }
            Entry::Vacant(e) => {
                let mock_counter = Arc::new(MockCounter::new());
                e.insert(mock_counter.clone());

                Counter::from(mock_counter)
            }
        }
    }

    fn register_gauge(&self, _key: &Key, _metadata: &Metadata<'_>) -> Gauge {
        todo!()
    }

    fn register_histogram(&self, _key: &Key, _metadata: &Metadata<'_>) -> Histogram {
        todo!()
    }
}
