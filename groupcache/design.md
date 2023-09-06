# Design
I want to do the following:
- write a simple application (axum) using this cache
- benchmark how well it performs in a distributed setting (multiple peers, using multiple cores)

## Requirements
1. Support peer discovery, dynamic membership changes, and failure detection:
    - A peer can join the group at any time.
    - A peer can leave the group at any time.
    - A peer can fail at any time.
2. Provide a simple API to retrieve values associated with a key from the cache.
3. Provide API to set key-value pair? This isn't implemented in groupcache. 
   As it's hard to provide consistency guarantees in a distributed setting.
4. Support cache dumps and loads for persistence?
   - that's problematic because once a peer goes down, 
     other peers would notice its absence and take ownership of its keys.
   - How do we guarantee that the peer that comes back up will have the same keys?
     Peer could have some sort of preference for which keys it wants to own, 
     He could store that alongside the cache dump. 
     That way, when it comes back up, it can take ownership of the keys it wants.
     The way we can control that in Consistent Hashing is by having a fixed order of the keys.
     Do we want to support moves of keys between peers when a peer list changes?
