use hashring::HashRing;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

const VNODES_PER_NODE: usize = 40;

#[derive(Clone)]
pub struct ConsistentHashRing {
    ring: HashRing<String>,
    nodes: Vec<String>,
}

impl ConsistentHashRing {
    pub fn new(node_ids: &[String]) -> Self {
        let mut ring = HashRing::new();
        for node_id in node_ids {
            for i in 0..VNODES_PER_NODE {
                ring.add(format!("{}_{}", node_id, i));
            }
        }
        Self {
            ring,
            nodes: node_ids.to_vec(),
        }
    }

    pub fn owner(&self, key: &str) -> &str {
        let vnode = self.ring.get(&key).expect("ring cannot be empty");
        let node_id = vnode.split('_').next().unwrap();
        self.nodes.iter().find(|n| n.as_str() == node_id).unwrap()
    }
}

struct CacheEntry {
    value: String,
    inserted_at: Instant,
}

pub struct Cache {
    entries: RwLock<HashMap<String, CacheEntry>>,
    ttl: Option<Duration>,
}

impl Cache {
    pub fn new(ttl: Option<Duration>) -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            ttl,
        }
    }

    pub async fn get(&self, key: &str) -> Option<String> {
        let entries = self.entries.read().await;
        if let Some(entry) = entries.get(key) {
            if let Some(ttl) = self.ttl {
                if entry.inserted_at.elapsed() > ttl {
                    drop(entries);
                    self.entries.write().await.remove(key);
                    return None;
                }
            }
            Some(entry.value.clone())
        } else {
            None
        }
    }

    pub async fn insert(&self, key: String, value: String) {
        self.entries.write().await.insert(
            key,
            CacheEntry {
                value,
                inserted_at: Instant::now(),
            },
        );
    }

    pub async fn remove(&self, key: &str) {
        self.entries.write().await.remove(key);
    }

    pub async fn invalidate_all(&self) {
        self.entries.write().await.clear();
    }
}
