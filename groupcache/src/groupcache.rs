use crate::errors::{DedupedGroupcacheError, GroupcacheError, InternalGroupcacheError};
use crate::routing::RoutingState;
use crate::{Key, Peer, ValueBounds, ValueLoader};
use anyhow::Result;
use groupcache_pb::groupcache_pb::groupcache_client::GroupcacheClient;
use groupcache_pb::groupcache_pb::GetRequest;
use quick_cache::sync::Cache;
use singleflight_async::SingleFlight;
use std::sync::{Arc, RwLock};
use tonic::IntoRequest;
use tracing::log;

pub struct Groupcache<Value: ValueBounds> {
    routing_state: Arc<RwLock<RoutingState>>,
    single_flight_group: SingleFlight<Result<Value, DedupedGroupcacheError>>,
    cache: Cache<String, Value>,
    loader: Box<dyn ValueLoader<Value = Value>>,
    pub(crate) me: Peer,
}

impl<Value: ValueBounds> Groupcache<Value> {
    pub fn new(me: Peer, loader: Box<dyn ValueLoader<Value = Value>>) -> Self {
        let routing_state = Arc::new(RwLock::new(RoutingState::with_local_peer(me)));
        // todo: cache capacity should be configurable
        let cache = Cache::new(1_000_000);
        let single_flight_group = SingleFlight::default();

        Self {
            me,
            routing_state,
            single_flight_group,
            cache,
            loader,
        }
    }

    pub(crate) async fn get(&self, key: &Key) -> core::result::Result<Value, GroupcacheError> {
        Ok(self.get_internal(key).await?)
    }

    async fn get_internal(&self, key: &Key) -> Result<Value, InternalGroupcacheError> {
        if let Some(value) = self.cache.get(key) {
            log::info!("peer {:?} serving from cache: {:?}", self.me.socket, key);
            return Ok(value);
        }

        let peer = {
            let lock = self.routing_state.read().unwrap();

            lock.peer_for_key(key)?
        };
        log::info!(
            "peer {:?} getting from peer: {:?}",
            self.me.socket,
            peer.socket
        );

        let value = self.get_dedup(key, peer).await?;
        Ok(value)
    }

    async fn get_dedup(&self, key: &Key, peer: Peer) -> Result<Value> {
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

    async fn dedup_get(&self, key: &Key, peer: Peer) -> Result<Value> {
        let value = if peer == self.me {
            let value = self.load_locally(key).await?;
            self.cache.insert(key.to_string(), value.clone());
            value
        } else {
            let value = self.load_remotely(key, peer).await?;
            value
        };

        Ok(value)
    }

    async fn load_locally(&self, key: &Key) -> Result<Value, InternalGroupcacheError> {
        self.loader
            .load(key)
            .await
            .map_err(InternalGroupcacheError::Loader)
    }

    async fn load_remotely(&self, key: &Key, peer: Peer) -> Result<Value> {
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

    pub(crate) async fn add_peer(&self, peer: Peer) -> Result<()> {
        let contains_peer = {
            let read_lock = self.routing_state.read().unwrap();
            read_lock.contains_peer(&peer)
        };

        if contains_peer {
            return Ok(());
        }

        let peer_server_address = format!("http://{}", peer.socket.clone());

        let client = GroupcacheClient::connect(peer_server_address).await?;

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
