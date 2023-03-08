# groupcache

This is intended to be a port of a popular caching library from Go [groupcache](https://github.com/golang/groupcache).


## Dev notes

- perf test:
    1. Boostrap a couple of nodes,
    2. Trigger a series of concurrent requests to the nodes
    3. Measure the time it takes to complete the requests - tp50, tp99 etc

