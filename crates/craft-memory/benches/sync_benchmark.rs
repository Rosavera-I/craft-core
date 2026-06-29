//! Benchmark for distributed memory sync performance

#![allow(clippy::unwrap_used)]

use craft_memory::{
    crdt::{
        Mergeable, NodeId, lww::LwwRegister, merkle::MerkleTree, or_set::OrSet,
        vector_clock::VectorClock,
    },
    crypto::{NoiseHandshake, SymmetricCipher, X25519Secret},
};
use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};

/// Benchmark CRDT merge operations
fn bench_crdt_merge(c: &mut Criterion) {
    let mut group = c.benchmark_group("crdt_merge");

    // LWW Register merge
    group.bench_function("lww_reg_merge", |b| {
        let node1 = NodeId::from("node-1");
        let node2 = NodeId::from("node-2");

        let mut reg1 = LwwRegister::with_timestamp("v1".to_string(), node1.clone(), 100);
        let reg2 = LwwRegister::with_timestamp("v2".to_string(), node2, 200);

        b.iter(|| {
            reg1.merge(black_box(&reg2));
        });
    });

    // OR-Set merge
    for size in [10, 100, 1000].iter() {
        group.bench_with_input(BenchmarkId::new("or_set_merge", size), size, |b, &size| {
            let node1 = NodeId::from("node-1");
            let node2 = NodeId::from("node-2");

            let mut set1 = OrSet::new(node1);
            let mut set2 = OrSet::new(node2);

            for i in 0..size {
                set1.add(format!("item{}", i));
                set2.add(format!("item{}", i + size));
            }

            b.iter(|| {
                set1.merge(black_box(&set2));
            });
        });
    }

    group.finish();
}

/// Benchmark Merkle tree operations
fn bench_merkle_tree(c: &mut Criterion) {
    let mut group = c.benchmark_group("merkle_tree");

    // Build tree from different sizes
    for size in [100, 500, 1000].iter() {
        group.bench_with_input(BenchmarkId::new("build", size), size, |b, &size| {
            let entries: Vec<(String, Vec<u8>)> = (0..size)
                .map(|i| (format!("key{}", i), format!("value{}", i).into_bytes()))
                .collect();

            b.iter(|| {
                let _tree = MerkleTree::from_entries(black_box(&entries));
            });
        });
    }

    // Compute diff between trees
    for size in [100, 500].iter() {
        group.bench_with_input(BenchmarkId::new("diff", size), size, |b, &size| {
            let entries1: Vec<(String, Vec<u8>)> = (0..size)
                .map(|i| (format!("key{}", i), format!("value{}", i).into_bytes()))
                .collect();

            let mut entries2 = entries1.clone();
            // Change a few entries
            for (i, entry) in entries2.iter_mut().enumerate().take(10.min(size)) {
                *entry = (format!("key{}", i), format!("changed{}", i).into_bytes());
            }

            let tree1 = MerkleTree::from_entries(&entries1);
            let tree2 = MerkleTree::from_entries(&entries2);

            b.iter(|| {
                let _diff = tree1.diff(black_box(&tree2));
            });
        });
    }

    group.finish();
}

/// Benchmark encryption operations
fn bench_encryption(c: &mut Criterion) {
    let mut group = c.benchmark_group("encryption");

    // Different payload sizes
    for size in [100, 512, 1024, 4096].iter() {
        group.bench_with_input(
            BenchmarkId::new("encrypt_decrypt", size),
            size,
            |b, &size| {
                let key: [u8; 32] = rand::random();
                let cipher = SymmetricCipher::from_shared_secret(&key);
                let data = vec![0u8; size];

                b.iter(|| {
                    let encrypted = cipher.encrypt(black_box(&data)).unwrap();
                    let _decrypted = cipher.decrypt(&encrypted).unwrap();
                });
            },
        );
    }

    // Handshake
    group.bench_function("noise_handshake", |b| {
        let secret2 = X25519Secret::generate();
        let pub2 = secret2.public_key();

        b.iter(|| {
            let mut handshake = NoiseHandshake::new(X25519Secret::generate());
            let _ephemeral = handshake.send_ephemeral();
            let _ = black_box(handshake.complete_handshake(&pub2, &pub2));
        });
    });

    group.finish();
}

/// Benchmark vector clock operations
fn bench_vector_clock(c: &mut Criterion) {
    let mut group = c.benchmark_group("vector_clock");

    // Increment
    group.bench_function("increment", |b| {
        let mut vc = VectorClock::new();
        b.iter(|| {
            vc.increment(black_box("node-1"));
        });
    });

    // Merge
    for size in [5, 10, 50].iter() {
        group.bench_with_input(BenchmarkId::new("merge", size), size, |b, &size| {
            let mut vc1 = VectorClock::new();
            let mut vc2 = VectorClock::new();

            for i in 0..size {
                vc1.increment(&format!("node{}", i));
                vc2.increment(&format!("node{}", i));
            }

            b.iter(|| {
                // Clone to avoid mutating original
                let mut v1 = vc1.clone();
                v1.merge(black_box(&vc2));
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_crdt_merge,
    bench_merkle_tree,
    bench_encryption,
    bench_vector_clock,
);
criterion_main!(benches);
