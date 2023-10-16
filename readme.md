# groupcache

This is intended to be a port of a popular caching library from Go [groupcache](https://github.com/golang/groupcache).

## examples
See [groupcache-app/src/main.rs](groupcache-app/src/main.rs).

To run basic example with cluster of three nodes:
```bash
test.sh
```

## todos
- [x] Groupcache implementation with consistent hashing.
- [x] Expose groupcache as axum router that can be nested in other routers.
- [x] Integration tests.
- [x] Implement hot cache - caching items that are owned by different peers but are frequently loaded:
  - Not sure how I want to approach this, if I intend to implement API to removing value from groupcache.
- [x] Implement API to remove items from groupcache.
- [ ] Expose timeouts as a configuration option.
- [x] Expose metrics from the library:
  - [ ] Struct with a bunch of atomic ints -> solved by metrics crate
  - [ ] Implement Prometheus Exporter crate for metrics. -> solved by metrics_exporter_prometheus crate
- [ ] Usability:
    - [ ] Create basic example showing how to run groupcache alongside a simple axum server.
    - [ ] Prepare Documentation.
    - [ ] Example with service discovery with consul in k8s.
- ...
- [ ] Publish to crates.io
