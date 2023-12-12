use std::net::SocketAddr;
use async_trait::async_trait;
use k8s_openapi::api::core::v1::Pod;
use kube::{Api, Client};
use kube::api::ListParams;
use groupcache::{GroupcachePeer, ServiceDiscovery, ServiceDiscoveryError};

pub struct Kubernetes {}

pub struct KubernetesBuilder {}

impl KubernetesBuilder {
    pub fn build(self) -> Kubernetes {
        Kubernetes {}
    }
}

impl Kubernetes {
    pub fn builder() -> KubernetesBuilder {
        KubernetesBuilder {}
    }
}

#[async_trait]
impl ServiceDiscovery for Kubernetes {
    async fn instances(&self) -> Result<Vec<GroupcachePeer>, ServiceDiscoveryError> {
        let client = Client::try_default().await.unwrap();
        let api: Api<Pod> = Api::default_namespaced(client);

        let pods_with_label_query = ListParams::default().labels("app=groupcache-powered-backend");
        Ok(api
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
