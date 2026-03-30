use crate::node::GroupcacheNode;
use crate::protocol::*;
use log::warn;
use maelstrom::protocol::{ErrorMessageBody, Message};
use maelstrom::{done, Result, Runtime};
use std::sync::Arc;

/// Top-level message dispatcher. Routes incoming messages to the appropriate handler
/// based on message type.
pub async fn handle_message(
    node: &Arc<GroupcacheNode>,
    runtime: &Runtime,
    req: &Message,
) -> Result<()> {
    match req.get_type() {
        "load" => handle_load(node, runtime, req).await,
        "read" => handle_read(node, runtime, req).await,
        "invalidate" => handle_invalidate(node, runtime, req).await,
        "gc_get" => handle_gc_get(node, runtime, req).await,
        "gc_load" => handle_gc_load(node, runtime, req).await,
        "gc_remove" => handle_gc_remove(node, runtime, req).await,
        "gc_invalidation_event" => handle_gc_invalidation_event(node, req).await,
        _ => done(runtime.clone(), req.clone()),
    }
}

/// Client operation: load a value into the cache for a given key.
///
/// - If this node is the owner: generate a new value, store in main_cache.
/// - If not owner: RPC `gc_load` to the owner, cache the result in hot_cache.
/// - Reply with `load_ok` + value.
async fn handle_load(
    node: &Arc<GroupcacheNode>,
    runtime: &Runtime,
    req: &Message,
) -> Result<()> {
    let load_req: LoadRequest = req.body.as_obj()?;
    let key = load_req.key;

    let value = if node.is_owner(&key) {
        // We are the owner: generate and store locally.
        let val = node.next_value(&key).await;
        node.main_cache.insert(key.clone(), val.clone()).await;
        val
    } else {
        // Forward to owner via gc_load RPC.
        let owner = node.owner_of(&key).to_string();
        let gc_load = GcLoad::new(key.clone());
        let rpc_result = runtime.rpc(owner, gc_load).await?;
        let resp_msg: Message = rpc_result.await?;
        let gc_load_ok: GcLoadOk = resp_msg.body.as_obj()?;
        // Cache in hot_cache for future reads.
        node.hot_cache
            .insert(key.clone(), gc_load_ok.value.clone())
            .await;
        gc_load_ok.value
    };

    runtime.reply(req.clone(), LoadResponse::new(value)).await
}

/// Client operation: read a cached value for a given key.
///
/// - Check local caches (main_cache then hot_cache).
/// - If hit: reply with value.
/// - If miss and we are the owner: reply with error code 20 (key not found).
/// - If miss and remote owner: RPC `gc_get` to owner, cache in hot_cache, reply.
/// - If RPC fails: reply with error code 11 (temporarily unavailable).
async fn handle_read(
    node: &Arc<GroupcacheNode>,
    runtime: &Runtime,
    req: &Message,
) -> Result<()> {
    let read_req: ReadRequest = req.body.as_obj()?;
    let key = read_req.key;

    // Check local caches first.
    if let Some(value) = node.local_lookup(&key).await {
        return runtime
            .reply(req.clone(), ReadResponse::new(value))
            .await;
    }

    if node.is_owner(&key) {
        // We are the owner but don't have the key -- not found.
        return runtime
            .reply(
                req.clone(),
                ErrorMessageBody::new(20, "key not found"),
            )
            .await;
    }

    // Forward to owner via gc_get RPC.
    let owner = node.owner_of(&key).to_string();
    let gc_get = GcGet::new(key.clone());
    let rpc_result = runtime.rpc(owner, gc_get).await;

    match rpc_result {
        Ok(call) => match call.await {
            Ok(resp_msg) => {
                let gc_get_ok: GcGetOk = resp_msg.body.as_obj()?;
                match gc_get_ok.value {
                    Some(value) => {
                        node.hot_cache.insert(key.clone(), value.clone()).await;
                        runtime
                            .reply(req.clone(), ReadResponse::new(value))
                            .await
                    }
                    None => {
                        runtime
                            .reply(
                                req.clone(),
                                ErrorMessageBody::new(20, "key not found"),
                            )
                            .await
                    }
                }
            }
            Err(e) => {
                warn!("gc_get RPC failed: {}", e);
                runtime
                    .reply(
                        req.clone(),
                        ErrorMessageBody::new(11, "temporarily unavailable"),
                    )
                    .await
            }
        },
        Err(e) => {
            warn!("gc_get RPC send failed: {}", e);
            runtime
                .reply(
                    req.clone(),
                    ErrorMessageBody::new(11, "temporarily unavailable"),
                )
                .await
        }
    }
}

/// Client operation: invalidate a cached entry for a given key.
///
/// - Clear own hot_cache for the key.
/// - If owner: clear main_cache, broadcast `gc_invalidation_event` to all peers.
/// - If not owner: RPC `gc_remove` to the owner.
/// - Reply with `invalidate_ok`.
async fn handle_invalidate(
    node: &Arc<GroupcacheNode>,
    runtime: &Runtime,
    req: &Message,
) -> Result<()> {
    let inv_req: InvalidateRequest = req.body.as_obj()?;
    let key = inv_req.key;

    // Always clear our own hot_cache.
    node.hot_cache.remove(&key).await;

    if node.is_owner(&key) {
        // Clear main_cache and broadcast invalidation to peers.
        node.main_cache.remove(&key).await;
        broadcast_invalidation(node, runtime, &key);
    } else {
        // Forward to owner via gc_remove RPC.
        let owner = node.owner_of(&key).to_string();
        let gc_remove = GcRemove::new(key.clone());
        let rpc_result = runtime.rpc(owner, gc_remove).await?;
        let _resp: Message = rpc_result.await?;
    }

    runtime
        .reply(req.clone(), InvalidateResponse::new())
        .await
}

/// Inter-node: peer requests a value from our main_cache.
/// We are the owner for this key.
async fn handle_gc_get(
    node: &Arc<GroupcacheNode>,
    runtime: &Runtime,
    req: &Message,
) -> Result<()> {
    let gc_get: GcGet = req.body.as_obj()?;
    let key = gc_get.key;
    let value = node.main_cache.get(&key).await;
    runtime
        .reply(req.clone(), GcGetOk::new(key, value))
        .await
}

/// Inter-node: peer asks us (the owner) to load/generate a value.
async fn handle_gc_load(
    node: &Arc<GroupcacheNode>,
    runtime: &Runtime,
    req: &Message,
) -> Result<()> {
    let gc_load: GcLoad = req.body.as_obj()?;
    let key = gc_load.key;
    let value = node.next_value(&key).await;
    node.main_cache.insert(key.clone(), value.clone()).await;
    runtime
        .reply(req.clone(), GcLoadOk::new(key, value))
        .await
}

/// Inter-node: peer asks us (the owner) to remove a key.
/// Clear main_cache and broadcast invalidation to all peers.
async fn handle_gc_remove(
    node: &Arc<GroupcacheNode>,
    runtime: &Runtime,
    req: &Message,
) -> Result<()> {
    let gc_remove: GcRemove = req.body.as_obj()?;
    let key = gc_remove.key;
    node.main_cache.remove(&key).await;
    broadcast_invalidation(node, runtime, &key);
    runtime
        .reply(req.clone(), GcRemoveOk::new(key))
        .await
}

/// Inter-node fire-and-forget: a peer tells us that a key has been invalidated.
/// Clear our hot_cache for that key. No reply needed.
async fn handle_gc_invalidation_event(
    node: &Arc<GroupcacheNode>,
    req: &Message,
) -> Result<()> {
    let event: GcInvalidationEvent = req.body.as_obj()?;
    node.hot_cache.remove(&event.key).await;
    Ok(())
}

/// Broadcast a `gc_invalidation_event` to all peers (fire-and-forget).
fn broadcast_invalidation(
    _node: &Arc<GroupcacheNode>,
    runtime: &Runtime,
    key: &str,
) {
    for peer in runtime.neighbours() {
        let event = GcInvalidationEvent::new(key.to_string());
        if let Err(e) = runtime.send_async(peer.as_str(), event) {
            warn!("Failed to send gc_invalidation_event to {}: {}", peer, e);
        }
    }
}
