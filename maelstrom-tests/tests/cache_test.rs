use groupcache_maelstrom::cache::{Cache, ConsistentHashRing};
use std::time::Duration;

#[test]
fn hash_ring_deterministic_ownership() {
    let nodes = vec!["n1".into(), "n2".into(), "n3".into()];
    let ring = ConsistentHashRing::new(&nodes);
    let owner1 = ring.owner("key-abc");
    let owner2 = ring.owner("key-abc");
    assert_eq!(owner1, owner2);
}

#[test]
fn hash_ring_distributes_keys() {
    let nodes = vec!["n1".into(), "n2".into(), "n3".into()];
    let ring = ConsistentHashRing::new(&nodes);
    let mut owners = std::collections::HashSet::new();
    for i in 0..100 {
        owners.insert(ring.owner(&format!("key-{}", i)).to_string());
    }
    assert_eq!(owners.len(), 3);
}

#[test]
fn hash_ring_owner_is_valid_node() {
    let nodes = vec!["n1".into(), "n2".into(), "n3".into()];
    let ring = ConsistentHashRing::new(&nodes);
    let owner = ring.owner("any-key");
    assert!(nodes.contains(&owner.to_string()));
}

#[tokio::test]
async fn cache_insert_and_get() {
    let cache = Cache::new(None);
    cache.insert("k1".into(), "v1".into()).await;
    assert_eq!(cache.get("k1").await, Some("v1".into()));
}

#[tokio::test]
async fn cache_remove() {
    let cache = Cache::new(None);
    cache.insert("k1".into(), "v1".into()).await;
    cache.remove("k1").await;
    assert_eq!(cache.get("k1").await, None);
}

#[tokio::test]
async fn cache_ttl_expiry() {
    let cache = Cache::new(Some(Duration::from_millis(50)));
    cache.insert("k1".into(), "v1".into()).await;
    assert_eq!(cache.get("k1").await, Some("v1".into()));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(cache.get("k1").await, None);
}

#[tokio::test]
async fn cache_invalidate_all() {
    let cache = Cache::new(None);
    cache.insert("k1".into(), "v1".into()).await;
    cache.insert("k2".into(), "v2".into()).await;
    cache.invalidate_all().await;
    assert_eq!(cache.get("k1").await, None);
    assert_eq!(cache.get("k2").await, None);
}
