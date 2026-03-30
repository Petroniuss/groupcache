//! groupcache module contains the core groupcache logic

use crate::errors::InternalGroupcacheError::Anyhow;
use crate::errors::{DedupedGroupcacheError, GroupcacheError, InternalGroupcacheError};
use crate::groupcache::{GroupcachePeer, GroupcachePeerClient, ValueBounds, ValueLoader};
use crate::invalidation::InvalidationManager;
use crate::metrics::{
    METRIC_GET_TOTAL, METRIC_LOCAL_CACHE_HIT_TOTAL, METRIC_LOCAL_LOAD_ERROR_TOTAL,
    METRIC_LOCAL_LOAD_TOTAL, METRIC_REMOTE_LOAD_ERROR, METRIC_REMOTE_LOAD_TOTAL,
    METRIC_REMOVE_TOTAL,
};
use crate::options::Options;
use crate::routing::{GroupcachePeerWithClient, RoutingState};
use anyhow::{Context, Result};
use groupcache_pb::GroupcacheClient;
use groupcache_pb::{GetRequest, RemoveRequest};
use metrics::counter;
use moka::future::Cache;
use singleflight_async::SingleFlight;
use std::collections::HashSet;
use std::net::SocketAddr;
use arc_swap::ArcSwap;
use std::sync::{Arc, OnceLock};
use tokio::task::{AbortHandle, JoinSet};
use tonic::transport::Endpoint;
use tonic::IntoRequest;

/// Core implementation of groupcache API.
pub struct GroupcacheInner<Value: ValueBounds> {
    routing_state: Arc<ArcSwap<RoutingState>>,
    single_flight_group: SingleFlight<Arc<str>, Result<Value, DedupedGroupcacheError>>,
    main_cache: Cache<Arc<str>, Value>,
    hot_cache: Cache<Arc<str>, Value>,
    loader: Box<dyn ValueLoader<Value = Value>>,
    config: Config,
    me: GroupcachePeer,
    cancel: tokio_util::sync::CancellationToken,
    service_discovery_abort: OnceLock<AbortHandle>,
    pub(crate) invalidation: InvalidationManager,
}

struct Config {
    https: bool,
    grpc_timeout: std::time::Duration,
    grpc_endpoint_builder: Arc<Box<dyn Fn(Endpoint) -> Endpoint + Send + Sync + 'static>>,
}

impl<Value: ValueBounds> GroupcacheInner<Value> {
    pub(crate) fn new(
        me: GroupcachePeer,
        loader: Box<dyn ValueLoader<Value = Value>>,
        cancel: tokio_util::sync::CancellationToken,
        options: Options<Value>,
    ) -> Self {
        let routing_state = Arc::new(ArcSwap::from_pointee(RoutingState::with_local_peer(me)));

        let main_cache = options.main_cache;
        let hot_cache = options.hot_cache;

        let single_flight_group = SingleFlight::default();

        let config = Config {
            https: options.https,
            grpc_timeout: options.grpc_timeout,
            grpc_endpoint_builder: Arc::new(options.grpc_endpoint_builder),
        };

        let invalidation = InvalidationManager::new(options.invalidation, cancel.clone());

        Self {
            routing_state,
            single_flight_group,
            main_cache,
            hot_cache,
            loader,
            me,
            config,
            cancel,
            service_discovery_abort: OnceLock::new(),
            invalidation,
        }
    }

    pub(crate) async fn get(&self, key: &str) -> core::result::Result<Value, GroupcacheError> {
        Ok(self.get_internal(key).await?)
    }

    pub(crate) async fn remove(&self, key: &str) -> core::result::Result<(), GroupcacheError> {
        Ok(self.remove_internal(key).await?)
    }

    async fn get_internal(&self, key: &str) -> Result<Value, InternalGroupcacheError> {
        counter!(METRIC_GET_TOTAL).increment(1);
        if let Some(value) = self.main_cache.get(key).await {
            counter!(METRIC_LOCAL_CACHE_HIT_TOTAL).increment(1);
            return Ok(value);
        }

        if let Some(value) = self.hot_cache.get(key).await {
            counter!(METRIC_LOCAL_CACHE_HIT_TOTAL).increment(1);
            return Ok(value);
        }

        let peer = {
            let lock = self.routing_state.load();
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
            .work(Arc::from(key), || async {
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
            self.main_cache.insert(Arc::from(key), value.clone()).await;
            return Ok(value);
        }

        let mut client = peer
            .client
            .context("unreachable: cannot be empty since it's a remote peer")?;
        let res = self.load_remotely_instrumented(key, &mut client).await;
        match res {
            Ok(value) => {
                self.hot_cache.insert(Arc::from(key), value.clone()).await;
                Ok(value)
            }
            Err(err) => {
                log::warn!(
                    "Remote load failed for key '{}' from peer {:?}, falling back to local load: {}",
                    key, peer.peer, err
                );
                let value = self.load_locally_instrumented(key).await?;
                Ok(value)
            }
        }
    }

    async fn load_locally_instrumented(&self, key: &str) -> Result<Value, InternalGroupcacheError> {
        counter!(METRIC_LOCAL_LOAD_TOTAL).increment(1);
        self.loader
            .load(key)
            .await
            .inspect_err(|_| {
                counter!(METRIC_LOCAL_LOAD_ERROR_TOTAL).increment(1);
            })
            .map_err(InternalGroupcacheError::LocalLoader)
    }

    async fn load_remotely_instrumented(
        &self,
        key: &str,
        client: &mut GroupcachePeerClient,
    ) -> Result<Value, InternalGroupcacheError> {
        counter!(METRIC_REMOTE_LOAD_TOTAL).increment(1);
        self.load_remotely(key, client)
            .await
            .inspect_err(|_| counter!(METRIC_REMOTE_LOAD_ERROR).increment(1))
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
        let bytes = get_response
            .value
            .ok_or_else(|| InternalGroupcacheError::EmptyResponse(key.to_string()))?;
        let value = crate::codec::deserialize(&bytes)
            .map_err(|e| anyhow::anyhow!("failed to deserialize value from peer: {}", e))?;

        Ok(value)
    }

    async fn remove_internal(
        &self,
        key: &str,
    ) -> core::result::Result<(), InternalGroupcacheError> {
        counter!(METRIC_REMOVE_TOTAL).increment(1);
        self.hot_cache.remove(key).await;

        let peer = {
            let lock = self.routing_state.load();
            lock.lookup_peer(key)
        }?;

        if peer.peer == self.me {
            self.main_cache.remove(key).await;
            self.invalidation.broadcast(key);
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
        // Quick check with read lock to avoid unnecessary connections.
        {
            let read_lock = self.routing_state.load();
            if read_lock.contains_peer(&peer) {
                return Ok(());
            }
        }

        let (_, client) = self.connect(peer).await?;

        if self.invalidation.config().enabled {
            self.invalidation
                .spawn_watcher(peer, client.clone(), self.hot_cache.clone());
        }

        // Re-check to avoid adding a duplicate if another task connected concurrently.
        {
            let current = self.routing_state.load();
            if !current.contains_peer(&peer) {
                let mut new_state = (**current).clone();
                new_state.add_peer(peer, client);
                self.routing_state.store(Arc::new(new_state));
            }
        }

        // After ring update: evict main cache keys we no longer own
        self.evict_main_cache_keys_not_owned().await;

        Ok(())
    }

    pub(crate) async fn set_peers(
        &self,
        updated_peers: HashSet<GroupcachePeer>,
    ) -> Result<(), GroupcacheError> {
        let current_peers: HashSet<GroupcachePeer> = {
            let read_lock = self.routing_state.load();
            read_lock.peers()
        };

        let new_connections_results = self
            .connect_to_new_peers(&updated_peers, &current_peers)
            .await?;
        let peers_to_remove = current_peers.difference(&updated_peers).collect::<Vec<_>>();

        let no_updates = peers_to_remove.is_empty() && new_connections_results.is_empty();
        if no_updates {
            return Ok(());
        }

        // Before ring update: evict hot cache entries for keys owned by departing peers
        for peer in &peers_to_remove {
            self.evict_hot_cache_keys_owned_by(**peer).await;
        }

        let conn_errors = self.update_routing_table(new_connections_results, peers_to_remove);

        // After ring update: evict main cache keys we no longer own
        self.evict_main_cache_keys_not_owned().await;

        if conn_errors.is_empty() {
            Ok(())
        } else {
            Err(GroupcacheError::from(
                InternalGroupcacheError::ConnectionErrors(conn_errors),
            ))
        }
    }

    /// Updates routing table by adding new successful connections and removing old peers
    fn update_routing_table(
        &self,
        connection_results: Vec<
            Result<(GroupcachePeer, GroupcachePeerClient), InternalGroupcacheError>,
        >,
        peers_to_remove: Vec<&GroupcachePeer>,
    ) -> Vec<InternalGroupcacheError> {
        let mut new_state = (**self.routing_state.load()).clone();

        let mut connection_errors = Vec::new();
        for result in connection_results {
            match result {
                Ok((peer, client)) => {
                    if self.invalidation.config().enabled {
                        self.invalidation
                            .spawn_watcher(peer, client.clone(), self.hot_cache.clone());
                    }
                    new_state.add_peer(peer, client);
                }
                Err(e) => {
                    connection_errors.push(e);
                }
            }
        }

        for removed_peer in peers_to_remove {
            self.invalidation.cancel_watcher(removed_peer);
            new_state.remove_peer(*removed_peer);
        }

        self.routing_state.store(Arc::new(new_state));

        connection_errors
    }

    /// Connects to new peers that were not previously connected in parallel.
    async fn connect_to_new_peers(
        &self,
        updated_peers: &HashSet<GroupcachePeer>,
        current_peers: &HashSet<GroupcachePeer>,
    ) -> Result<
        Vec<Result<(GroupcachePeer, GroupcachePeerClient), InternalGroupcacheError>>,
        GroupcacheError,
    > {
        let peers_to_connect = updated_peers.difference(current_peers);
        let mut connection_task = JoinSet::<
            Result<(GroupcachePeer, GroupcachePeerClient), InternalGroupcacheError>,
        >::new();
        for new_peer in peers_to_connect {
            let moved_peer = *new_peer;
            let https = self.config.https;
            let timeout = self.config.grpc_timeout;
            let grpc_endpoint_builder = self.config.grpc_endpoint_builder.clone();
            connection_task.spawn(async move {
                GroupcacheInner::<Value>::connect_static(moved_peer, https, timeout, grpc_endpoint_builder)
                    .await
            });
        }

        let mut results = Vec::with_capacity(connection_task.len());
        while let Some(res) = connection_task.join_next().await {
            let conn_result = res
                .context("unexpected JoinError when awaiting peer connection")
                .map_err(Anyhow)?;

            results.push(conn_result);
        }

        Ok(results)
    }

    async fn connect(
        &self,
        peer: GroupcachePeer,
    ) -> Result<(GroupcachePeer, GroupcachePeerClient), InternalGroupcacheError> {
        GroupcacheInner::<Value>::connect_static(
            peer,
            self.config.https,
            self.config.grpc_timeout,
            self.config.grpc_endpoint_builder.clone(),
        )
        .await
    }

    async fn connect_static(
        peer: GroupcachePeer,
        https: bool,
        timeout: std::time::Duration,
        grpc_endpoint_builder: Arc<Box<dyn Fn(Endpoint) -> Endpoint + Send + Sync + 'static>>,
    ) -> Result<(GroupcachePeer, GroupcachePeerClient), InternalGroupcacheError> {
        let socket = peer.socket;
        let peer_addr = if https {
            format!("https://{}", socket)
        } else {
            format!("http://{}", socket)
        };

        let endpoint: Endpoint = peer_addr.try_into()?;
        let endpoint = endpoint.timeout(timeout);
        let endpoint = grpc_endpoint_builder.as_ref()(endpoint);
        let client = GroupcacheClient::connect(endpoint).await?;
        Ok((peer, client))
    }

    pub(crate) async fn remove_peer(&self, peer: GroupcachePeer) -> Result<(), GroupcacheError> {
        let contains_peer = {
            let read_lock = self.routing_state.load();
            read_lock.contains_peer(&peer)
        };

        if !contains_peer {
            return Ok(());
        }

        self.invalidation.cancel_watcher(&peer);

        // Evict hot cache entries for keys owned by the departing peer
        // (before ring update, while we can still identify them)
        self.evict_hot_cache_keys_owned_by(peer).await;

        {
            let mut new_state = (**self.routing_state.load()).clone();
            new_state.remove_peer(peer);
            self.routing_state.store(Arc::new(new_state));
        }

        // After ring update: evict main cache keys we no longer own
        // (peer removal can shift keys away from this node)
        self.evict_main_cache_keys_not_owned().await;

        Ok(())
    }

    pub(crate) fn set_service_discovery_abort(&self, handle: AbortHandle) {
        let _ = self.service_discovery_abort.set(handle);
    }

    pub(crate) fn addr(&self) -> SocketAddr {
        self.me.socket
    }

    pub(crate) fn status(&self) -> crate::status::Status {
        use crate::status::{HealthState, Status};

        let peer_count = self.routing_state.load().peers().len();

        let health = if self.cancel.is_cancelled() {
            HealthState::ShuttingDown
        } else if peer_count == 0 {
            HealthState::NoPeers
        } else {
            HealthState::Healthy
        };

        Status {
            health,
            peer_count,
            main_cache_size: self.main_cache.entry_count(),
            hot_cache_size: self.hot_cache.entry_count(),
        }
    }

    /// Evict hot cache entries for keys currently owned by the given peer.
    /// Called before removing a peer from the ring, so ownership is still
    /// queryable from the current ring state.
    async fn evict_hot_cache_keys_owned_by(&self, peer: GroupcachePeer) {
        let keys_to_evict: Vec<Arc<str>> = {
            let lock = self.routing_state.load();
            self.hot_cache
                .iter()
                .filter(|(key, _)| {
                    let key_str: &str = key.as_ref();
                    lock.owner_of(key_str) == Some(peer)
                })
                .map(|(key, _)| {
                    let inner: &Arc<str> = &key;
                    Arc::clone(inner)
                })
                .collect()
        };
        for key in keys_to_evict {
            self.hot_cache.remove(key.as_ref()).await;
        }
    }

    /// Evict main cache entries for keys this node no longer owns.
    /// Called after a ring change (peer added) to clear keys that
    /// moved to the new peer.
    async fn evict_main_cache_keys_not_owned(&self) {
        let keys_to_evict: Vec<Arc<str>> = {
            let lock = self.routing_state.load();
            self.main_cache
                .iter()
                .filter(|(key, _)| {
                    let key_str: &str = key.as_ref();
                    lock.owner_of(key_str).is_some_and(|owner| owner != self.me)
                })
                .map(|(key, _)| {
                    let inner: &Arc<str> = &key;
                    Arc::clone(inner)
                })
                .collect()
        };
        for key in keys_to_evict {
            self.main_cache.remove(key.as_ref()).await;
        }
    }
}

impl<Value: ValueBounds> Drop for GroupcacheInner<Value> {
    fn drop(&mut self) {
        if let Some(handle) = self.service_discovery_abort.get() {
            handle.abort();
        }
    }
}
