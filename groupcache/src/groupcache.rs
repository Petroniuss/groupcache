use crate::errors::{DedupedGroupcacheError, GroupcacheError, InternalGroupcacheError};
use crate::routing::RoutingState;
use crate::{Key, Options, Peer, ValueBounds, ValueLoader};
use anyhow::Result;
use groupcache_pb::groupcache_pb::groupcache_client::GroupcacheClient;
use groupcache_pb::groupcache_pb::{GetRequest, RemoveRequest};
use moka::future::Cache;
use singleflight_async::SingleFlight;
use std::sync::{Arc, RwLock};
use tonic::IntoRequest;

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
        if let Some(value) = self.cache.get(key).await {
            return Ok(value);
        }

        if let Some(value) = self.hot_cache.get(key).await {
            return Ok(value);
        }

        let peer = {
            let lock = self.routing_state.read().unwrap();
            lock.peer_for_key(key)?
        };

        let value = self.get_dedup(key, peer).await?;
        Ok(value)
    }

    async fn get_dedup(&self, key: &Key, peer: Peer) -> Result<Value, InternalGroupcacheError> {
        let value = self
            .single_flight_group
            .work(key, || async { self.error_wrapped_dedup(key, peer).await })
            .await?;

        Ok(value)
    }

    async fn error_wrapped_dedup(
        &self,
        key: &Key,
        peer: Peer,
    ) -> Result<Value, DedupedGroupcacheError> {
        self.dedup_get(key, peer)
            .await
            .map_err(|e| DedupedGroupcacheError(Arc::new(e)))
    }

    async fn dedup_get(&self, key: &Key, peer: Peer) -> Result<Value, InternalGroupcacheError> {
        let value = if peer == self.me {
            let value = self.load_locally(key).await?;
            self.cache.insert(key.to_string(), value.clone()).await;

            value
        } else {
            let value = self.load_remotely(key, peer).await?;
            self.hot_cache.insert(key.to_string(), value.clone()).await;

            value
        };

        Ok(value)
    }

    async fn load_locally(&self, key: &Key) -> Result<Value, InternalGroupcacheError> {
        self.loader
            .load(key)
            .await
            .map_err(InternalGroupcacheError::LocalLoader)
    }

    async fn load_remotely(&self, key: &Key, peer: Peer) -> Result<Value, InternalGroupcacheError> {
        let mut client = {
            let read_lock = self.routing_state.read().unwrap();
            read_lock.client_for_peer(&peer)?
        };

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
            lock.peer_for_key(key)?
        };

        if peer == self.me {
            self.cache.remove(key).await;
        } else {
            self.remove_remotely(key, peer).await?;
        }

        Ok(())
    }

    async fn remove_remotely(
        &self,
        key: &Key,
        peer: Peer,
    ) -> core::result::Result<(), InternalGroupcacheError> {
        let mut client = {
            let read_lock = self.routing_state.read().unwrap();
            read_lock.client_for_peer(&peer)?
        };

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
