use crate::groupcache::{GroupcachePeer, GroupcachePeerClient};
use anyhow::{Context, Result};
use hashring::HashRing;
use std::collections::HashMap;
use std::net::SocketAddr;

static VNODES_PER_PEER: i32 = 40;

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

    pub(crate) fn lookup_peer(&self, key: &str) -> Result<GroupcachePeerWithClient> {
        let peer = self.peer_for_key(key)?;
        let client = self.connected_client(&peer);

        Ok(GroupcachePeerWithClient { peer, client })
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

    pub(crate) fn set_peers(&mut self, peers: Vec<GroupcachePeer>) {
        for peer in self.peers {

        }
    }

    pub(crate) fn contains_peer(&self, peer: &GroupcachePeer) -> bool {
        self.peers.contains_key(peer)
    }
}

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct VNode {
    addr_id: String,
}

impl VNode {
    fn new(addr: SocketAddr, id: usize) -> Self {
        Self {
            addr_id: format!("{}_{}", addr, id),
        }
    }

    fn vnodes_for_peer(peer: GroupcachePeer, num: i32) -> Vec<VNode> {
        let mut vnodes = Vec::new();
        for i in 0..num {
            let vnode = VNode::new(peer.socket, i as usize);

            vnodes.push(vnode);
        }
        vnodes
    }

    fn as_peer(&self) -> GroupcachePeer {
        let addr = self.addr_id.split('_').next().unwrap().parse().unwrap();
        GroupcachePeer { socket: addr }
    }
}
