use crate::groupcache::{GroupcachePeer, GroupcachePeerClient};
use anyhow::{Context, Result};
use hashring::HashRing;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::net::SocketAddr;

static VNODES_PER_PEER: u32 = 40;

#[derive(Clone)]
pub(crate) struct RoutingState {
    peers: HashMap<GroupcachePeer, GroupcachePeerClient>,
    ring: HashRing<VNode>,
}

pub(crate) struct GroupcachePeerWithClient {
    pub(crate) peer: GroupcachePeer,
    pub(crate) client: Option<GroupcachePeerClient>,
}

impl RoutingState {
    pub(crate) fn with_local_peer(peer: GroupcachePeer) -> Self {
        let ring = {
            let mut ring = HashRing::new();
            let vnodes = VNode::vnodes_for_peer(peer, VNODES_PER_PEER);
            for vnode in vnodes {
                ring.add(vnode)
            }

            ring
        };

        RoutingState {
            peers: HashMap::new(),
            ring,
        }
    }

    pub(crate) fn peers(&self) -> HashSet<GroupcachePeer> {
        self.peers.keys().copied().collect()
    }

    pub(crate) fn lookup_peer(&self, key: &str) -> Result<GroupcachePeerWithClient> {
        let peer = self.peer_for_key(key)?;
        let client = self.connected_client(&peer);

        Ok(GroupcachePeerWithClient { peer, client })
    }

    /// Returns the peer that owns the given key, or None if the ring is empty.
    pub(crate) fn owner_of(&self, key: &str) -> Option<GroupcachePeer> {
        self.ring.get(&key).map(|vnode| vnode.as_peer())
    }

    fn peer_for_key(&self, key: &str) -> Result<GroupcachePeer> {
        let vnode = self
            .ring
            .get(&key)
            .context("unreachable: ring can't be empty")?;
        Ok(vnode.as_peer())
    }

    fn connected_client(&self, peer: &GroupcachePeer) -> Option<GroupcachePeerClient> {
        self.peers.get(peer).cloned()
    }

    pub(crate) fn add_peer(&mut self, peer: GroupcachePeer, client: GroupcachePeerClient) {
        let vnodes = VNode::vnodes_for_peer(peer, VNODES_PER_PEER);
        for vnode in vnodes {
            self.ring.add(vnode);
        }
        self.peers.insert(peer, client);
    }

    pub(crate) fn remove_peer(&mut self, peer: GroupcachePeer) {
        let vnodes = VNode::vnodes_for_peer(peer, VNODES_PER_PEER);
        for vnode in vnodes {
            self.ring.remove(&vnode);
        }
        self.peers.remove(&peer);
    }

    pub(crate) fn contains_peer(&self, peer: &GroupcachePeer) -> bool {
        self.peers.contains_key(peer)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct VNode {
    addr: SocketAddr,
    id: u32,
}

impl Hash for VNode {
    fn hash<H: Hasher>(&self, state: &mut H) {
        // Preserve the same hash distribution as the original string-based format
        // so that upgrading doesn't reshuffle the hash ring.
        format!("{}_{}", self.addr, self.id).hash(state);
    }
}

impl VNode {
    fn new(addr: SocketAddr, id: u32) -> Self {
        Self { addr, id }
    }

    fn vnodes_for_peer(peer: GroupcachePeer, num: u32) -> Vec<VNode> {
        (0..num).map(|i| VNode::new(peer.socket, i)).collect()
    }

    fn as_peer(&self) -> GroupcachePeer {
        GroupcachePeer { socket: self.addr }
    }
}
