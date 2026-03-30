//! Benchmarks comparing routing state implementations under contention.
//!
//! Three approaches:
//!   RwLock:        readers acquire read lock, writers acquire write lock
//!   ArcSwap+clone: readers do atomic load, writers clone-mutate-store
//!   Double-buffer: readers do atomic load, writers mutate standby then swap
//!
//! All use the same Vec-based HashRing (best cache locality for reads).

use arc_swap::ArcSwap;
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use hashring::HashRing;
use parking_lot::{Mutex, RwLock};
use std::sync::Arc;

const VNODES_PER_PEER: usize = 40;

// ── Shared ring type ────────────────────────────────────────────────

#[derive(Clone)]
struct Ring {
    ring: HashRing<String>,
}

impl Ring {
    fn new(num_peers: usize) -> Self {
        let mut ring = HashRing::new();
        for peer_id in 0..num_peers {
            for vnode_id in 0..VNODES_PER_PEER {
                ring.add(format!("10.0.0.{}:8080_{}", peer_id, vnode_id));
            }
        }
        Self { ring }
    }

    fn lookup(&self, key: &str) -> &str {
        self.ring.get(&key).expect("ring not empty")
    }

    fn add_vnode(&mut self, vnode: String) {
        self.ring.add(vnode);
    }

}

// ── Double-buffer wrapper (mirrors production DoubleBufferedRouting) ─

struct DoubleBuf {
    active: ArcSwap<Ring>,
    standby: Mutex<Ring>,
}

impl DoubleBuf {
    fn new(ring: Ring) -> Self {
        Self {
            active: ArcSwap::from_pointee(ring.clone()),
            standby: Mutex::new(ring),
        }
    }

    fn load(&self) -> arc_swap::Guard<Arc<Ring>> {
        self.active.load()
    }

    fn mutate(&self, vnode: &str) {
        let mut standby = self.standby.lock();
        standby.add_vnode(vnode.to_string());

        let old_active = self.active.swap(Arc::new(standby.clone()));
        let mut caught_up = Arc::try_unwrap(old_active).unwrap_or_else(|arc| (*arc).clone());
        caught_up.add_vnode(vnode.to_string());
        *standby = caught_up;
    }
}

// ── Benchmarks ──────────────────────────────────────────────────────

fn bench_read_scaling(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(16)
        .build()
        .unwrap();

    let ring = Ring::new(10);
    let keys: Vec<String> = (0..1000).map(|i| format!("cache-key-{}", i)).collect();

    let mut group = c.benchmark_group("routing_lookup");
    group.sample_size(50);

    for num_readers in [1, 2, 4, 8, 16] {
        // RwLock
        let state = Arc::new(RwLock::new(ring.clone()));
        group.bench_with_input(BenchmarkId::new("rwlock", num_readers), &num_readers, |b, &n| {
            b.to_async(&rt).iter(|| {
                let state = state.clone();
                let keys = keys.clone();
                async move {
                    let mut handles = Vec::with_capacity(n);
                    for rid in 0..n {
                        let s = state.clone();
                        let k = keys.clone();
                        handles.push(tokio::spawn(async move {
                            for i in 0..1000 {
                                let key = &k[(rid * 100 + i) % k.len()];
                                std::hint::black_box(s.read().lookup(key));
                            }
                        }));
                    }
                    for h in handles { h.await.unwrap(); }
                }
            });
        });

        // ArcSwap (read path same as double-buffer)
        let state = Arc::new(ArcSwap::from_pointee(ring.clone()));
        group.bench_with_input(BenchmarkId::new("arcswap", num_readers), &num_readers, |b, &n| {
            b.to_async(&rt).iter(|| {
                let state = state.clone();
                let keys = keys.clone();
                async move {
                    let mut handles = Vec::with_capacity(n);
                    for rid in 0..n {
                        let s = state.clone();
                        let k = keys.clone();
                        handles.push(tokio::spawn(async move {
                            for i in 0..1000 {
                                let key = &k[(rid * 100 + i) % k.len()];
                                std::hint::black_box(s.load().lookup(key));
                            }
                        }));
                    }
                    for h in handles { h.await.unwrap(); }
                }
            });
        });
    }
    group.finish();
}

fn bench_mixed_matrix(c: &mut Criterion) {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(20)
        .build()
        .unwrap();

    let ring = Ring::new(10);
    let keys: Vec<String> = (0..1000).map(|i| format!("cache-key-{}", i)).collect();

    let mut group = c.benchmark_group("routing_mixed");
    group.sample_size(30);

    for num_readers in [1, 4, 8, 16] {
        for num_writers in [0, 1, 4] {
            let label = format!("{}r_{}w", num_readers, num_writers);

            // ── RwLock ──
            let state = Arc::new(RwLock::new(ring.clone()));
            group.bench_function(format!("rwlock/{}", label), |b| {
                b.to_async(&rt).iter(|| {
                    let state = state.clone();
                    let keys = keys.clone();
                    async move {
                        let mut handles = Vec::with_capacity(num_readers + num_writers);
                        for rid in 0..num_readers {
                            let s = state.clone();
                            let k = keys.clone();
                            handles.push(tokio::spawn(async move {
                                for i in 0..1000 {
                                    let key = &k[(rid * 100 + i) % k.len()];
                                    std::hint::black_box(s.read().lookup(key));
                                }
                            }));
                        }
                        for wid in 0..num_writers {
                            let s = state.clone();
                            handles.push(tokio::spawn(async move {
                                for i in 0..10 {
                                    {
                                        let vnode = format!("10.0.0.{}:8080_{}", 50 + wid, i);
                                        s.write().add_vnode(vnode);
                                    }
                                    tokio::task::yield_now().await;
                                }
                            }));
                        }
                        for h in handles { h.await.unwrap(); }
                    }
                });
            });

            // ── ArcSwap + clone ──
            let state = Arc::new(ArcSwap::from_pointee(ring.clone()));
            group.bench_function(format!("arcswap_clone/{}", label), |b| {
                b.to_async(&rt).iter(|| {
                    let state = state.clone();
                    let keys = keys.clone();
                    async move {
                        let mut handles = Vec::with_capacity(num_readers + num_writers);
                        for rid in 0..num_readers {
                            let s = state.clone();
                            let k = keys.clone();
                            handles.push(tokio::spawn(async move {
                                for i in 0..1000 {
                                    let key = &k[(rid * 100 + i) % k.len()];
                                    std::hint::black_box(s.load().lookup(key));
                                }
                            }));
                        }
                        for wid in 0..num_writers {
                            let s = state.clone();
                            handles.push(tokio::spawn(async move {
                                for i in 0..10 {
                                    let mut new = (**s.load()).clone();
                                    new.add_vnode(format!("10.0.0.{}:8080_{}", 50 + wid, i));
                                    s.store(Arc::new(new));
                                    tokio::task::yield_now().await;
                                }
                            }));
                        }
                        for h in handles { h.await.unwrap(); }
                    }
                });
            });

            // ── Double-buffer ──
            let state = Arc::new(DoubleBuf::new(ring.clone()));
            group.bench_function(format!("double_buf/{}", label), |b| {
                b.to_async(&rt).iter(|| {
                    let state = state.clone();
                    let keys = keys.clone();
                    async move {
                        let mut handles = Vec::with_capacity(num_readers + num_writers);
                        for rid in 0..num_readers {
                            let s = state.clone();
                            let k = keys.clone();
                            handles.push(tokio::spawn(async move {
                                for i in 0..1000 {
                                    let key = &k[(rid * 100 + i) % k.len()];
                                    std::hint::black_box(s.load().lookup(key));
                                }
                            }));
                        }
                        for wid in 0..num_writers {
                            let s = state.clone();
                            handles.push(tokio::spawn(async move {
                                for i in 0..10 {
                                    s.mutate(&format!("10.0.0.{}:8080_{}", 50 + wid, i));
                                    tokio::task::yield_now().await;
                                }
                            }));
                        }
                        for h in handles { h.await.unwrap(); }
                    }
                });
            });
        }
    }
    group.finish();
}

fn bench_clone_and_lookup_by_cluster_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("ring_by_cluster_size");

    for num_peers in [5, 10, 50, 128, 256] {
        let ring = Ring::new(num_peers);
        let keys: Vec<String> = (0..1000).map(|i| format!("cache-key-{}", i)).collect();

        group.bench_function(format!("clone/{}nodes", num_peers), |b| {
            b.iter(|| std::hint::black_box(ring.clone()))
        });

        group.bench_function(format!("lookup_1000/{}nodes", num_peers), |b| {
            b.iter(|| {
                for key in &keys {
                    std::hint::black_box(ring.lookup(key));
                }
            })
        });
    }
    group.finish();
}

criterion_group!(benches, bench_read_scaling, bench_mixed_matrix, bench_clone_and_lookup_by_cluster_size);
criterion_main!(benches);
