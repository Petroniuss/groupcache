# groupcache

This is intended to be a port of a popular caching library from Go [groupcache](https://github.com/golang/groupcache).

## todos
- [x] Groupcache implementation with consistent hashing.
- [ ] Expose timeouts as a configuration option.
- [ ] Expose 
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
