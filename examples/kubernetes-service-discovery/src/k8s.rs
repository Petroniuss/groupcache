use async_trait::async_trait;
use groupcache::{GroupcachePeer, ServiceDiscovery};
use k8s_openapi::api::core::v1::Pod;
use kube::api::ListParams;
use kube::{Api, Client};
use std::collections::HashSet;
use std::error::Error;
use std::net::SocketAddr;

pub struct Kubernetes {
    api: Api<Pod>,
}

pub struct KubernetesBuilder {
    client: Option<Client>,
}

impl KubernetesBuilder {
    pub fn build(self) -> Kubernetes {
        Kubernetes {
            api: Api::default_namespaced(self.client.unwrap()),
        }
    }
    pub fn client(mut self, client: Client) -> Self {
        self.client = Some(client);
        self
    }
}

impl Kubernetes {
    pub fn builder() -> KubernetesBuilder {
        KubernetesBuilder { client: None }
    }
}

#[async_trait]
impl ServiceDiscovery for Kubernetes {
    async fn pull_instances(
        &self,
    ) -> Result<HashSet<GroupcachePeer>, Box<dyn Error + Send + Sync + 'static>> {
        let pods_with_label_query = ListParams::default().labels("app=groupcache-powered-backend");
        Ok(self
            .api
            .list(&pods_with_label_query)
            .await
            .unwrap()
            .into_iter()
            .filter_map(|pod| {
                let status = pod.status?;
                let pod_ip = status.pod_ip?;

                let Ok(ip) = pod_ip.parse() else {
                    return None;
                };

                let addr = SocketAddr::new(ip, 3000);
                Some(GroupcachePeer::from_socket(addr))
            })
            .collect())
    }
}
