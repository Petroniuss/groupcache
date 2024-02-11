# groupcache
[![Crates.io](https://img.shields.io/crates/v/groupcache.svg)](https://crates.io/crates/groupcache)
[![Documentation](https://docs.rs/groupcache/badge.svg)](https://docs.rs/groupcache)
[![Codecov](https://codecov.io/gh/petroniuss/groupcache/main/graph/badge.svg)](https://codecov.io/gh/petroniuss/groupcache)
[![Dependency status](https://deps.rs/repo/github/petroniuss/groupcache/status.svg)](https://deps.rs/repo/github/petroniuss/groupcache)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)

This is a port of a popular distributed caching library from [Go groupcache](https://github.com/golang/groupcache) to Rust. 

groupcache is a distributed caching and cache-filling library, intended as a replacement for a pool of memcached nodes in many cases. It shards by key to select which peer is responsible for that key.

## Comparison with original implementation
- Implemented in Rust, thus it's blazingly fast. For real though, I haven't done any serious benchmarks yet.
- Does not implement groups (which makes me wonder whether `groupcache` name still makes sense).
  - if such functionality is needed, it can be implemented on top of this library by having different implementations of `ValueLoader` depending on key prefix.
  - In this case however all metrics would be aggregated together, and it wouldn't be possible to customize cache per each group. Possibly something to think about in future versions.
- Slightly different API for service discovery:
  - API allows to add a peer, remove a peer compared to adding all peers at once (and thus invalidating previously added peers) in original implementation.
- Still does not implement CAS, Increment/Decrement but supports cache invalidation.
  - Care must be taken to handle stale values from `hot_cache`, see documentation for `Options`.

## Comparison to memcached (from original repository)

#### Like memcached, groupcache:
shards by key to select which peer is responsible for that key. Unlike memcached, groupcache:
- does not require running a separate set of servers, thus massively reducing deployment/configuration pain. groupcache is a client library as well as a server. It connects to its own peers, forming a distributed cache.

- comes with a cache filling mechanism. Whereas memcached just says "Sorry, cache miss", often resulting in a thundering herd of database (or whatever) loads from an unbounded number of clients (which has resulted in several fun outages), groupcache coordinates cache fills such that only one load in one process of an entire replicated set of processes populates the cache, then multiplexes the loaded value to all callers.

- does not support versioned values. If key `"foo"` is value `"bar"`, key `"foo"` must always be `"bar"`. There are neither cache expiration times, nor explicit cache evictions. Thus, there is also no CAS, nor Increment/Decrement. This also means that groupcache....

- ... supports automatic mirroring of super-hot items to multiple processes. This prevents memcached hot spotting where a machine's CPU and/or NIC are overloaded by very popular keys/values.

- is currently only available for Go. It's very unlikely that I (bradfitz@) will port the code to any other language.

#### Loading process
In a nutshell, a groupcache lookup of `Get("foo")` looks like:

(On machine #5 of a set of N machines running the same code)

- Is the value of `"foo"` in local memory because it's super hot? If so, use it.

- Is the value of `"foo"` in local memory because peer `#5` (the current peer) is the owner of it? If so, use it.

- Amongst all the peers in my set of N, am I the owner of the key `"foo"`? (e.g. does it consistent hash to 5?) If so, load it. If other callers come in, via the same process or via RPC requests from peers, they block waiting for the load to finish and get the same answer. If not, RPC to the peer that's the owner and get the answer. If the RPC fails, just load it locally (still with local dup suppression).

## Examples
 - There is one example showing how to run groupcache alongside a simple axum server deployed on k8s, see [examples/kubernetes-service-discovery](https://github.com/Petroniuss/groupcache/tree/main/examples/kubernetes-service-discovery).

## Documentation
See <https://docs.rs/groupcache> and <https://docs.rs/groupcache/latest/groupcache/struct.Groupcache.html>

