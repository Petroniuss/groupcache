use crate::{Key, Peer, PeerClient};
use anyhow::{anyhow, Context, Result};
use hashring::HashRing;
use std::collections::HashMap;
use std::net::SocketAddr;

static VNODES_PER_PEER: i32 = 40;

pub(crate) struct RoutingState {
    peers: HashMap<Peer, ConnectedPeer>,
    ring: HashRing<VNode>,
}

impl RoutingState {
    pub(crate) fn with_local_peer(peer: Peer) -> Self {
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

    pub(crate) fn peer_for_key(&self, key: &Key) -> Result<Peer> {
        let vnode = self.ring.get(&key).context("ring can't be empty")?;

        Ok(vnode.as_peer())
    }

    pub(crate) fn client_for_peer(&self, peer: &Peer) -> Result<PeerClient> {
        match self.peers.get(peer) {
            Some(peer) => Ok(peer.client.clone()),
            None => Err(anyhow!("peer not found")),
        }
    }

    pub(crate) fn add_peer(&mut self, peer: Peer, client: PeerClient) {
        let vnodes = VNode::vnodes_for_peer(peer, VNODES_PER_PEER);
        for vnode in vnodes {
            self.ring.add(vnode);
        }
        self.peers.insert(peer, ConnectedPeer { client });
    }

    pub(crate) fn remove_peer(&mut self, peer: Peer) {
        let vnodes = VNode::vnodes_for_peer(peer, VNODES_PER_PEER);
        for vnode in vnodes {
            self.ring.remove(&vnode);
        }
        self.peers.remove(&peer);
    }

    pub(crate) fn contains_peer(&self, peer: &Peer) -> bool {
        self.peers.contains_key(peer)
    }
}

#[derive(Debug)]
struct ConnectedPeer {
    client: PeerClient,
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

    fn vnodes_for_peer(peer: Peer, num: i32) -> Vec<VNode> {
        let mut vnodes = Vec::new();
        for i in 0..num {
            let vnode = VNode::new(peer.socket, i as usize);

            vnodes.push(vnode);
        }
        vnodes
    }

    fn as_peer(&self) -> Peer {
        let addr = self.addr_id.split('_').next().unwrap().parse().unwrap();
        Peer { socket: addr }
    }
}
