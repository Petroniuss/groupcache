mod common;

use anyhow::Result;
use common::*;
use groupcache::{Groupcache, GroupcachePeer};
use pretty_assertions::assert_eq;
use std::collections::HashSet;

use std::net::SocketAddr;
use std::ops::Sub;
use std::time::Duration;
use tokio::time;

#[tokio::test]
async fn test_when_there_is_only_one_peer_it_should_handle_entire_key_space() -> Result<()> {
    let groupcache = {
        let loader = TestCacheLoader::new("1");
        let addr: SocketAddr = "127.0.0.1:8080".parse()?;
        Groupcache::<CachedValue>::builder(addr.into(), loader, groupcache::CancellationToken::new()).build()
    };

    let key = "K-some-random-key-d2k";
    successful_get(key, Some("1"), groupcache.clone()).await;
    successful_get(key, Some("1"), groupcache.clone()).await;

    let error_key = "Key-triggering-loading-error";
    let err = groupcache
        .get(error_key)
        .await
        .expect_err("expected error from loader");

    assert_eq!(
        "Loading error: 'Something bad happened during loading :/'",
        err.to_string()
    );

    Ok(())
}

#[tokio::test]
async fn test_set_peers_updates_routing_table() -> Result<()> {
    let peers = spawn_instances(5).await?;

    let (me, others) = peers.split_first().unwrap();
    let peers = others
        .iter()
        .map(|e| e.addr().into())
        .collect::<HashSet<GroupcachePeer>>();

    me.set_peers(peers.clone()).await?;
    for i in 1..5 {
        let peer = others[i - 1].clone();
        let key = key_owned_by_instance(peer);
        let peer_index = &*i.to_string();
        successful_get(&key, Some(peer_index), me.clone()).await;
    }

    let tbr: HashSet<GroupcachePeer> = vec![others[3].addr().into()].into_iter().collect();
    let subset = peers.sub(&tbr);
    me.set_peers(subset.clone()).await?;

    let key = key_owned_by_instance(others[3].clone());
    successful_get_not_from_instance(&key, "4", me.clone()).await;

    Ok(())
}

#[tokio::test]
async fn test_healthy_peers_added_despite_one_unhealthy() -> Result<()> {
    let peers = spawn_instances(5).await?;

    let (me, others) = peers.split_first().unwrap();
    let mut peers = others
        .iter()
        .map(|e| e.addr().into())
        .collect::<HashSet<GroupcachePeer>>();

    // Bind a port, then immediately drop the listener so the port is free
    // but nothing is listening. This avoids flakiness from hardcoded ports
    // that may be occupied by other processes.
    let unreachable_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let unreachable_addr = unreachable_listener.local_addr()?;
    drop(unreachable_listener);
    peers.insert(unreachable_addr.into());
    let result = me.set_peers(peers.clone()).await;
    if result.is_ok() {
        panic!("Expected failure to set peers with unreachable address");
    }

    for i in 1..5 {
        let peer = others[i - 1].clone();
        let key = key_owned_by_instance(peer);
        let peer_index = &*i.to_string();
        successful_get(&key, Some(peer_index), me.clone()).await;
    }

    Ok(())
}

#[tokio::test]
async fn test_when_peers_are_healthy_they_should_respond_to_queries() -> Result<()> {
    let (instance_one, instance_two) = two_connected_instances().await?;

    for &key in ["K-b3a", "K-karo", "K-x3d", "K-d42", "W-a1a"].iter() {
        successful_get(key, None, instance_one.clone()).await;
        successful_get(key, None, instance_two.clone()).await;
    }

    Ok(())
}

#[tokio::test]
async fn test_when_peer_disconnects_requests_should_fail_with_transport_error() -> Result<()> {
    let (instance_one, instance_two) = two_instances_with_one_disconnected().await?;
    for &key in ["K-b3a", "K-karo", "K-x3d", "K-d42", "W-a1a"].iter() {
        success_or_transport_err(key, instance_one.clone()).await;
        success_or_transport_err(key, instance_two.clone()).await;
    }

    Ok(())
}

#[ignore]
#[tokio::test]
async fn test_when_peer_reconnects_it_should_respond_to_queries() -> Result<()> {
    let (instance_one, instance_two) = two_instances_with_one_disconnected().await?;
    reconnect(instance_two.clone()).await;

    let key_on_instance_2 = key_owned_by_instance(instance_two.clone());
    success_or_transport_err(&key_on_instance_2, instance_one.clone()).await;
    successful_get(&key_on_instance_2, Some("2"), instance_one.clone()).await;

    Ok(())
}

#[tokio::test]
async fn when_new_peer_joins_it_should_receive_requests() -> Result<()> {
    let (instance_one, instance_two) = two_instances().await?;
    let key_on_instance_2 = key_owned_by_instance(instance_two.clone());
    successful_get(&key_on_instance_2, Some("2"), instance_two.clone()).await;

    instance_one.add_peer(instance_two.addr().into()).await?;
    successful_get(&key_on_instance_2, Some("2"), instance_one.clone()).await;

    Ok(())
}

#[tokio::test]
async fn test_when_remote_get_fails_during_load_then_load_locally() -> Result<()> {
    let (instance_one, instance_two) = two_connected_instances().await?;
    let key_on_instance_2 = error_key_on_instance(instance_two.clone());

    let err = instance_one
        .get(&key_on_instance_2)
        .await
        .expect_err("expected error from loader");

    assert_eq!(
        "Loading error: 'Something bad happened during loading :/'",
        err.to_string()
    );

    Ok(())
}

#[tokio::test]
async fn test_when_peer_is_removed_traffic_is_no_longer_routed_to_it() -> Result<()> {
    let (instance_one, instance_two) = two_connected_instances().await?;
    instance_one
        .remove_peer(GroupcachePeer::from_socket(instance_two.addr()))
        .await?;
    let key = key_owned_by_instance(instance_two.clone());

    successful_get(&key, Some("1"), instance_one.clone()).await;
    Ok(())
}

#[tokio::test]
async fn test_when_there_are_multiple_instances_each_should_own_portion_of_key_space() -> Result<()>
{
    let instances = spawn_instances(10).await?;
    let first_instance = instances[0].clone();

    for (i, instance) in instances.iter().enumerate() {
        let key_on_instance = key_owned_by_instance(instance.clone());
        successful_get(
            &key_on_instance,
            Some(&i.to_string()),
            first_instance.clone(),
        )
        .await;
    }

    Ok(())
}

#[tokio::test]
async fn when_kv_is_loaded_it_should_be_cached_by_the_owner() -> Result<()> {
    let (instance_one, instance_two) = two_connected_instances().await?;
    let key_on_instance_2 = key_owned_by_instance(instance_two.clone());

    successful_get_opts(
        &key_on_instance_2,
        instance_one.clone(),
        GetAssertions {
            expected_instance_id: Some("2".into()),
            expected_load_count: Some(1),
            ..GetAssertions::default()
        },
    )
    .await;

    successful_get_opts(
        &key_on_instance_2,
        instance_two.clone(),
        GetAssertions {
            expected_instance_id: Some("2".into()),
            expected_load_count: Some(1),
            ..GetAssertions::default()
        },
    )
    .await;

    Ok(())
}

#[tokio::test]
async fn when_kv_is_loaded_it_should_be_cached_in_hot_cache() -> Result<()> {
    let (instance_one, instance_two) = two_connected_instances().await?;
    let key_on_instance_2 = key_owned_by_instance(instance_two.clone());

    successful_get_opts(
        &key_on_instance_2,
        instance_one.clone(),
        GetAssertions {
            expected_instance_id: Some("2".into()),
            expected_load_count: Some(1),
            ..GetAssertions::default()
        },
    )
    .await;

    instance_two.remove(&key_on_instance_2).await?;

    // With streaming invalidation, hot cache is cleared near-realtime.
    time::sleep(INVALIDATION_PROPAGATION_DELAY).await;

    // Value should be reloaded (load count = 2) because streaming
    // invalidation cleared instance_one's hot cache.
    successful_get_opts(
        &key_on_instance_2,
        instance_one.clone(),
        GetAssertions {
            expected_instance_id: Some("2".into()),
            expected_load_count: Some(2),
            ..GetAssertions::default()
        },
    )
    .await;

    Ok(())
}

#[tokio::test]
async fn when_kv_is_saved_in_hot_cache_it_should_expire_according_to_ttl() -> Result<()> {
    let (instance_one, instance_two) = two_connected_instances().await?;
    let key_on_instance_2 = key_owned_by_instance(instance_two.clone());

    successful_get_opts(
        &key_on_instance_2,
        instance_one.clone(),
        GetAssertions {
            expected_instance_id: Some("2".into()),
            expected_load_count: Some(1),
            ..GetAssertions::default()
        },
    )
    .await;

    instance_two.remove(&key_on_instance_2).await?;
    time::sleep(HOT_CACHE_TTL).await;

    successful_get_opts(
        &key_on_instance_2,
        instance_one.clone(),
        GetAssertions {
            expected_instance_id: Some("2".into()),
            expected_load_count: Some(2),
            ..GetAssertions::default()
        },
    )
    .await;

    Ok(())
}

#[tokio::test]
async fn when_key_is_removed_then_it_should_be_removed_from_owner() -> Result<()> {
    let (instance_one, instance_two) = two_connected_instances().await?;
    let key_on_instance_2 = key_owned_by_instance(instance_two.clone());

    successful_get_opts(
        &key_on_instance_2,
        instance_one.clone(),
        GetAssertions {
            expected_instance_id: Some("2".into()),
            expected_load_count: Some(1),
            ..GetAssertions::default()
        },
    )
    .await;

    instance_one.remove(&key_on_instance_2).await?;

    successful_get_opts(
        &key_on_instance_2,
        instance_two.clone(),
        GetAssertions {
            expected_instance_id: Some("2".into()),
            expected_load_count: Some(2),
            ..GetAssertions::default()
        },
    )
    .await;

    Ok(())
}

#[tokio::test]
async fn when_key_is_removed_hot_cache_on_other_peers_should_be_invalidated_via_stream(
) -> Result<()> {
    let (instance_one, instance_two) = two_connected_instances().await?;
    let key_on_instance_2 = key_owned_by_instance(instance_two.clone());

    // Instance 1 fetches key from instance 2 -> cached in instance 1's hot cache
    successful_get_opts(
        &key_on_instance_2,
        instance_one.clone(),
        GetAssertions {
            expected_instance_id: Some("2".into()),
            expected_load_count: Some(1),
            ..GetAssertions::default()
        },
    )
    .await;

    // Instance 2 (owner) removes the key
    instance_two.remove(&key_on_instance_2).await?;

    // Wait for invalidation event to propagate via stream
    time::sleep(INVALIDATION_PROPAGATION_DELAY).await;

    // Instance 1 should now reload from instance 2 (hot cache was cleared by stream)
    successful_get_opts(
        &key_on_instance_2,
        instance_one.clone(),
        GetAssertions {
            expected_instance_id: Some("2".into()),
            expected_load_count: Some(2),
            ..GetAssertions::default()
        },
    )
    .await;

    Ok(())
}

#[tokio::test]
async fn when_non_owner_removes_key_all_hot_caches_should_be_invalidated() -> Result<()> {
    let (instance_one, instance_two) = two_connected_instances().await?;
    let key_on_instance_2 = key_owned_by_instance(instance_two.clone());

    // Instance 1 fetches key -> cached in instance 1's hot cache
    successful_get_opts(
        &key_on_instance_2,
        instance_one.clone(),
        GetAssertions {
            expected_instance_id: Some("2".into()),
            expected_load_count: Some(1),
            ..GetAssertions::default()
        },
    )
    .await;

    // Instance 1 (non-owner) calls remove -> routes Remove RPC to instance 2 (owner)
    // Instance 2 broadcasts invalidation event -> instance 1's watcher clears hot cache
    instance_one.remove(&key_on_instance_2).await?;

    // Wait for broadcast propagation
    time::sleep(INVALIDATION_PROPAGATION_DELAY).await;

    // Instance 1 re-fetches -> should reload since both its hot cache
    // (cleared by remove() + stream) and instance 2's main cache (cleared by Remove RPC) are empty
    successful_get_opts(
        &key_on_instance_2,
        instance_one.clone(),
        GetAssertions {
            expected_instance_id: Some("2".into()),
            expected_load_count: Some(2),
            ..GetAssertions::default()
        },
    )
    .await;

    Ok(())
}

// ---------------------------------------------------------------------------
// service_discovery.rs: exercise run_service_discovery loop
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_service_discovery_default_interval_is_used() -> Result<()> {
    let sd = DefaultIntervalServiceDiscovery {
        peers: HashSet::new(),
    };

    let instance = spawn_groupcache_with_service_discovery("sd-default", sd).await?;

    // Give it a moment to execute at least one pull and call interval().
    time::sleep(Duration::from_millis(50)).await;

    // Verify the instance is functional
    let key = "sd-default-key";
    successful_get(key, Some("sd-default"), instance.clone()).await;

    Ok(())
}

#[tokio::test]
async fn test_service_discovery_with_unreachable_peer_logs_error() -> Result<()> {
    let unreachable_peer: GroupcachePeer = "127.0.0.1:19999".parse::<SocketAddr>()?.into();
    let peers: HashSet<GroupcachePeer> = vec![unreachable_peer].into_iter().collect();
    let sd = TestServiceDiscovery::new(peers, Duration::from_millis(50));

    let _instance = spawn_groupcache_with_service_discovery("sd-unreachable", sd).await?;

    time::sleep(Duration::from_millis(200)).await;

    Ok(())
}

#[tokio::test]
async fn test_service_discovery_pulls_peers_and_updates_routing() -> Result<()> {
    let discovered_peer = spawn_groupcache("sd-peer").await?;

    let peers: HashSet<GroupcachePeer> = vec![discovered_peer.addr().into()].into_iter().collect();
    let sd = TestServiceDiscovery::new(peers, Duration::from_millis(50));

    let instance = spawn_groupcache_with_service_discovery("sd-main", sd).await?;

    time::sleep(Duration::from_millis(200)).await;

    let key_on_discovered = key_owned_by_instance(discovered_peer.clone());
    successful_get(&key_on_discovered, Some("sd-peer"), instance.clone()).await;

    Ok(())
}

#[tokio::test]
async fn test_service_discovery_error_does_not_crash_loop() -> Result<()> {
    let discovered_peer = spawn_groupcache("sd-err-peer").await?;

    let peers: HashSet<GroupcachePeer> = vec![discovered_peer.addr().into()].into_iter().collect();
    let sd = TestServiceDiscovery::new(peers.clone(), Duration::from_millis(50));
    let error_flag = sd.error_on_next.clone();

    *error_flag.write().unwrap() = true;

    let instance = spawn_groupcache_with_service_discovery("sd-err-main", sd).await?;

    time::sleep(Duration::from_millis(200)).await;

    *error_flag.write().unwrap() = false;
    time::sleep(Duration::from_millis(200)).await;

    let key_on_discovered = key_owned_by_instance(discovered_peer.clone());
    successful_get(&key_on_discovered, Some("sd-err-peer"), instance.clone()).await;

    Ok(())
}

// ---------------------------------------------------------------------------
// groupcache_inner.rs: hot cache hit path
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_hot_cache_hit_returns_cached_value_without_reload() -> Result<()> {
    let (instance_one, instance_two) = two_connected_instances().await?;
    let key_on_instance_2 = key_owned_by_instance(instance_two.clone());

    successful_get_opts(
        &key_on_instance_2,
        instance_one.clone(),
        GetAssertions {
            expected_instance_id: Some("2".into()),
            expected_load_count: Some(1),
            ..GetAssertions::default()
        },
    )
    .await;

    successful_get_opts(
        &key_on_instance_2,
        instance_one.clone(),
        GetAssertions {
            expected_instance_id: Some("2".into()),
            expected_load_count: Some(1),
            ..GetAssertions::default()
        },
    )
    .await;

    Ok(())
}

// ---------------------------------------------------------------------------
// groupcache_inner.rs: add_peer on already-added peer (early return)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_add_peer_twice_is_idempotent() -> Result<()> {
    let (instance_one, instance_two) = two_instances().await?;

    instance_one.add_peer(instance_two.addr().into()).await?;
    instance_one.add_peer(instance_two.addr().into()).await?;

    let key_on_instance_2 = key_owned_by_instance(instance_two.clone());
    successful_get(&key_on_instance_2, Some("2"), instance_one.clone()).await;

    Ok(())
}

// ---------------------------------------------------------------------------
// groupcache_inner.rs: remove_peer on non-existent peer (early return)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_remove_peer_not_present_is_noop() -> Result<()> {
    let instance = spawn_groupcache("rm-noop").await?;

    let fake_peer = GroupcachePeer::from_socket("127.0.0.1:19999".parse()?);
    instance.remove_peer(fake_peer).await?;

    Ok(())
}

// ---------------------------------------------------------------------------
// groupcache_inner.rs: set_peers with new successful connections
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_set_peers_adds_new_peers_via_update_routing_table() -> Result<()> {
    let peers = spawn_instances(3).await?;
    let (me, others) = peers.split_first().unwrap();

    let all_peers: HashSet<GroupcachePeer> = others.iter().map(|e| e.addr().into()).collect();
    me.set_peers(all_peers).await?;

    for (i, peer) in others.iter().enumerate() {
        let key = key_owned_by_instance(peer.clone());
        let peer_index = &*(i + 1).to_string();
        successful_get(&key, Some(peer_index), me.clone()).await;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// invalidation.rs: watcher reconnect after peer restart
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_invalidation_watcher_flushes_hot_cache_on_peer_disconnect() -> Result<()> {
    let (shutdown_signal, shutdown_recv) = tokio::sync::oneshot::channel::<()>();
    let (shutdown_done_s, shutdown_done_r) = tokio::sync::oneshot::channel::<()>();

    async fn shutdown_proxy(
        shutdown_signal: tokio::sync::oneshot::Receiver<()>,
        shutdown_done: tokio::sync::oneshot::Sender<()>,
    ) {
        shutdown_signal.await.unwrap();
        shutdown_done.send(()).unwrap();
    }

    let instance_one = spawn_groupcache("disc-1").await?;
    let instance_two = spawn_groupcache_instance(
        "disc-2",
        OS_ALLOCATED_PORT_ADDR,
        shutdown_proxy(shutdown_recv, shutdown_done_s),
    )
    .await?;

    instance_one.add_peer(instance_two.addr().into()).await?;
    instance_two.add_peer(instance_one.addr().into()).await?;

    time::sleep(Duration::from_millis(100)).await;

    let key_on_two = key_owned_by_instance(instance_two.clone());
    successful_get_opts(
        &key_on_two,
        instance_one.clone(),
        GetAssertions {
            expected_instance_id: Some("disc-2".into()),
            expected_load_count: Some(1),
            ..GetAssertions::default()
        },
    )
    .await;

    shutdown_signal.send(()).unwrap();
    shutdown_done_r.await.unwrap();

    time::sleep(Duration::from_millis(1000)).await;

    let result = instance_one.get(&key_on_two).await;
    match result {
        Ok(v) => {
            assert!(
                v.contains("disc-1"),
                "expected local fallback after disconnect, got: '{}'",
                v
            );
        }
        Err(e) => {
            assert!(
                e.to_string().contains("Transport") || e.to_string().contains("Loading error"),
                "unexpected error: '{}'",
                e
            );
        }
    }

    time::sleep(Duration::from_millis(500)).await;

    Ok(())
}

// ---------------------------------------------------------------------------
// set_peers with no changes returns early
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_set_peers_with_same_peers_is_noop() -> Result<()> {
    let (instance_one, instance_two) = two_connected_instances().await?;

    let peers: HashSet<GroupcachePeer> = vec![instance_two.addr().into()].into_iter().collect();

    instance_one.set_peers(peers).await?;

    let key_on_instance_2 = key_owned_by_instance(instance_two.clone());
    successful_get(&key_on_instance_2, Some("2"), instance_one.clone()).await;

    Ok(())
}

// ---------------------------------------------------------------------------
// groupcache_inner.rs: evict_main_cache_keys_not_owned after peer joins
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_main_cache_evicts_keys_that_move_to_new_peer() -> Result<()> {
    let instance_one = spawn_groupcache("evict-1").await?;
    let instance_two = spawn_groupcache("evict-2").await?;

    for i in 0..50 {
        let key = format!("K-evict-safe-{}", i);
        let _ = instance_one.get(&key).await?;
    }

    instance_one.add_peer(instance_two.addr().into()).await?;

    let key_on_two = key_owned_by_instance(instance_two.clone());
    successful_get(&key_on_two, Some("evict-2"), instance_one.clone()).await;

    Ok(())
}

#[tokio::test]
async fn test_readding_peer_via_set_peers_replaces_invalidation_watcher() -> Result<()> {
    let peers = spawn_instances(3).await?;
    let (me, others) = peers.split_first().unwrap();

    let all_peers: HashSet<GroupcachePeer> = others.iter().map(|e| e.addr().into()).collect();
    me.set_peers(all_peers.clone()).await?;

    let subset: HashSet<GroupcachePeer> = vec![others[0].addr().into()].into_iter().collect();
    me.set_peers(subset).await?;
    me.set_peers(all_peers).await?;

    time::sleep(Duration::from_millis(100)).await;

    for (i, peer) in others.iter().enumerate() {
        let key = key_owned_by_instance(peer.clone());
        let peer_index = &*(i + 1).to_string();
        successful_get(&key, Some(peer_index), me.clone()).await;
    }

    Ok(())
}

#[tokio::test]
async fn test_add_peer_is_idempotent() -> Result<()> {
    let (instance_one, instance_two) = two_connected_instances().await?;

    // Adding the same peer again should be a no-op (not an error).
    instance_one
        .add_peer(GroupcachePeer::from_socket(instance_two.addr()))
        .await?;

    // Routing should still work correctly.
    let key = key_owned_by_instance(instance_two.clone());
    successful_get(&key, Some("2"), instance_one.clone()).await;

    Ok(())
}

#[tokio::test]
async fn test_set_peers_connects_to_genuinely_new_peers() -> Result<()> {
    // Create instances WITHOUT connecting them through spawn_instances.
    let instance_one = spawn_groupcache("1").await?;
    let instance_two = spawn_groupcache("2").await?;
    let instance_three = spawn_groupcache("3").await?;

    // Use set_peers to introduce both new peers at once.
    let peers: HashSet<GroupcachePeer> = [instance_two.addr(), instance_three.addr()]
        .into_iter()
        .map(GroupcachePeer::from_socket)
        .collect();

    instance_one.set_peers(peers).await?;

    // Verify routing works through the newly connected peers.
    let key_on_two = key_owned_by_instance(instance_two.clone());
    successful_get(&key_on_two, Some("2"), instance_one.clone()).await;

    let key_on_three = key_owned_by_instance(instance_three.clone());
    successful_get(&key_on_three, Some("3"), instance_one.clone()).await;

    Ok(())
}

#[tokio::test]
async fn test_remove_from_disconnected_peer_returns_transport_error() -> Result<()> {
    let (instance_one, instance_two) = two_instances_with_one_disconnected().await?;

    // Find a key owned by the disconnected instance_two.
    let key = key_owned_by_instance(instance_two.clone());

    // The graceful shutdown keeps existing HTTP/2 connections alive briefly.
    // Retry remove until the connection actually drops.
    let mut last_err = None;
    for _ in 0..20 {
        match instance_one.remove(&key).await {
            Err(e) => {
                last_err = Some(e);
                break;
            }
            Ok(()) => {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        }
    }

    let err = last_err.expect("expected transport error for remove on disconnected peer");
    let err_string = err.to_string();
    assert!(
        err_string.contains("Transport"),
        "expected Transport error, got: '{}'",
        err_string
    );

    Ok(())
}

#[tokio::test]
async fn test_remove_peer_that_is_not_known_is_noop() -> Result<()> {
    let instance = single_instance().await?;
    let unknown_addr: SocketAddr = "127.0.0.1:19999".parse()?;

    // Removing a peer that was never added should succeed silently.
    instance
        .remove_peer(GroupcachePeer::from_socket(unknown_addr))
        .await?;

    Ok(())
}
