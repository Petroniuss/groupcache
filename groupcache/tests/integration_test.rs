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

    match v2 {
        Err(e) => {
            assert_eq!(
                e.to_string(),
                "Loading error: 'Something bad happened during loading :/'"
            );
        }
        Ok(_) => {
            panic!("should be error");
        }
    }

    Ok(())
}

#[tokio::test]
pub async fn test_get_successful_from_a_remote_peer() -> Result<()> {
    let (instance_one, instance_two) = two_connected_instances().await?;

    async fn assert_lookup(key: &str, groupcache: TestGroupcache) -> Result<()> {
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
    let (instance_one, instance_two) = two_instances_with_one_disconnected().await?;
    for &key in ["K-b3a", "K-karo", "K-x3d", "K-d42", "W-a1a"].iter() {
        assert_success_or_transport_err(key, instance_one.clone()).await;
        assert_success_or_transport_err(key, instance_two.clone()).await;
    }

    Ok(())
}

#[tokio::test]
pub async fn test_no_errors_after_peer_reconnects() -> Result<()> {
    async fn reconnect(instance: TestGroupcache) {
        let listener = TcpListener::bind(instance.addr()).await.unwrap();
        tokio::spawn(async move {
            Server::builder()
                .add_service(instance.server())
                .serve_with_incoming(TcpListenerStream::new(listener))
                .await
                .unwrap();
        });
    }
    let (instance_one, instance_two) = two_instances_with_one_disconnected().await?;

    reconnect(instance_two.clone()).await;
    let instance_2_addr = instance_two.addr().to_string();
    let key_on_instance_2 = format!("{}_0", instance_2_addr);
    assert_success_or_transport_err(&key_on_instance_2, instance_one.clone()).await;

    let res = instance_one.get(&key_on_instance_2).await?;
    assert_eq!(format!("VAL_INSTANCE_2_KEY_{}", key_on_instance_2), res);

    Ok(())
}

// todo: test peer disconnecting

// todo: test peer reconnecting

// todo: test peer joining

// todo: test peer returning errors

async fn two_connected_instances() -> Result<(TestGroupcache, TestGroupcache)> {
    let (instance_one, addr_one) = spawn_groupcache("1").await?;
    let (instance_two, addr_two) = spawn_groupcache("2").await?;

    instance_one.add_peer(addr_two.into()).await?;
    instance_two.add_peer(addr_one.into()).await?;

    Ok((instance_one, instance_two))
}

async fn two_instances_with_one_disconnected() -> Result<(TestGroupcache, TestGroupcache)> {
    let (shutdown_signal, shutdown_recv) = tokio::sync::oneshot::channel::<()>();
    let (shutdown_done_s, shutdown_done_r) = tokio::sync::oneshot::channel::<()>();
    async fn shutdown_proxy(shutdown_signal: Receiver<()>, shutdown_done: Sender<()>) {
        shutdown_signal.await.unwrap();
        shutdown_done.send(()).unwrap();
    }

    let (instance_one, addr_one) = spawn_groupcache("1").await?;
    let (instance_two, addr_two) = spawn_groupcache_instance(
        "2",
        "127.0.0.1:0",
        shutdown_proxy(shutdown_recv, shutdown_done_s),
    )
    .await?;

    instance_one.add_peer(addr_two.into()).await?;
    instance_two.add_peer(addr_one.into()).await?;

    shutdown_signal.send(()).unwrap();
    shutdown_done_r.await.unwrap();

    Ok((instance_one, instance_two))
}

async fn spawn_groupcache(instance_id: &str) -> Result<(TestGroupcache, SocketAddr)> {
    spawn_groupcache_instance(instance_id, "127.0.0.1:0", pending()).await
}

async fn spawn_groupcache_instance(
    instance_id: &str,
    addr: &str,
    shutdown_signal: impl Future<Output = ()> + Send + 'static,
) -> Result<(TestGroupcache, SocketAddr)> {
    let listener = TcpListener::bind(addr).await.unwrap();
    let addr = listener.local_addr()?;
    let groupcache = {
        let loader = TestCacheLoader {
            instance_id: instance_id.to_string(),
        };
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

async fn assert_success_or_transport_err(key: &str, groupcache: TestGroupcache) {
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

type TestGroupcache = GroupcacheWrapper<CachedValue>;

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
