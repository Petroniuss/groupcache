use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use groupcache::{GroupcacheWrapper, Key};
use pretty_assertions::assert_eq;
use std::future::{pending, Future};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::sync::oneshot::{Receiver, Sender};
use tokio_stream::wrappers::TcpListenerStream;
use tonic::codegen::tokio_stream;
use tonic::transport::Server;

static OS_ALLOCATED_PORT_ADDR: &str = "127.0.0.1:0";

#[tokio::test]
async fn test_when_there_is_only_one_peer_it_should_handle_entire_key_space() -> Result<()> {
    let groupcache = {
        let loader = TestCacheLoader::new("1");
        let addr: SocketAddr = "127.0.0.1:8080".parse()?;
        GroupcacheWrapper::<CachedValue>::new(addr.into(), Box::new(loader))
    };

    let key = "K-some-random-key-d2k";
    successful_get(key, Some("1"), groupcache.clone()).await;
    successful_get(key, Some("1"), groupcache.clone()).await;

    let error_key = "Key-triggering-loading-error";
    let err = groupcache
        .get(error_key)
        .await
        .expect_err("expected error from loader");

    assert_eq!(
        "Loading error: 'Something bad happened during loading :/'",
        err.to_string()
    );

    Ok(())
}

#[tokio::test]
async fn test_when_peers_are_healthy_they_should_respond_to_queries() -> Result<()> {
    let (instance_one, instance_two) = two_connected_instances().await?;

    for &key in ["K-b3a", "K-karo", "K-x3d", "K-d42", "W-a1a"].iter() {
        successful_get(key, None, instance_one.clone()).await;
        successful_get(key, None, instance_two.clone()).await;
    }

    Ok(())
}

#[tokio::test]
async fn test_when_peer_disconnects_requests_should_fail_with_transport_error() -> Result<()> {
    let (instance_one, instance_two) = two_instances_with_one_disconnected().await?;
    for &key in ["K-b3a", "K-karo", "K-x3d", "K-d42", "W-a1a"].iter() {
        success_or_transport_err(key, instance_one.clone()).await;
        success_or_transport_err(key, instance_two.clone()).await;
    }

    Ok(())
}

#[tokio::test]
async fn test_when_peer_reconnects_it_should_respond_to_queries() -> Result<()> {
    let (instance_one, instance_two) = two_instances_with_one_disconnected().await?;
    reconnect(instance_two.clone()).await;

    let key_on_instance_2 = key_owned_by_instance(instance_two.clone());
    success_or_transport_err(&key_on_instance_2, instance_one.clone()).await;
    successful_get(&key_on_instance_2, Some("2"), instance_one.clone()).await;

    Ok(())
}

#[tokio::test]
async fn when_new_peer_joins_it_should_receive_requests() -> Result<()> {
    let (instance_one, instance_two) = two_instances().await?;
    let key_on_instance_2 = key_owned_by_instance(instance_two.clone());
    successful_get(&key_on_instance_2, Some("2"), instance_two.clone()).await;

    instance_one.add_peer(instance_two.addr().into()).await?;
    successful_get(&key_on_instance_2, Some("2"), instance_one.clone()).await;

    Ok(())
}

#[tokio::test]
async fn test_when_remote_get_fails_during_load_then_error_is_forwarded() -> Result<()> {
    let (instance_one, instance_two) = two_connected_instances().await?;
    let key_on_instance_2 = error_key_on_instance(instance_two.clone());

    let err = instance_one
        .get(&key_on_instance_2)
        .await
        .expect_err("expected error from loader");

    assert_eq!(
        "Transport error: 'Loading error: 'Something bad happened during loading :/''",
        err.to_string()
    );

    Ok(())
}

#[tokio::test]
async fn test_when_there_are_multiple_instances_each_should_own_portion_of_key_space() -> Result<()>
{
    let instances = spawn_instances(10).await?;
    let first_instance = instances[0].clone();

    for (i, instance) in instances.iter().enumerate() {
        let key_on_instance = key_owned_by_instance(instance.clone());
        successful_get(
            &key_on_instance,
            Some(&i.to_string()),
            first_instance.clone(),
        )
        .await;
    }

    Ok(())
}

// todo: test timeout.
// todo: test local peer cache.
// todo: test removal of peer from peer_list.

fn key_owned_by_instance(instance: TestGroupcache) -> String {
    format!("{}_0", instance.addr())
}

fn error_key_on_instance(instance: TestGroupcache) -> String {
    format!("{}_13", instance.addr())
}

async fn two_instances() -> Result<(TestGroupcache, TestGroupcache)> {
    let instance_one = spawn_groupcache("1").await?;
    let instance_two = spawn_groupcache("2").await?;

    Ok((instance_one, instance_two))
}

async fn spawn_instances(n: usize) -> Result<Vec<TestGroupcache>> {
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

async fn two_connected_instances() -> Result<(TestGroupcache, TestGroupcache)> {
    let (instance_one, instance_two) = two_instances().await?;

    instance_one.add_peer(instance_two.addr().into()).await?;
    instance_two.add_peer(instance_one.addr().into()).await?;

    Ok((instance_one, instance_two))
}

async fn reconnect(instance: TestGroupcache) {
    let listener = TcpListener::bind(instance.addr()).await.unwrap();
    tokio::spawn(async move {
        Server::builder()
            .add_service(instance.grpc_service())
            .serve_with_incoming(TcpListenerStream::new(listener))
            .await
            .unwrap();
    });
}

async fn two_instances_with_one_disconnected() -> Result<(TestGroupcache, TestGroupcache)> {
    let (shutdown_signal, shutdown_recv) = tokio::sync::oneshot::channel::<()>();
    let (shutdown_done_s, shutdown_done_r) = tokio::sync::oneshot::channel::<()>();
    async fn shutdown_proxy(shutdown_signal: Receiver<()>, shutdown_done: Sender<()>) {
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

async fn spawn_groupcache(instance_id: &str) -> Result<TestGroupcache> {
    spawn_groupcache_instance(instance_id, OS_ALLOCATED_PORT_ADDR, pending()).await
}

async fn spawn_groupcache_instance(
    instance_id: &str,
    addr: &str,
    shutdown_signal: impl Future<Output = ()> + Send + 'static,
) -> Result<TestGroupcache> {
    let listener = TcpListener::bind(addr).await.unwrap();
    let addr = listener.local_addr()?;
    let groupcache = {
        let loader = TestCacheLoader::new(instance_id);
        GroupcacheWrapper::<CachedValue>::new(addr.into(), Box::new(loader))
    };

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

async fn success_or_transport_err(key: &str, groupcache: TestGroupcache) {
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

async fn successful_get(key: &str, expected_instance_id: Option<&str>, groupcache: TestGroupcache) {
    let v = groupcache.get(key).await.expect("get should be successful");

    assert_eq!(v.contains(key), true);
    if let Some(instance) = expected_instance_id {
        assert_eq!(v.contains(instance), true);
    }
}

type TestGroupcache = GroupcacheWrapper<CachedValue>;

type CachedValue = String;

struct TestCacheLoader {
    instance_id: String,
}

impl TestCacheLoader {
    fn new(instance_id: &str) -> Self {
        Self {
            instance_id: instance_id.to_string(),
        }
    }
}

#[async_trait]
impl groupcache::ValueLoader for TestCacheLoader {
    type Value = CachedValue;

    async fn load(
        &self,
        key: &Key,
    ) -> std::result::Result<Self::Value, Box<dyn std::error::Error + Send + Sync + 'static>> {
        return if !key.contains("error") && !key.contains("_13") {
            Ok(format!("VAL_INSTANCE_{}_KEY_{}", self.instance_id, key))
        } else {
            Err(anyhow!("Something bad happened during loading :/").into())
        };
    }
}
