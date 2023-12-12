mod common;

use anyhow::Result;
use common::*;
use groupcache::{Groupcache, GroupcachePeer};
use pretty_assertions::assert_eq;

use std::net::SocketAddr;
use tokio::time;

#[tokio::test]
async fn test_when_there_is_only_one_peer_it_should_handle_entire_key_space() -> Result<()> {
    let groupcache = {
        let loader = TestCacheLoader::new("1");
        let addr: SocketAddr = "127.0.0.1:8080".parse()?;
        Groupcache::<CachedValue>::builder(addr.into(), loader).build()
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
        },
    )
    .await;

    successful_get_opts(
        &key_on_instance_2,
        instance_two.clone(),
        GetAssertions {
            expected_instance_id: Some("2".into()),
            expected_load_count: Some(1),
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
        },
    )
    .await;

    instance_two.remove(&key_on_instance_2).await?;

    successful_get_opts(
        &key_on_instance_2,
        instance_one.clone(),
        GetAssertions {
            expected_instance_id: Some("2".into()),
            expected_load_count: Some(1),
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
        },
    )
    .await;

    Ok(())
}
