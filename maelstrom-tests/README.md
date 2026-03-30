# groupcache-maelstrom

Maelstrom-based convergence testing for groupcache's streaming invalidation protocol.

## What This Tests

After a key is invalidated, all nodes in the cluster must stop serving stale data
within a bounded time window — even under network partitions and packet loss.

## Prerequisites

- Rust toolchain (`cargo`)
- Java 11+ (`java -version`)
- Leiningen (`brew install leiningen` or see https://leiningen.org/)
- Graphviz (`brew install graphviz` — for result visualization)
- Gnuplot (`brew install gnuplot` — for latency graphs)

## Quick Start

```bash
# Run all convergence tests
make test-maelstrom

# View results in browser
make serve-results
```

## Test Scenarios

| Command | Scenario | What it tests |
|---------|----------|--------------|
| `make test-baseline` | No faults, 5 nodes, 30s | Normal convergence latency |
| `make test-partitions` | Network partitions every 10s | Recovery after partition heals |
| `make test-packet-loss` | Random packet drops | Convergence under lossy network |
| `make test-combined` | Partitions + packet loss | Worst case behavior |

## How It Works

Each test node implements groupcache's core algorithm:
- **Consistent hashing** (40 vnodes) for key ownership
- **Main cache** (owner) + **hot cache** (replicas, 30s TTL)
- **Invalidation broadcast** from owner to all peers

Communication between nodes goes through Maelstrom's simulated network,
which can inject partitions and packet loss.

### Operations

| Operation | Description |
|-----------|-------------|
| `load` | Force a key to be loaded — owner generates a versioned value |
| `read` | Read a key from any cache tier (main, hot, or fetch from owner) |
| `invalidate` | Remove a key — owner clears main cache and broadcasts to peers |

### Convergence Checker

A custom Maelstrom checker analyzes the complete operation history post-hoc:

1. For each `invalidate(key)` at time T, finds all subsequent reads that returned the stale (pre-invalidation) value
2. Computes **convergence latency** = time of last stale read - T
3. Reports latency percentiles (p50, p95, p99, max)
4. **PASS** if no stale reads persist beyond the delta-t threshold (5s default)
5. **FAIL** if any stale read occurs after the threshold

### Example Output

```
:valid?       true
:convergence  {:p50-ms 2.1, :p95-ms 12.3, :p99-ms 48.7, :max-ms 312.0, :count 1523}
:stale-reads  {:total 47, :violations 0}
:delta-t-ms   5000.0
```

## What This Proves

- The invalidation protocol converges under network faults
- Measured convergence latency under various fault conditions
- No permanent stale data after faults heal

## What This Does NOT Prove

- Performance of the real gRPC transport (covered by groupcache integration tests)
- Linearizability (not claimed — groupcache is eventually consistent)
- Byzantine fault tolerance (not claimed)

## Project Structure

```
groupcache-maelstrom/
├── Cargo.toml                    # Rust project
├── Makefile                      # All test commands
├── src/
│   ├── main.rs                   # Entry point
│   ├── lib.rs                    # Module exports
│   ├── protocol.rs               # Message type definitions
│   ├── cache.rs                  # Consistent hashing + cache with TTL
│   ├── node.rs                   # Node state management
│   └── handler.rs                # Message handlers
├── tests/
│   └── cache_test.rs             # Unit tests
├── maelstrom/
│   └── src/maelstrom/
│       ├── workload/
│       │   └── cache_convergence.clj  # Client + operation generator
│       └── checker/
│           └── convergence.clj        # Convergence latency analysis
└── scripts/
    └── patch-maelstrom.sh        # Registers workload in Maelstrom
```
