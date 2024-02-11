//! groupcache module contains the core groupcache logic

use crate::errors::{DedupedGroupcacheError, GroupcacheError, InternalGroupcacheError};
use crate::groupcache::{GroupcachePeer, GroupcachePeerClient, ValueBounds, ValueLoader};
use crate::metrics::{
    METRIC_GET_TOTAL, METRIC_LOCAL_CACHE_HIT_TOTAL, METRIC_LOCAL_LOAD_ERROR_TOTAL,
    METRIC_LOCAL_LOAD_TOTAL, METRIC_REMOTE_LOAD_ERROR, METRIC_REMOTE_LOAD_TOTAL,
};
use crate::options::Options;
use crate::routing::{GroupcachePeerWithClient, RoutingState};
use crate::ServiceDiscovery;
use anyhow::{Context, Result};
use groupcache_pb::GroupcacheClient;
use groupcache_pb::{GetRequest, RemoveRequest};
use metrics::counter;
use moka::future::Cache;
use singleflight_async::SingleFlight;
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tonic::transport::Endpoint;
use tonic::IntoRequest;

/// Core implementation of groupcache API.
pub struct GroupcacheInner<Value: ValueBounds> {
    routing_state: Arc<RwLock<RoutingState>>,
    single_flight_group: SingleFlight<Result<Value, DedupedGroupcacheError>>,
    main_cache: Cache<String, Value>,
    hot_cache: Cache<String, Value>,
    loader: Box<dyn ValueLoader<Value = Value>>,
    config: Config,
    me: GroupcachePeer,
}

struct Config {
    https: bool,
    grpc_endpoint_builder: Arc<Box<dyn Fn(Endpoint) -> Endpoint + Send + Sync + 'static>>,
}

async fn run_service_discovery<Value: ValueBounds>(
    _cache: Arc<GroupcacheInner<Value>>,
    service_discovery: Box<dyn ServiceDiscovery>,
) {
    loop {
        tokio::time::sleep(Duration::from_secs(10)).await;
        match service_discovery.instances().await {
            Ok(instances) => {
                println!("Instances: {}", instances.len());
            }
            Err(error) => {
                println!("Error: {}", error);
            }
        }
    }
}

impl<Value: ValueBounds> GroupcacheInner<Value> {
    pub(crate) fn create(
        me: GroupcachePeer,
        loader: Box<dyn ValueLoader<Value = Value>>,
        options: Options<Value>,
    ) -> Arc<Self> {
        let routing_state = Arc::new(RwLock::new(RoutingState::with_local_peer(me)));

        let main_cache = options.main_cache;
        let hot_cache = options.hot_cache;

        let single_flight_group = SingleFlight::default();

        let config = Config {
            https: options.https,
            grpc_endpoint_builder: Arc::new(options.grpc_endpoint_builder),
        };

        let cache = Arc::new(Self {
            routing_state,
            single_flight_group,
            main_cache,
            hot_cache,
            loader,
            me,
            config,
        });

        if let Some(service_discovery) = options.service_discovery {
            tokio::spawn(run_service_discovery(cache.clone(), service_discovery));
        }

        cache
    }

    pub(crate) async fn get(&self, key: &str) -> core::result::Result<Value, GroupcacheError> {
        Ok(self.get_internal(key).await?)
    }

    pub(crate) async fn remove(&self, key: &str) -> core::result::Result<(), GroupcacheError> {
        Ok(self.remove_internal(key).await?)
    }

    async fn get_internal(&self, key: &str) -> Result<Value, InternalGroupcacheError> {
        counter!(METRIC_GET_TOTAL, 1);
        if let Some(value) = self.main_cache.get(key).await {
            counter!(METRIC_LOCAL_CACHE_HIT_TOTAL, 1);
            return Ok(value);
        }

        if let Some(value) = self.hot_cache.get(key).await {
            counter!(METRIC_LOCAL_CACHE_HIT_TOTAL, 1);
            return Ok(value);
        }

        let peer = {
            let lock = self.routing_state.read().unwrap();
            lock.lookup_peer(key)
        }?;

        let value = self.get_deduped_instrumented(key, peer).await?;
        Ok(value)
    }

    async fn get_deduped_instrumented(
        &self,
        key: &str,
        peer: GroupcachePeerWithClient,
    ) -> Result<Value, InternalGroupcacheError> {
        self.single_flight_group
            .work(key, || async {
                self.get_deduped(key, peer)
                    .await
                    .map_err(|e| DedupedGroupcacheError(Arc::new(e)))
            })
            .await
            .map_err(InternalGroupcacheError::Deduped)
    }

    async fn get_deduped(
        &self,
        key: &str,
        peer: GroupcachePeerWithClient,
    ) -> Result<Value, InternalGroupcacheError> {
        if peer.peer == self.me {
            let value = self.load_locally_instrumented(key).await?;
            self.main_cache.insert(key.to_string(), value.clone()).await;
            return Ok(value);
        }

        let mut client = peer
            .client
            .context("unreachable: cannot be empty since it's a remote peer")?;
        let res = self.load_remotely_instrumented(key, &mut client).await;
        match res {
            Ok(value) => {
                self.hot_cache.insert(key.to_string(), value.clone()).await;
                Ok(value)
            }
            Err(_) => {
                let value = self.load_locally_instrumented(key).await?;
                Ok(value)
            }
        }
    }

    async fn load_locally_instrumented(&self, key: &str) -> Result<Value, InternalGroupcacheError> {
        counter!(METRIC_LOCAL_LOAD_TOTAL, 1);
        self.loader
            .load(key)
            .await
            .map_err(|e| {
                counter!(METRIC_LOCAL_LOAD_ERROR_TOTAL, 1);
                e
            })
            .map_err(InternalGroupcacheError::LocalLoader)
    }

    async fn load_remotely_instrumented(
        &self,
        key: &str,
        client: &mut GroupcachePeerClient,
    ) -> Result<Value, InternalGroupcacheError> {
        counter!(METRIC_REMOTE_LOAD_TOTAL, 1);
        self.load_remotely(key, client).await.map_err(|e| {
            counter!(METRIC_REMOTE_LOAD_ERROR, 1);
            e
        })
    }

    async fn load_remotely(
        &self,
        key: &str,
        client: &mut GroupcachePeerClient,
    ) -> Result<Value, InternalGroupcacheError> {
        let response = client
            .get(
                GetRequest {
                    key: key.to_string(),
                }
                .into_request(),
            )
            .await?;

        let get_response = response.into_inner();
        let bytes = get_response.value.unwrap();
        let value = rmp_serde::from_read(bytes.as_slice())?;

        Ok(value)
    }

    async fn remove_internal(
        &self,
        key: &str,
    ) -> core::result::Result<(), InternalGroupcacheError> {
        self.hot_cache.remove(key).await;

        let peer = {
            let lock = self.routing_state.read().unwrap();
            lock.lookup_peer(key)
        }?;

        if peer.peer == self.me {
            self.main_cache.remove(key).await;
        } else {
            let mut client = peer
                .client
                .context("unreachable: cannot be empty since it's a remote peer")?;
            self.remove_remotely(key, &mut client).await?;
        }

        Ok(())
    }

    async fn remove_remotely(
        &self,
        key: &str,
        client: &mut GroupcachePeerClient,
    ) -> core::result::Result<(), InternalGroupcacheError> {
        let _ = client
            .remove(
                RemoveRequest {
                    key: key.to_string(),
                }
                .into_request(),
            )
            .await?;

        Ok(())
    }

    pub(crate) async fn add_peer(&self, peer: GroupcachePeer) -> Result<(), GroupcacheError> {
        let contains_peer = {
            let read_lock = self.routing_state.read().unwrap();
            read_lock.contains_peer(&peer)
        };

        if contains_peer {
            return Ok(());
        }

        let client = self.connect(peer).await?;
        let mut write_lock = self.routing_state.write().unwrap();
        write_lock.add_peer(peer, client);

        Ok(())
    }

    async fn connect(
        &self,
        peer: GroupcachePeer,
    ) -> Result<GroupcachePeerClient, InternalGroupcacheError> {
        let socket = peer.socket;
        let peer_addr = if self.config.https {
            format!("https://{}", socket)
        } else {
            format!("http://{}", socket)
        };

        let endpoint: Endpoint = peer_addr.try_into()?;
        let endpoint = self.config.grpc_endpoint_builder.as_ref()(endpoint);
        let client = GroupcacheClient::connect(endpoint).await?;
        Ok(client)
    }

    pub(crate) async fn remove_peer(&self, peer: GroupcachePeer) -> Result<(), GroupcacheError> {
        let contains_peer = {
            let read_lock = self.routing_state.read().unwrap();
            read_lock.contains_peer(&peer)
        };

        if !contains_peer {
            return Ok(());
        }

        let mut write_lock = self.routing_state.write().unwrap();
        write_lock.remove_peer(peer);

        Ok(())
    }

    pub(crate) fn addr(&self) -> SocketAddr {
        self.me.socket
    }
}
