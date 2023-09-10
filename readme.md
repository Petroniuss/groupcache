# groupcache

This is intended to be a port of a popular caching library from Go [groupcache](https://github.com/golang/groupcache).

## todos
- [x] Groupcache implementation with consistent hashing.
- [ ] Expose timeouts as a configuration option.
- [ ] Support base http path.
- [ ] Expose groupcache as axum router that can be nested in other routers.
- [ ] Integration tests:
  - [ ] Test hitting key that's located on a different peer.
  - [ ] Test hitting key that's located on the same peer.
  - [ ] Test hitting key when loading fails.
  - [ ] Test what happens when peer is down and then is reconnected.
- [ ] Expose metrics from the library:
  - [ ] Struct with a bunch of atomic ints.
  - [ ] Implement Prometheus Exporter crate for metrics.
- [ ] Usability:
    - [ ] Create basic example showing how to run groupcache alongside a simple axum server.
    - [ ] Prepare Documentation.
- [ ] Implement Benchmark
- [ ] Implement hot cache - caching items that are owned by different peers but are frequently loaded:
  - Not sure how I want to approach this, if I intend to implement API to removing value from groupcache.
- ...
- [ ] Publish to crates.io
