


# Design


I want to do the following:
- write a simple application (axum) using this cache
- benchmark how well it performs in a distributed setting (multiple peers, using multiple cores)


## 20.08.2023
Implement gRPC server and client. 

## Requirements

1. Support peer discovery, dynamic membership changes, and failure detection:
    - A peer can join the group at any time.
    - A peer can leave the group at any time.
    - A peer can fail at any time.
2. Provide a simple API to retrieve values associated with a key from the cache.
3. Provide API to set key-value pair.
4. Provide API to load value from cache.
5. Support cache dumps and loads for persistence?:
   - that's problematic because once a peer goes down, 
     other peers would notice its absence and take ownership of its keys.
   - How do we guarantee that the peer that comes back up will have the same keys?
     Peer could have some sort of preference for which keys it wants to own, 
     He could store that alongside the cache dump. 
     That way, when it comes back up, it can take ownership of the keys it wants.
     The way we can control that in Consistent Hashing is by having a fixed order of the keys.
     Do we want to support moves of keys between peers when a peer list changes?


Ideally we could have peer list sorted so that we can easily determine which peer owns a key.
Once a peer comes back up, it should load its cache dump and if it turns out that there are more peers or something
he can transfer some of the keys to other peers that are now owners.
This looks good to me :)
     
6. Distributed cache should be highly available and partition tolerant:
    - It should be possible to specify replication factor.



## Implementation

1. Function to set peers or add peer/remove peer.
2. Function to set key-value pair.
3. Function to get value from key.
4. Function to implement key retriever.
5. Simple RPC to retrieve values from peers.
6. cache implementation for local cache:
    - LRU cache,
    - Limit the size of the cache.
7. Function to save current state of the cache (for persistence):
   - cache dump.
8. Function to load state of the cache (for persistence):
   - cache load.
