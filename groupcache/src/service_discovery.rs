use crate::{GroupcacheInner, GroupcachePeer, ValueBounds};
use async_trait::async_trait;
use log::error;
use std::collections::HashSet;
use std::error::Error;
use std::sync::Weak;
use std::time::Duration;

/// This trait abstracts away boilerplate associated with pull-based service discovery.
///
/// groupcache periodically runs [`ServiceDiscovery::pull_instances`] and
/// compares pulled instances with the ones it already knew about
/// from previous invocations of [`ServiceDiscovery::pull_instances`].
///
/// groupcache connects with new peers and disconnects with missing peers via [`Groupcache::set_peers`].
#[async_trait]
pub trait ServiceDiscovery: Send {
    /// Pulls groupcache instances from a source-of-truth system (kubernetes API server, consul etc).
    ///
    /// Based on this function groupcache is able to update its routing table,
    /// so that it can correctly route to healthy instances and stop hitting unhealthy nodes.
    ///
    /// Returning Error from this function will be logged by groupcache
    /// but routing table will not be updated.
    async fn pull_instances(
        &self,
    ) -> Result<HashSet<GroupcachePeer>, Box<dyn Error + Send + Sync + 'static>>;

    /// Specifies duration between consecutive executions of [`ServiceDiscovery::pull_instances`].
    fn interval(&self) -> Duration {
        Duration::from_secs(10)
    }
}

pub(crate) async fn run_service_discovery<Value: ValueBounds>(
    cache: Weak<GroupcacheInner<Value>>,
    service_discovery: Box<dyn ServiceDiscovery>,
    cancel: tokio_util::sync::CancellationToken,
) {
    loop {
        tokio::select! {
            _ = tokio::time::sleep(service_discovery.interval()) => {}
            _ = cancel.cancelled() => break,
        }
        let Some(cache) = cache.upgrade() else { break };
        match service_discovery.pull_instances().await {
            Ok(instances) => {
                if let Err(error) = cache.set_peers(instances).await {
                    error!("Error connecting to refreshed instances: {}", error);
                };
            }
            Err(error) => {
                error!("Error during when refreshing instances: {}", error);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::groupcache::ValueLoader;
    use crate::options::Options;
    use std::sync::atomic::{AtomicU32, Ordering};
    use tokio_util::sync::CancellationToken;
    use std::sync::Arc;
    use std::time::Duration;

    struct DummyLoader;

    #[async_trait]
    impl ValueLoader for DummyLoader {
        type Value = String;
        async fn load(
            &self,
            key: &str,
        ) -> Result<String, Box<dyn std::error::Error + Send + Sync + 'static>> {
            Ok(key.to_string())
        }
    }

    #[test]
    fn default_interval_is_ten_seconds() {
        struct NoOp;
        #[async_trait]
        impl ServiceDiscovery for NoOp {
            async fn pull_instances(
                &self,
            ) -> Result<HashSet<GroupcachePeer>, Box<dyn Error + Send + Sync + 'static>>
            {
                Ok(HashSet::new())
            }
        }
        assert_eq!(NoOp.interval(), Duration::from_secs(10));
    }

    #[tokio::test]
    async fn service_discovery_stops_when_cache_dropped() {
        static PULL_COUNT: AtomicU32 = AtomicU32::new(0);

        struct CountingDiscovery;
        #[async_trait]
        impl ServiceDiscovery for CountingDiscovery {
            async fn pull_instances(
                &self,
            ) -> Result<HashSet<GroupcachePeer>, Box<dyn Error + Send + Sync + 'static>>
            {
                PULL_COUNT.fetch_add(1, Ordering::SeqCst);
                Ok(HashSet::new())
            }
            fn interval(&self) -> Duration {
                Duration::from_millis(10)
            }
        }

        PULL_COUNT.store(0, Ordering::SeqCst);

        let addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
        let peer = GroupcachePeer::from_socket(addr);
        let inner = Arc::new(GroupcacheInner::new(
            peer,
            Box::new(DummyLoader),
            CancellationToken::new(),
            Options::default(),
        ));
        let weak = Arc::downgrade(&inner);

        let handle = tokio::spawn(run_service_discovery(weak, Box::new(CountingDiscovery), CancellationToken::new()));

        // Let it run a few iterations
        tokio::time::sleep(Duration::from_millis(80)).await;
        let count_before_drop = PULL_COUNT.load(Ordering::SeqCst);
        assert!(count_before_drop > 0, "service discovery should have run");

        // Drop the strong reference — next iteration should break
        drop(inner);
        tokio::time::sleep(Duration::from_millis(40)).await;

        assert!(
            handle.is_finished(),
            "task should stop after cache is dropped"
        );
    }

    #[tokio::test]
    async fn service_discovery_logs_set_peers_error() {
        // Bind+drop so port is free but nothing listens.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let unreachable_addr = listener.local_addr().unwrap();
        drop(listener);

        struct UnreachablePeerDiscovery(std::net::SocketAddr);
        #[async_trait]
        impl ServiceDiscovery for UnreachablePeerDiscovery {
            async fn pull_instances(
                &self,
            ) -> Result<HashSet<GroupcachePeer>, Box<dyn Error + Send + Sync + 'static>>
            {
                Ok(HashSet::from([GroupcachePeer::from_socket(self.0)]))
            }
            fn interval(&self) -> Duration {
                Duration::from_millis(50)
            }
        }

        let addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
        let peer = GroupcachePeer::from_socket(addr);
        let inner = Arc::new(GroupcacheInner::new(
            peer,
            Box::new(DummyLoader),
            CancellationToken::new(),
            Options::default(),
        ));
        let weak = Arc::downgrade(&inner);

        let handle = tokio::spawn(run_service_discovery(
            weak,
            Box::new(UnreachablePeerDiscovery(unreachable_addr)),
            CancellationToken::new(),
        ));

        // Wait for at least one failed set_peers attempt.
        tokio::time::sleep(Duration::from_millis(300)).await;
        assert!(
            !handle.is_finished(),
            "task should keep running despite set_peers errors"
        );

        drop(inner);
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert!(handle.is_finished());
    }

    #[tokio::test]
    async fn service_discovery_logs_pull_error() {
        struct FailingDiscovery;
        #[async_trait]
        impl ServiceDiscovery for FailingDiscovery {
            async fn pull_instances(
                &self,
            ) -> Result<HashSet<GroupcachePeer>, Box<dyn Error + Send + Sync + 'static>>
            {
                Err("discovery unavailable".into())
            }
            fn interval(&self) -> Duration {
                Duration::from_millis(10)
            }
        }

        let addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
        let peer = GroupcachePeer::from_socket(addr);
        let inner = Arc::new(GroupcacheInner::new(
            peer,
            Box::new(DummyLoader),
            CancellationToken::new(),
            Options::default(),
        ));
        let weak = Arc::downgrade(&inner);

        let handle = tokio::spawn(run_service_discovery(weak, Box::new(FailingDiscovery), CancellationToken::new()));

        // Let it attempt a few pulls (they all fail but shouldn't crash)
        tokio::time::sleep(Duration::from_millis(50)).await;
        assert!(
            !handle.is_finished(),
            "task should keep running despite errors"
        );

        drop(inner);
        tokio::time::sleep(Duration::from_millis(30)).await;
        assert!(handle.is_finished());
    }
}
