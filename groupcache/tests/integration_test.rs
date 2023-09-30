use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;

use groupcache::{GroupcacheWrapper, Key};
use serde::{Deserialize, Serialize};

use pretty_assertions::assert_eq;
use tokio::net::TcpListener;
use tonic::codegen::tokio_stream;
use tonic::transport::Server;

#[tokio::test]
pub async fn test_get_from_single_peer() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let groupcache = {
        let loader = TestCacheLoader {};

        let addr = listener.local_addr()?;
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

    let key = "K1";
    let v = groupcache.get(key).await?;
    assert_eq!("VAL_K1", v.plain_string);

    let v = groupcache.get(key).await?;
    assert_eq!("VAL_K1", v.plain_string);

    let key2 = "K_error";
    let v2 = groupcache.get(key2).await;
    assert_eq!(true, v2.is_err(), "is error");

    Ok(())
}

#[derive(Clone, Deserialize, Serialize)]
struct CachedValue {
    plain_string: String,
}

struct TestCacheLoader {}

#[async_trait]
impl groupcache::ValueLoader for TestCacheLoader {
    type Value = CachedValue;

    async fn load(
        &self,
        key: &Key,
    ) -> std::result::Result<Self::Value, Box<dyn std::error::Error + Send + Sync + 'static>> {
        return if key.contains("error") {
            Err(anyhow!("Something bad happened during loading :/").into())
        } else {
            Ok(CachedValue {
                plain_string: format!("VAL_{}", key),
            })
        };
    }
}
