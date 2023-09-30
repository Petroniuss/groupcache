use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use groupcache::{GroupcacheWrapper, Key};
use pretty_assertions::assert_eq;
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tonic::codegen::tokio_stream;
use tonic::transport::Server;

#[tokio::test]
pub async fn test_get_without_any_remote_peers() -> Result<()> {
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
pub async fn test_get_from_a_remote_peer() -> Result<()> {
    let groupcache = two_connected_instances().await?;

    async fn assert_lookup(key: &str, groupcache: GroupcacheWrapper<CachedValue>) -> Result<()> {
        let v = groupcache.get(key).await?;
        assert_eq!(v.contains(key), true);
        Ok(())
    }

    for &key in ["K-b3a", "K-karo", "K-x3d", "K-d42", "W-a1a"].iter() {
        assert_lookup(key, groupcache.clone()).await?;
    }

    Ok(())
}

// todo: test peer disconnecting

// todo: test peer reconnecting

// todo: test peer joining

async fn two_connected_instances() -> Result<GroupcacheWrapper<CachedValue>> {
    let (instance_one, addr_one) = spawn_groupcache("1".to_string()).await?;
    let (instance_two, addr_two) = spawn_groupcache("2".to_string()).await?;

    instance_one.add_peer(addr_two.into()).await?;
    instance_two.add_peer(addr_one.into()).await?;

    Ok(instance_one)
}

async fn spawn_groupcache(
    instance_id: String,
) -> Result<(GroupcacheWrapper<CachedValue>, SocketAddr)> {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
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
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
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
