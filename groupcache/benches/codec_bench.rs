use criterion::{black_box, criterion_group, criterion_main, Criterion};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, PartialEq)]
struct SmallValue {
    id: String,
    data: String,
}

#[derive(Clone, Serialize, Deserialize, PartialEq)]
struct LargeValue {
    id: String,
    name: String,
    description: String,
    tags: Vec<String>,
    metadata: Vec<(String, String)>,
}

fn small_value() -> SmallValue {
    SmallValue {
        id: "user-12345".into(),
        data: "cached-payload".into(),
    }
}

fn large_value() -> LargeValue {
    LargeValue {
        id: "entity-99999".into(),
        name: "Example Entity with a Reasonably Long Name".into(),
        description: "A description that is representative of typical cached \
            values in a distributed system, containing enough data to show \
            serialization overhead differences between formats."
            .into(),
        tags: vec![
            "distributed".into(),
            "cache".into(),
            "performance".into(),
            "benchmark".into(),
        ],
        metadata: vec![
            ("region".into(), "us-east-1".into()),
            ("version".into(), "3.2.1".into()),
            ("owner".into(), "team-platform".into()),
        ],
    }
}

fn bench_serialize(c: &mut Criterion) {
    let small = small_value();
    let large = large_value();

    let mut group = c.benchmark_group("serialize");

    group.bench_function("msgpack/small", |b| {
        b.iter(|| rmp_serde::to_vec(black_box(&small)).unwrap())
    });

    group.bench_function("bincode/small", |b| {
        b.iter(|| bincode::serialize(black_box(&small)).unwrap())
    });

    group.bench_function("msgpack/large", |b| {
        b.iter(|| rmp_serde::to_vec(black_box(&large)).unwrap())
    });

    group.bench_function("bincode/large", |b| {
        b.iter(|| bincode::serialize(black_box(&large)).unwrap())
    });

    group.finish();
}

fn bench_deserialize(c: &mut Criterion) {
    let small = small_value();
    let large = large_value();

    let small_mp = rmp_serde::to_vec(&small).unwrap();
    let small_bc = bincode::serialize(&small).unwrap();
    let large_mp = rmp_serde::to_vec(&large).unwrap();
    let large_bc = bincode::serialize(&large).unwrap();

    let mut group = c.benchmark_group("deserialize");

    group.bench_function("msgpack/small", |b| {
        b.iter(|| rmp_serde::from_slice::<SmallValue>(black_box(&small_mp)).unwrap())
    });

    group.bench_function("bincode/small", |b| {
        b.iter(|| bincode::deserialize::<SmallValue>(black_box(&small_bc)).unwrap())
    });

    group.bench_function("msgpack/large", |b| {
        b.iter(|| rmp_serde::from_slice::<LargeValue>(black_box(&large_mp)).unwrap())
    });

    group.bench_function("bincode/large", |b| {
        b.iter(|| bincode::deserialize::<LargeValue>(black_box(&large_bc)).unwrap())
    });

    group.finish();
}

fn bench_roundtrip(c: &mut Criterion) {
    let small = small_value();
    let large = large_value();

    let mut group = c.benchmark_group("roundtrip");

    group.bench_function("msgpack/small", |b| {
        b.iter(|| {
            let bytes = rmp_serde::to_vec(black_box(&small)).unwrap();
            let _: SmallValue = rmp_serde::from_slice(&bytes).unwrap();
        })
    });

    group.bench_function("bincode/small", |b| {
        b.iter(|| {
            let bytes = bincode::serialize(black_box(&small)).unwrap();
            let _: SmallValue = bincode::deserialize(&bytes).unwrap();
        })
    });

    group.bench_function("msgpack/large", |b| {
        b.iter(|| {
            let bytes = rmp_serde::to_vec(black_box(&large)).unwrap();
            let _: LargeValue = rmp_serde::from_slice(&bytes).unwrap();
        })
    });

    group.bench_function("bincode/large", |b| {
        b.iter(|| {
            let bytes = bincode::serialize(black_box(&large)).unwrap();
            let _: LargeValue = bincode::deserialize(&bytes).unwrap();
        })
    });

    group.finish();
}

fn bench_wire_size(c: &mut Criterion) {
    let small = small_value();
    let large = large_value();

    let mut group = c.benchmark_group("wire_size");

    // Not really a benchmark but a convenient way to report sizes
    group.bench_function("msgpack/small", |b| {
        let bytes = rmp_serde::to_vec(&small).unwrap();
        eprintln!("  msgpack small: {} bytes", bytes.len());
        b.iter(|| bytes.len())
    });

    group.bench_function("bincode/small", |b| {
        let bytes = bincode::serialize(&small).unwrap();
        eprintln!("  bincode small: {} bytes", bytes.len());
        b.iter(|| bytes.len())
    });

    group.bench_function("msgpack/large", |b| {
        let bytes = rmp_serde::to_vec(&large).unwrap();
        eprintln!("  msgpack large: {} bytes", bytes.len());
        b.iter(|| bytes.len())
    });

    group.bench_function("bincode/large", |b| {
        let bytes = bincode::serialize(&large).unwrap();
        eprintln!("  bincode large: {} bytes", bytes.len());
        b.iter(|| bytes.len())
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_serialize,
    bench_deserialize,
    bench_roundtrip,
    bench_wire_size
);
criterion_main!(benches);
