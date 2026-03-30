//! Consul-based service discovery for groupcache.
//!
//! Discovers healthy peers by querying Consul's
//! [`/v1/health/service`](https://developer.hashicorp.com/consul/api-docs/health#list-service-instances-for-a-service)
//! endpoint. Requires the `consul` feature.
//!
//! # Example
//!
//! ```no_run
//! use groupcache::discovery::consul::ConsulDiscovery;
//!
//! let discovery = ConsulDiscovery::builder()
//!     .service_name("groupcache")
//!     .port(8080)
//!     .build();
//! ```

use crate::groupcache::GroupcachePeer;
use crate::service_discovery::ServiceDiscovery;
use async_trait::async_trait;
use serde::Deserialize;
use std::collections::HashSet;
use std::error::Error;
use std::net::SocketAddr;
use std::time::Duration;

/// Consul-based service discovery.
///
/// Queries Consul's health API for healthy instances of a named service
/// and returns their addresses as groupcache peers.
pub struct ConsulDiscovery {
    client: reqwest::Client,
    url: String,
    port: u16,
    poll_interval: Duration,
}

/// Builder for [`ConsulDiscovery`].
pub struct ConsulDiscoveryBuilder {
    consul_addr: String,
    service_name: Option<String>,
    port: Option<u16>,
    poll_interval: Option<Duration>,
    tags: Vec<String>,
}

impl ConsulDiscoveryBuilder {
    /// Set the Consul service name to query. Required.
    pub fn service_name(mut self, name: impl Into<String>) -> Self {
        self.service_name = Some(name.into());
        self
    }

    /// Set the Consul HTTP address. Default: `http://localhost:8500`.
    pub fn consul_addr(mut self, addr: impl Into<String>) -> Self {
        self.consul_addr = addr.into();
        self
    }

    /// Set the groupcache port. If not set, uses the port from Consul's
    /// service registration.
    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    /// Filter by a Consul service tag.
    pub fn tag(mut self, tag: impl Into<String>) -> Self {
        self.tags.push(tag.into());
        self
    }

    /// Set the polling interval. Default: 10 seconds.
    pub fn poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = Some(interval);
        self
    }

    /// Build the [`ConsulDiscovery`] instance.
    pub fn build(self) -> ConsulDiscovery {
        let service = self.service_name.expect("ConsulDiscovery requires service_name");
        let mut url = format!(
            "{}/v1/health/service/{}?passing=true",
            self.consul_addr.trim_end_matches('/'),
            service,
        );
        for tag in &self.tags {
            url.push_str(&format!("&tag={}", tag));
        }

        ConsulDiscovery {
            client: reqwest::Client::new(),
            url,
            port: self.port.unwrap_or(0),
            poll_interval: self.poll_interval.unwrap_or(Duration::from_secs(10)),
        }
    }
}

impl ConsulDiscovery {
    /// Create a new builder.
    pub fn builder() -> ConsulDiscoveryBuilder {
        ConsulDiscoveryBuilder {
            consul_addr: "http://localhost:8500".into(),
            service_name: None,
            port: None,
            poll_interval: None,
            tags: Vec::new(),
        }
    }
}

// Consul health API response types (minimal)
#[derive(Deserialize)]
struct HealthServiceEntry {
    #[serde(rename = "Service")]
    service: ConsulService,
}

#[derive(Deserialize)]
struct ConsulService {
    #[serde(rename = "Address")]
    address: String,
    #[serde(rename = "Port")]
    port: u16,
}

#[async_trait]
impl ServiceDiscovery for ConsulDiscovery {
    async fn pull_instances(
        &self,
    ) -> Result<HashSet<GroupcachePeer>, Box<dyn Error + Send + Sync + 'static>> {
        let entries: Vec<HealthServiceEntry> = self
            .client
            .get(&self.url)
            .send()
            .await?
            .json()
            .await?;

        let peers = entries
            .into_iter()
            .filter_map(|entry| {
                let ip = entry.service.address.parse().ok()?;
                let port = if self.port > 0 {
                    self.port
                } else {
                    entry.service.port
                };
                let addr = SocketAddr::new(ip, port);
                Some(GroupcachePeer::from_socket(addr))
            })
            .collect();

        Ok(peers)
    }

    fn interval(&self) -> Duration {
        self.poll_interval
    }
}
