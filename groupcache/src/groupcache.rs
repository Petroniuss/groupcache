use crate::errors::{DedupedGroupcacheError, GroupcacheError, InternalGroupcacheError};
use crate::routing::{PeerWithClient, RoutingState};
use crate::{Key, Options, Peer, PeerClient, ValueBounds, ValueLoader};
use anyhow::{Context, Result};
use groupcache_pb::groupcache_pb::groupcache_client::GroupcacheClient;
use groupcache_pb::groupcache_pb::{GetRequest, RemoveRequest};
use metrics::counter;
use moka::future::Cache;
use singleflight_async::SingleFlight;
use std::sync::{Arc, RwLock};
use tonic::IntoRequest;

const METRIC_GET_TOTAL: &str = "groupcache.get_total";
pub(crate) const METRIC_GET_SERVER_REQUESTS_TOTAL: &str = "groupcache.get_server_requests_total";
const METRIC_LOCAL_CACHE_HIT_TOTAL: &str = "groupcache.local_cache_hit_total";
const METRIC_LOCAL_LOAD_TOTAL: &str = "groupcache.local_load_total";
const METRIC_LOCAL_LOAD_ERROR_TOTAL: &str = "groupcache.local_load_errors";
const METRIC_REMOTE_LOAD_TOTAL: &str = "groupcache.remote_load_total";
const METRIC_REMOTE_LOAD_ERROR: &str = "groupcache.remote_load_errors";

pub struct Groupcache<Value: ValueBounds> {
    routing_state: Arc<RwLock<RoutingState>>,
    single_flight_group: SingleFlight<Result<Value, DedupedGroupcacheError>>,
    // cache is used for values that are owned by this peer.
    cache: Cache<String, Value>,
    // hot_cache is used for caching values that are owned by other peers.
    hot_cache: Cache<String, Value>,
    loader: Box<dyn ValueLoader<Value = Value>>,
    pub(crate) me: Peer,
}

impl<Value: ValueBounds> Groupcache<Value> {
    pub(crate) fn new(
        me: Peer,
        loader: Box<dyn ValueLoader<Value = Value>>,
        options: Options,
    ) -> Self {
        let routing_state = Arc::new(RwLock::new(RoutingState::with_local_peer(me)));

        let cache = Cache::<String, Value>::builder()
            .max_capacity(options.cache_capacity)
            .build();

        let hot_cache = Cache::<String, Value>::builder()
            .max_capacity(options.hot_cache_capacity)
            .time_to_live(options.hot_cache_ttl)
            .build();

        let single_flight_group = SingleFlight::default();

        Self {
            routing_state,
            single_flight_group,
            cache,
            hot_cache,
            loader,
            me,
        }
    }

    pub(crate) async fn get(&self, key: &Key) -> core::result::Result<Value, GroupcacheError> {
        Ok(self.get_internal(key).await?)
    }

    pub(crate) async fn remove(&self, key: &Key) -> core::result::Result<(), GroupcacheError> {
        Ok(self.remove_internal(key).await?)
    }

    async fn get_internal(&self, key: &Key) -> Result<Value, InternalGroupcacheError> {
        counter!(METRIC_GET_TOTAL, 1);
        if let Some(value) = self.cache.get(key).await {
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
        key: &Key,
        peer: PeerWithClient,
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
        key: &Key,
        peer: PeerWithClient,
    ) -> Result<Value, InternalGroupcacheError> {
        if peer.peer == self.me {
            let value = self.load_locally_instrumented(key).await?;
            self.cache.insert(key.to_string(), value.clone()).await;
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

    async fn load_locally_instrumented(&self, key: &Key) -> Result<Value, InternalGroupcacheError> {
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
        key: &Key,
        client: &mut PeerClient,
    ) -> Result<Value, InternalGroupcacheError> {
        counter!(METRIC_REMOTE_LOAD_TOTAL, 1);
        self.load_remotely(key, client).await.map_err(|e| {
            counter!(METRIC_REMOTE_LOAD_ERROR, 1);
            e
        })
    }

    async fn load_remotely(
        &self,
        key: &Key,
        client: &mut PeerClient,
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
        key: &Key,
    ) -> core::result::Result<(), InternalGroupcacheError> {
        self.hot_cache.remove(key).await;

        let peer = {
            let lock = self.routing_state.read().unwrap();
            lock.lookup_peer(key)
        }?;

        if peer.peer == self.me {
            self.cache.remove(key).await;
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
        key: &Key,
        client: &mut PeerClient,
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

    pub(crate) async fn add_peer(&self, peer: Peer) -> Result<()> {
        let contains_peer = {
            let read_lock = self.routing_state.read().unwrap();
            read_lock.contains_peer(&peer)
        };

        if contains_peer {
            return Ok(());
        }

        let client = GroupcacheClient::connect(peer.addr()).await?;

        let mut write_lock = self.routing_state.write().unwrap();
        write_lock.add_peer(peer, client);

        Ok(())
    }

    pub(crate) async fn remove_peer(&self, peer: Peer) -> Result<()> {
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
}
