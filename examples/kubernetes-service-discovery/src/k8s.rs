use async_trait::async_trait;
use groupcache::{GroupcachePeer, ServiceDiscovery};
use k8s_openapi::api::core::v1::Pod;
use kube::api::ListParams;
use kube::{Api, Client};
use std::error::Error;
use std::net::SocketAddr;
use std::time::Duration;

pub struct Kubernetes {
    api: Option<Api<Pod>>,
}

pub struct KubernetesBuilder {}

impl KubernetesBuilder {
    pub fn build(self) -> Kubernetes {
        Kubernetes { api: None }
    }
}

impl Kubernetes {
    pub fn builder() -> KubernetesBuilder {
        KubernetesBuilder {}
    }
}

#[async_trait]
impl ServiceDiscovery for Kubernetes {
    async fn initialize(&mut self) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
        let client = Client::try_default().await?;
        let api: Api<Pod> = Api::default_namespaced(client);
        self.api = Some(api);
        Ok(())
    }

    async fn instances(
        &self,
    ) -> Result<Vec<GroupcachePeer>, Box<dyn Error + Send + Sync + 'static>> {
        let api = self.api.as_ref().unwrap();
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

    fn delay(&self) -> Duration {
        Duration::from_secs(10)
    }
}
