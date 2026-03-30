use crate::cache::{Cache, ConsistentHashRing};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tokio::sync::RwLock;

const HOT_CACHE_TTL: Duration = Duration::from_secs(30);

pub struct GroupcacheNode {
    pub node_id: String,
    pub ring: ConsistentHashRing,
    pub main_cache: Cache,
    pub hot_cache: Cache,
    versions: RwLock<HashMap<String, AtomicU64>>,
}

impl GroupcacheNode {
    pub fn new(node_id: String, all_node_ids: Vec<String>) -> Self {
        Self {
            ring: ConsistentHashRing::new(&all_node_ids),
            node_id,
            main_cache: Cache::new(None),
            hot_cache: Cache::new(Some(HOT_CACHE_TTL)),
            versions: RwLock::new(HashMap::new()),
        }
    }

    pub fn is_owner(&self, key: &str) -> bool {
        self.ring.owner(key) == self.node_id
    }

    pub fn owner_of(&self, key: &str) -> &str {
        self.ring.owner(key)
    }

    pub async fn next_value(&self, key: &str) -> String {
        let mut versions = self.versions.write().await;
        let counter = versions
            .entry(key.to_string())
            .or_insert_with(|| AtomicU64::new(0));
        let version = counter.fetch_add(1, Ordering::SeqCst) + 1;
        format!("{}:{}", self.node_id, version)
    }

    pub async fn local_lookup(&self, key: &str) -> Option<String> {
        if let Some(v) = self.main_cache.get(key).await {
            return Some(v);
        }
        self.hot_cache.get(key).await
    }
}
