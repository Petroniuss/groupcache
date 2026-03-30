//! Kubernetes service discovery for groupcache.
//!
//! Discovers peers by listing pods matching a label selector via the
//! Kubernetes API server. Requires the `kubernetes` feature.
//!
//! # Example
//!
//! ```no_run
//! use groupcache::discovery::kubernetes::KubernetesDiscovery;
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
//! let client = kube::Client::try_default().await?;
//! let discovery = KubernetesDiscovery::builder()
//!     .client(client)
//!     .label_selector("app=my-cache")
//!     .port(8080)
//!     .build()?;
//! # Ok(())
//! # }
//! ```

use crate::groupcache::GroupcachePeer;
use crate::service_discovery::ServiceDiscovery;
use async_trait::async_trait;
use k8s_openapi::api::core::v1::Pod;
use kube::api::ListParams;
use kube::{Api, Client};
use std::collections::HashSet;
use std::error::Error;
use std::net::SocketAddr;
use std::time::Duration;

/// Kubernetes-based service discovery for groupcache peers.
///
/// Lists pods matching a label selector and extracts pod IPs to build
/// the peer set. Uses the Kubernetes API server (via the `kube` crate).
pub struct KubernetesDiscovery {
    api: Api<Pod>,
    label_selector: String,
    port: u16,
    poll_interval: Duration,
}

/// Builder for [`KubernetesDiscovery`].
pub struct KubernetesDiscoveryBuilder {
    client: Option<Client>,
    namespace: Option<String>,
    label_selector: Option<String>,
    port: Option<u16>,
    poll_interval: Option<Duration>,
}

impl KubernetesDiscoveryBuilder {
    /// Set the Kubernetes client. Required.
    pub fn client(mut self, client: Client) -> Self {
        self.client = Some(client);
        self
    }

    /// Set a specific namespace. Defaults to the pod's own namespace.
    pub fn namespace(mut self, namespace: impl Into<String>) -> Self {
        self.namespace = Some(namespace.into());
        self
    }

    /// Set the label selector for discovering peer pods. Required.
    ///
    /// Example: `"app=my-groupcache-service"`
    pub fn label_selector(mut self, selector: impl Into<String>) -> Self {
        self.label_selector = Some(selector.into());
        self
    }

    /// Set the port that groupcache peers listen on. Required.
    pub fn port(mut self, port: u16) -> Self {
        self.port = Some(port);
        self
    }

    /// Set the polling interval for service discovery. Default: 10 seconds.
    pub fn poll_interval(mut self, interval: Duration) -> Self {
        self.poll_interval = Some(interval);
        self
    }

    /// Build the [`KubernetesDiscovery`] instance.
    ///
    /// # Errors
    ///
    /// Returns an error if `client`, `label_selector`, or `port` were not set.
    pub fn build(self) -> Result<KubernetesDiscovery, Box<dyn Error + Send + Sync>> {
        let client = self
            .client
            .ok_or("KubernetesDiscovery requires a kube::Client")?;
        let label_selector = self
            .label_selector
            .ok_or("KubernetesDiscovery requires a label_selector")?;
        let port = self
            .port
            .ok_or("KubernetesDiscovery requires a port")?;

        let api = match self.namespace {
            Some(ns) => Api::namespaced(client, &ns),
            None => Api::default_namespaced(client),
        };

        Ok(KubernetesDiscovery {
            api,
            label_selector,
            port,
            poll_interval: self.poll_interval.unwrap_or(Duration::from_secs(10)),
        })
    }
}

impl KubernetesDiscovery {
    /// Create a new builder.
    pub fn builder() -> KubernetesDiscoveryBuilder {
        KubernetesDiscoveryBuilder {
            client: None,
            namespace: None,
            label_selector: None,
            port: None,
            poll_interval: None,
        }
    }
}

#[async_trait]
impl ServiceDiscovery for KubernetesDiscovery {
    async fn pull_instances(
        &self,
    ) -> Result<HashSet<GroupcachePeer>, Box<dyn Error + Send + Sync + 'static>> {
        let params = ListParams::default().labels(&self.label_selector);
        let pods = self.api.list(&params).await?;

        let peers = pods
            .into_iter()
            .filter_map(|pod| {
                let pod_ip = pod.status?.pod_ip?;
                let ip = pod_ip.parse().ok()?;
                let addr = SocketAddr::new(ip, self.port);
                Some(GroupcachePeer::from_socket(addr))
            })
            .collect();

        Ok(peers)
    }

    fn interval(&self) -> Duration {
        self.poll_interval
    }
}
