//! DNS-based service discovery for groupcache.
//!
//! Resolves a DNS name to IP addresses and uses them as peers.
//! Works with headless Kubernetes services, SRV records, or any
//! DNS name that resolves to multiple A/AAAA records.
//!
//! No external dependencies required — uses `tokio::net::lookup_host`.
//!
//! # Example
//!
//! ```no_run
//! use groupcache::discovery::dns::DnsDiscovery;
//!
//! // Headless k8s service: resolves to all pod IPs
//! let discovery = DnsDiscovery::new("my-cache.default.svc.cluster.local", 8080);
//! ```

use crate::groupcache::GroupcachePeer;
use crate::service_discovery::ServiceDiscovery;
use async_trait::async_trait;
use std::collections::HashSet;
use std::error::Error;
use std::time::Duration;

/// DNS-based service discovery.
///
/// Resolves a hostname to IP addresses on each poll and returns
/// them as groupcache peers on the configured port.
pub struct DnsDiscovery {
    host_port: String,
    port: u16,
    poll_interval: Duration,
}

impl DnsDiscovery {
    /// Create a new DNS discovery that resolves the given hostname.
    ///
    /// The `port` is the groupcache gRPC port that peers listen on.
    pub fn new(hostname: impl Into<String>, port: u16) -> Self {
        let hostname = hostname.into();
        Self {
            host_port: format!("{}:{}", hostname, port),
            port,
            poll_interval: Duration::from_secs(10),
        }
    }

    /// Set the polling interval. Default: 10 seconds.
    pub fn with_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = interval;
        self
    }
}

#[async_trait]
impl ServiceDiscovery for DnsDiscovery {
    async fn pull_instances(
        &self,
    ) -> Result<HashSet<GroupcachePeer>, Box<dyn Error + Send + Sync + 'static>> {
        let addrs = tokio::net::lookup_host(&self.host_port).await?;

        let peers = addrs
            .map(|mut addr| {
                // Ensure we use our configured port (DNS might return
                // a different port from SRV records)
                addr.set_port(self.port);
                GroupcachePeer::from_socket(addr)
            })
            .collect();

        Ok(peers)
    }

    fn interval(&self) -> Duration {
        self.poll_interval
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn dns_resolves_localhost() {
        let discovery = DnsDiscovery::new("localhost", 8080);
        let peers = discovery.pull_instances().await.unwrap();
        assert!(!peers.is_empty(), "localhost should resolve to at least one address");
        for peer in &peers {
            assert_eq!(peer.socket.port(), 8080);
        }
    }

    #[tokio::test]
    async fn dns_error_on_nonexistent_host() {
        let discovery = DnsDiscovery::new("this-host-does-not-exist.invalid", 8080);
        let result = discovery.pull_instances().await;
        assert!(result.is_err());
    }

    #[test]
    fn custom_interval() {
        let discovery = DnsDiscovery::new("example.com", 8080)
            .with_interval(Duration::from_secs(30));
        assert_eq!(discovery.interval(), Duration::from_secs(30));
    }
}
