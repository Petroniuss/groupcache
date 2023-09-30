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

#[tokio::test]
pub async fn test_get_local_peer() -> Result<()> {
    let groupcache = {
        let loader = TestCacheLoader {
            instance_id: "1".to_string(),
        };
        let addr: SocketAddr = "127.0.0.1:8080".parse()?;
        GroupcacheWrapper::<CachedValue>::new(addr.into(), Box::new(loader))
    };

    let key = "K1";
    let v = groupcache.get(key).await?;
    assert_eq!("VAL_INSTANCE_1_KEY_K1", v);

    let v = groupcache.get(key).await?;
    assert_eq!("VAL_INSTANCE_1_KEY_K1", v);

    let key2 = "K_error";
    let v2 = groupcache.get(key2).await;
    assert_eq!(true, v2.is_err(), "is error");

    Ok(())
}

#[tokio::test]
pub async fn test_get_successful_from_a_remote_peer() -> Result<()> {
    let (instance_one, instance_two) = two_connected_instances().await?;

    async fn assert_lookup(key: &str, groupcache: GroupcacheWrapper<CachedValue>) -> Result<()> {
        let v = groupcache.get(key).await?;
        assert_eq!(v.contains(key), true);
        Ok(())
    }

    for &key in ["K-b3a", "K-karo", "K-x3d", "K-d42", "W-a1a"].iter() {
        assert_lookup(key, instance_one.clone()).await?;
        assert_lookup(key, instance_two.clone()).await?;
    }

    Ok(())
}

#[tokio::test]
pub async fn test_failing_get_after_peer_disconnects() -> Result<()> {
    async fn assert_lookup(key: &str, groupcache: GroupcacheWrapper<CachedValue>) {
        let result = groupcache.get(key).await;
        match result {
            Ok(v) => {
                assert_eq!(v.contains(key), true);
            }
            Err(e) => {
                let error_string = e.to_string();
                assert_eq!(
                    error_string.contains("error"),
                    true,
                    "expected error, got: '{}'",
                    error_string
                );
            }
        }
    }

    let groupcache = two_instances_with_one_disconnected().await?;

    for &key in ["K-b3a", "K-karo", "K-x3d", "K-d42", "W-a1a"].iter() {
        assert_lookup(key, groupcache.clone()).await;
    }

    Ok(())
}

// todo: test peer disconnecting

// todo: test peer reconnecting

// todo: test peer joining

// todo: test peer returning errors

async fn two_connected_instances() -> Result<(
    GroupcacheWrapper<CachedValue>,
    GroupcacheWrapper<CachedValue>,
)> {
    let (instance_one, addr_one) = spawn_groupcache("1".to_string()).await?;
    let (instance_two, addr_two) = spawn_groupcache("2".to_string()).await?;

    instance_one.add_peer(addr_two.into()).await?;
    instance_two.add_peer(addr_one.into()).await?;

    Ok((instance_one, instance_two))
}

async fn two_instances_with_one_disconnected() -> Result<GroupcacheWrapper<CachedValue>> {
    let (shutdown_signal, shutdown_recv) = tokio::sync::oneshot::channel::<()>();
    let (shutdown_done_s, shutdown_done_r) = tokio::sync::oneshot::channel::<()>();
    async fn shutdown_proxy(shutdown_signal: Receiver<()>, shutdown_done: Sender<()>) {
        shutdown_signal.await.unwrap();
        shutdown_done.send(()).unwrap();
    }

    let (instance_one, addr_one) = spawn_groupcache("1".to_string()).await?;
    let (instance_two, addr_two) = spawn_groupcache_instance(
        "2".to_string(),
        "127.0.0.1:0".to_string(),
        shutdown_proxy(shutdown_recv, shutdown_done_s),
    )
    .await?;

    instance_one.add_peer(addr_two.into()).await?;
    instance_two.add_peer(addr_one.into()).await?;

    shutdown_signal.send(()).unwrap();
    shutdown_done_r.await.unwrap();

    Ok(instance_one)
}

async fn spawn_groupcache(
    instance_id: String,
) -> Result<(GroupcacheWrapper<CachedValue>, SocketAddr)> {
    spawn_groupcache_instance(instance_id, "127.0.0.1:0".to_string(), pending()).await
}

async fn spawn_groupcache_instance(
    instance_id: String,
    addr: String,
    shutdown_signal: impl Future<Output = ()> + Send + 'static,
) -> Result<(GroupcacheWrapper<CachedValue>, SocketAddr)> {
    let listener = TcpListener::bind(addr).await.unwrap();
    let addr = listener.local_addr()?;
    let groupcache = {
        let loader = TestCacheLoader { instance_id };
        let addr = addr;
        GroupcacheWrapper::<CachedValue>::new(addr.into(), Box::new(loader))
    };

    let server = groupcache.server();
    tokio::spawn(async move {
        Server::builder()
            .add_service(server)
            .serve_with_incoming_shutdown(TcpListenerStream::new(listener), shutdown_signal)
            .await
            .unwrap();
    });

    Ok((groupcache, addr))
}

type CachedValue = String;

struct TestCacheLoader {
    instance_id: String,
}

#[async_trait]
impl groupcache::ValueLoader for TestCacheLoader {
    type Value = CachedValue;

    async fn load(
        &self,
        key: &Key,
    ) -> std::result::Result<Self::Value, Box<dyn std::error::Error + Send + Sync + 'static>> {
        return if !key.contains("error") {
            Ok(format!("VAL_INSTANCE_{}_KEY_{}", self.instance_id, key))
        } else {
            Err(anyhow!("Something bad happened during loading :/").into())
        };
    }
}
