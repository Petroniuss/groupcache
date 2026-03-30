#![allow(dead_code)]

use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use groupcache::{Groupcache, GroupcachePeer, ServiceDiscovery};
use moka::future::CacheBuilder;
use pretty_assertions::assert_eq;
use std::collections::HashMap;
use std::collections::HashSet;
use std::error::Error;
use std::future::{pending, Future};
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::sync::oneshot::{Receiver, Sender};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::codegen::tokio_stream;
use tonic::transport::Server;

pub static OS_ALLOCATED_PORT_ADDR: &str = "127.0.0.1:0";
pub static HOT_CACHE_TTL: Duration = Duration::from_millis(100);

/// Small delay to allow invalidation events to propagate over gRPC streams.
/// This is NOT a TTL wait -- streams deliver in sub-millisecond, but we need
/// to yield to the tokio runtime for the watcher task to process the event.
pub static INVALIDATION_PROPAGATION_DELAY: Duration = Duration::from_millis(50);

pub fn key_owned_by_instance(instance: TestGroupcache) -> String {
    format!("{}_0", instance.addr())
}

pub fn error_key_on_instance(instance: TestGroupcache) -> String {
    format!("{}_13", instance.addr())
}

pub async fn two_instances() -> Result<(TestGroupcache, TestGroupcache)> {
    let instance_one = spawn_groupcache("1").await?;
    let instance_two = spawn_groupcache("2").await?;

    Ok((instance_one, instance_two))
}

pub async fn spawn_instances(n: usize) -> Result<Vec<TestGroupcache>> {
    let mut instances = Vec::new();
    for i in 0..n {
        let instance = spawn_groupcache(&i.to_string()).await?;
        instances.push(instance);
    }

    let first_instance = &instances[0];
    for instance in instances.iter().skip(1) {
        first_instance.add_peer(instance.addr().into()).await?;
    }

    Ok(instances)
}

pub async fn single_instance() -> Result<TestGroupcache> {
    spawn_groupcache("1").await
}

pub async fn two_connected_instances() -> Result<(TestGroupcache, TestGroupcache)> {
    let (instance_one, instance_two) = two_instances().await?;

    instance_one.add_peer(instance_two.addr().into()).await?;
    instance_two.add_peer(instance_one.addr().into()).await?;

    Ok((instance_one, instance_two))
}

pub async fn reconnect(instance: TestGroupcache) {
    let listener = TcpListener::bind(instance.addr()).await.unwrap();
    tokio::spawn(async move {
        Server::builder()
            .add_service(instance.grpc_service())
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
}

pub async fn two_instances_with_one_disconnected() -> Result<(TestGroupcache, TestGroupcache)> {
    let (shutdown_signal, shutdown_recv) = tokio::sync::oneshot::channel::<()>();
    let (shutdown_done_s, shutdown_done_r) = tokio::sync::oneshot::channel::<()>();
    pub async fn shutdown_proxy(shutdown_signal: Receiver<()>, shutdown_done: Sender<()>) {
        shutdown_signal.await.unwrap();
        shutdown_done.send(()).unwrap();
    }

    let instance_one = spawn_groupcache("1").await?;
    let instance_two = spawn_groupcache_instance(
        "2",
        OS_ALLOCATED_PORT_ADDR,
        shutdown_proxy(shutdown_recv, shutdown_done_s),
    )
    .await?;

    instance_one.add_peer(instance_two.addr().into()).await?;
    instance_two.add_peer(instance_one.addr().into()).await?;

    shutdown_signal.send(()).unwrap();
    shutdown_done_r.await.unwrap();

    Ok((instance_one, instance_two))
}

pub async fn spawn_groupcache(instance_id: &str) -> Result<TestGroupcache> {
    spawn_groupcache_instance(instance_id, OS_ALLOCATED_PORT_ADDR, pending()).await
}

/// Spawn a groupcache instance with a ServiceDiscovery implementation attached.
pub async fn spawn_groupcache_with_service_discovery(
    instance_id: &str,
    sd: impl ServiceDiscovery + 'static,
) -> Result<TestGroupcache> {
    let listener = TcpListener::bind(OS_ALLOCATED_PORT_ADDR).await.unwrap();
    let addr = listener.local_addr()?;
    let groupcache = Groupcache::builder(addr.into(), TestCacheLoader::new(instance_id), groupcache::CancellationToken::new())
        .hot_cache(CacheBuilder::default().time_to_live(HOT_CACHE_TTL).build())
        .enable_invalidation_streaming()
        .service_discovery(sd)
        .build();

    let server = groupcache.grpc_service();
    tokio::spawn(async move {
        Server::builder()
            .add_service(server)
            .serve_with_incoming_shutdown(TcpListenerStream::new(listener), pending::<()>())
            .await
            .unwrap();
    });

    Ok(groupcache)
}

/// A test implementation of ServiceDiscovery that returns a configurable set of peers.
pub struct TestServiceDiscovery {
    pub peers: Arc<RwLock<HashSet<GroupcachePeer>>>,
    pub poll_interval: Duration,
    pub error_on_next: Arc<RwLock<bool>>,
}

impl TestServiceDiscovery {
    pub fn new(peers: HashSet<GroupcachePeer>, poll_interval: Duration) -> Self {
        Self {
            peers: Arc::new(RwLock::new(peers)),
            poll_interval,
            error_on_next: Arc::new(RwLock::new(false)),
        }
    }
}

#[async_trait]
impl ServiceDiscovery for TestServiceDiscovery {
    async fn pull_instances(
        &self,
    ) -> std::result::Result<HashSet<GroupcachePeer>, Box<dyn Error + Send + Sync + 'static>> {
        let should_error = { *self.error_on_next.read().unwrap() };
        if should_error {
            return Err("Simulated service discovery error".into());
        }
        let peers = self.peers.read().unwrap().clone();
        Ok(peers)
    }

    fn interval(&self) -> Duration {
        self.poll_interval
    }
}

/// A ServiceDiscovery that uses the default interval() implementation (10s).
/// Used to cover the default trait method.
pub struct DefaultIntervalServiceDiscovery {
    pub peers: HashSet<GroupcachePeer>,
}

#[async_trait]
impl ServiceDiscovery for DefaultIntervalServiceDiscovery {
    async fn pull_instances(
        &self,
    ) -> std::result::Result<HashSet<GroupcachePeer>, Box<dyn Error + Send + Sync + 'static>> {
        Ok(self.peers.clone())
    }
    // interval() is NOT overridden, exercising the default 10s implementation
}

pub async fn spawn_groupcache_instance(
    instance_id: &str,
    addr: &str,
    shutdown_signal: impl Future<Output = ()> + Send + 'static,
) -> Result<TestGroupcache> {
    let listener = TcpListener::bind(addr).await.unwrap();
    let addr = listener.local_addr()?;
    let groupcache = Groupcache::builder(addr.into(), TestCacheLoader::new(instance_id), groupcache::CancellationToken::new())
        .hot_cache(CacheBuilder::default().time_to_live(HOT_CACHE_TTL).build())
        .enable_invalidation_streaming()
        .build();

    let server = groupcache.grpc_service();
    tokio::spawn(async move {
        Server::builder()
            .add_service(server)
            .serve_with_incoming_shutdown(TcpListenerStream::new(listener), shutdown_signal)
            .await
            .unwrap();
    });

    Ok(groupcache)
}

pub async fn success_or_transport_err(key: &str, groupcache: TestGroupcache) {
    let result = groupcache.get(key).await;
    match result {
        Ok(v) => {
            assert_eq!(v.contains(key), true);
        }
        Err(e) => {
            let error_string = e.to_string();
            assert_eq!(
                error_string.contains("Transport"),
                true,
                "expected transport error, got: '{}'",
                error_string
            );
        }
    }
}

#[derive(Default)]
pub struct GetAssertions {
    pub expected_instance_id: Option<String>,
    pub unexpected_instance_id: Option<String>,
    pub expected_load_count: Option<i32>,
}

pub async fn successful_get_not_from_instance(
    key: &str,
    unexpected_instance_id: &str,
    groupcache: TestGroupcache,
) {
    let opts = GetAssertions {
        unexpected_instance_id: Some(unexpected_instance_id.to_string()),
        ..GetAssertions::default()
    };

    successful_get_opts(key, groupcache, opts).await;
}

pub async fn successful_get(
    key: &str,
    expected_instance_id: Option<&str>,
    groupcache: TestGroupcache,
) {
    let opts = GetAssertions {
        expected_instance_id: expected_instance_id.map(|s| s.to_string()),
        ..GetAssertions::default()
    };

    successful_get_opts(key, groupcache, opts).await;
}

pub async fn successful_get_opts(key: &str, groupcache: TestGroupcache, opts: GetAssertions) {
    let v = groupcache.get(key).await.expect("get should be successful");

    assert_eq!(
        v.contains(key),
        true,
        "expected value to be '{}', got: '{}'",
        key,
        v
    );
    if let Some(instance) = opts.expected_instance_id {
        assert_eq!(
            v.contains(&format!("INSTANCE_{}", instance)),
            true,
            "expected instance id to be '{}', got: '{}'",
            instance,
            v
        );
    }

    if let Some(load) = opts.expected_load_count {
        assert_eq!(
            v.contains(&format!("LOAD_{}", load)),
            true,
            "expected load count to be '{}', got: '{}'",
            load,
            v,
        );
    }
}

pub type TestGroupcache = Groupcache<CachedValue>;

pub type CachedValue = String;

pub struct TestCacheLoader {
    instance_id: String,
    load_counter: Arc<RwLock<HashMap<String, i32>>>,
}

impl TestCacheLoader {
    pub fn new(instance_id: &str) -> Self {
        Self {
            instance_id: instance_id.to_string(),
            load_counter: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub fn count_loads(&self, key: &str) -> Result<i32> {
        let mut lock = self.load_counter.write().unwrap();
        let counter = lock.entry(key.to_string()).or_insert(0);
        *counter += 1;

        Ok(*counter)
    }
}

#[async_trait]
impl groupcache::ValueLoader for TestCacheLoader {
    type Value = CachedValue;

    async fn load(
        &self,
        key: &str,
    ) -> std::result::Result<Self::Value, Box<dyn std::error::Error + Send + Sync + 'static>> {
        let load_counter = self.count_loads(key)?;
        return if !key.contains("error") && !key.contains("_13") {
            Ok(format!(
                "VAL_INSTANCE_{}_KEY_{}_LOAD_{}",
                self.instance_id, key, load_counter
            ))
        } else {
            Err(anyhow!("Something bad happened during loading :/").into())
        };
    }
}
