use crate::groupcache::{GroupcachePeer, GroupcachePeerClient, ValueBounds};
use crate::metrics::{
    METRIC_INVALIDATION_BROADCAST_TOTAL, METRIC_INVALIDATION_RECEIVED_TOTAL,
    METRIC_INVALIDATION_STREAM_DISCONNECT_TOTAL,
};
use groupcache_pb::{InvalidationEvent, WatchRequest};
use log::{error, info, warn};
use metrics::counter;
use moka::future::Cache;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio_stream::wrappers::BroadcastStream;
use tokio_stream::StreamExt;
use tonic::IntoRequest;

/// Capacity of the broadcast channel. Events that arrive when all receivers are
/// this far behind are dropped (receivers will detect the lag and flush).
const BROADCAST_CHANNEL_CAPACITY: usize = 4096;

/// Configuration for invalidation streaming.
#[derive(Clone)]
pub(crate) struct InvalidationConfig {
    /// Whether streaming invalidation is enabled.
    pub(crate) enabled: bool,
    /// Base delay for exponential backoff on reconnection.
    pub(crate) reconnect_base_delay: Duration,
    /// Maximum delay for exponential backoff on reconnection.
    pub(crate) reconnect_max_delay: Duration,
}

impl Default for InvalidationConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            reconnect_base_delay: Duration::from_millis(100),
            reconnect_max_delay: Duration::from_secs(5),
        }
    }
}

/// Manages invalidation event broadcasting and watcher streams.
pub(crate) struct InvalidationManager {
    sender: broadcast::Sender<String>,
    config: InvalidationConfig,
    cancel: tokio_util::sync::CancellationToken,
    watchers: Mutex<HashMap<GroupcachePeer, JoinHandle<()>>>,
}

impl InvalidationManager {
    pub(crate) fn new(config: InvalidationConfig, cancel: tokio_util::sync::CancellationToken) -> Self {
        let (sender, _) = broadcast::channel(BROADCAST_CHANNEL_CAPACITY);
        Self {
            cancel,
            sender,
            config,
            watchers: Mutex::new(HashMap::new()),
        }
    }

    pub(crate) fn config(&self) -> &InvalidationConfig {
        &self.config
    }

    /// Broadcast an invalidation event for a key to all connected watchers.
    /// Called by the key owner when a key is removed from main_cache.
    pub(crate) fn broadcast(&self, key: &str) {
        counter!(METRIC_INVALIDATION_BROADCAST_TOTAL).increment(1);
        // send() returns Err only if there are no receivers -- that's fine,
        // it means no peers are watching yet.
        let _ = self.sender.send(key.to_string());
    }

    /// Create a new stream for a watcher (server-side of WatchInvalidations RPC).
    /// Returns a stream of InvalidationEvent that tonic can serve.
    pub(crate) fn subscribe_stream(
        &self,
    ) -> impl tokio_stream::Stream<Item = Result<InvalidationEvent, tonic::Status>> {
        let rx = self.sender.subscribe();
        BroadcastStream::new(rx).filter_map(|result| match result {
            Ok(key) => Some(Ok(InvalidationEvent { key })),
            Err(tokio_stream::wrappers::errors::BroadcastStreamRecvError::Lagged(n)) => {
                warn!("Invalidation watcher lagged by {} events", n);
                // Return empty-key event that the client interprets as "flush all"
                Some(Ok(InvalidationEvent {
                    key: String::new(),
                }))
            }
        })
    }

    /// Spawn a background task that watches a remote peer's invalidation stream
    /// and removes keys from the local hot_cache.
    pub(crate) fn spawn_watcher<V: ValueBounds>(
        &self,
        peer: GroupcachePeer,
        mut client: GroupcachePeerClient,
        hot_cache: Cache<Arc<str>, V>,
    ) {
        let config = self.config.clone();
        let peer_socket = peer.socket;
        let cancel = self.cancel.clone();

        let handle = tokio::spawn(async move {
            let mut backoff = config.reconnect_base_delay;

            loop {
                if cancel.is_cancelled() {
                    break;
                }

                match client
                    .watch_invalidations(WatchRequest {}.into_request())
                    .await
                {
                    Ok(response) => {
                        backoff = config.reconnect_base_delay; // reset on successful connect
                        let mut stream = response.into_inner();
                        info!("Connected invalidation watcher to peer {:?}", peer_socket);

                        while let Some(result) = stream.next().await {
                            match result {
                                Ok(event) => {
                                    if event.key.is_empty() {
                                        // Empty key = "flush all" signal (lagged or reconnect)
                                        warn!(
                                            "Received flush signal from peer {:?}, invalidating entire hot cache",
                                            peer_socket
                                        );
                                        hot_cache.invalidate_all();
                                        counter!(METRIC_INVALIDATION_STREAM_DISCONNECT_TOTAL)
                                            .increment(1);
                                    } else {
                                        hot_cache.remove(event.key.as_str()).await;
                                        counter!(METRIC_INVALIDATION_RECEIVED_TOTAL).increment(1);
                                    }
                                }
                                Err(status) => {
                                    warn!(
                                        "Invalidation stream error from peer {:?}: {}",
                                        peer_socket, status
                                    );
                                    break;
                                }
                            }
                        }
                    }
                    Err(status) => {
                        error!(
                            "Failed to open invalidation stream to peer {:?}: {}",
                            peer_socket, status
                        );
                    }
                }

                // Stream ended or failed to connect -- flush hot cache and reconnect
                warn!(
                    "Invalidation stream to peer {:?} disconnected, flushing hot cache",
                    peer_socket
                );
                hot_cache.invalidate_all();
                counter!(METRIC_INVALIDATION_STREAM_DISCONNECT_TOTAL).increment(1);

                tokio::select! {
                    _ = tokio::time::sleep(backoff) => {}
                    _ = cancel.cancelled() => break,
                }
                backoff = (backoff * 2).min(config.reconnect_max_delay);
            }
        });

        let mut watchers = self.watchers.lock().unwrap();
        if let Some(old_handle) = watchers.insert(peer, handle) {
            old_handle.abort();
        }
    }

    pub(crate) fn cancel_watcher(&self, peer: &GroupcachePeer) {
        let mut watchers = self.watchers.lock().unwrap();
        if let Some(handle) = watchers.remove(peer) {
            handle.abort();
        }
    }
}
