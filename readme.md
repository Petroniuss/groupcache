# groupcache

![codecov](https://codecov.io/gh/petroniuss/groupcache)

This is intended to be a port of a popular distributed caching library from Go [groupcache](https://github.com/golang/groupcache) to Rust.


## Description from original repository

groupcache is a distributed caching and cache-filling library, intended as a replacement for a pool of memcached nodes in many cases. It shards by key to select which peer is responsible for that key.

### Comparison to memcached

#### Like memcached, groupcache:
shards by key to select which peer is responsible for that key
Unlike memcached, groupcache:
- does not require running a separate set of servers, thus massively reducing deployment/configuration pain. groupcache is a client library as well as a server. It connects to its own peers, forming a distributed cache.

- comes with a cache filling mechanism. Whereas memcached just says "Sorry, cache miss", often resulting in a thundering herd of database (or whatever) loads from an unbounded number of clients (which has resulted in several fun outages), groupcache coordinates cache fills such that only one load in one process of an entire replicated set of processes populates the cache, then multiplexes the loaded value to all callers.

- does not support versioned values. If key "foo" is value "bar", key "foo" must always be "bar". There are neither cache expiration times, nor explicit cache evictions. Thus there is also no CAS, nor Increment/Decrement. This also means that groupcache....

- ... supports automatic mirroring of super-hot items to multiple processes. This prevents memcached hot spotting where a machine's CPU and/or NIC are overloaded by very popular keys/values.

- is currently only available for Go. It's very unlikely that I (bradfitz@) will port the code to any other language.

#### Loading process
In a nutshell, a groupcache lookup of Get("foo") looks like:

(On machine #5 of a set of N machines running the same code)

- Is the value of "foo" in local memory because it's super hot? If so, use it.

- Is the value of "foo" in local memory because peer #5 (the current peer) is the owner of it? If so, use it.

- Amongst all the peers in my set of N, am I the owner of the key "foo"? (e.g. does it consistent hash to 5?) If so, load it. If other callers come in, via the same process or via RPC requests from peers, they block waiting for the load to finish and get the same answer. If not, RPC to the peer that's the owner and get the answer. If the RPC fails, just load it locally (still with local dup suppression).

## Differences from original implementation
todo: rewrite this section
- it is written in Rust :D
- allows to invalidate cache entries
- does not implement groups (if such functionality is needed, it can be implemented on top of this library by using having different implementation depending on key prefix)
- slightly different API for service discovery:
  - exposes functions that allows to add a peer, remove a peer compared to adding all peers at once.
- still does not implement CAS, Increment/Decrement but supports cache invalidation
  - though care must be taken to handle stale values (from hot_cache)


## Examples
 - There is one example showing how to run groupcache alongside a simple axum server deployed on k8s, see [examples/kubernetes-service-discovery](examples/kubernetes-service-discovery).

## Documentation
..

todo: license

## Roadmap
- [x] Groupcache implementation with consistent hashing.
- [x] Expose groupcache as axum router that can be nested in other routers.
- [x] Integration tests.
- [x] Implement hot cache - caching items that are owned by different peers but are frequently loaded:
  - hot_cache needs to be configurable to allows consumers to deal with stale values.
- [x] Implement API to remove items from groupcache.
- [x] Expose metrics from the library:
  - [x] Struct with a bunch of atomic ints -> solved by metrics crate
  - [x] Implement Prometheus Exporter crate for metrics. -> solved by metrics_exporter_prometheus crate
- [x] Usability:
  - [x] Create basic example showing how to run groupcache alongside a simple axum server.
  - [x] Prepare reasonable readme.
  - [] Prepare Documentation.
- [x] Expose timeouts for remote cache loads as a configuration option.
- [x] Support https
- [] Prepare an example with HTTPS
- ...
- [ ] Publish to crates.io
