//! A toy sharded-cache demo: place a few nodes on a consistent-hash ring and
//! route keys the way a real cache would.
//!
//! Run with `cargo run --example cache`.

use std::collections::HashMap;

use tattler::ring::hash::hash128;
use tattler::ring::{ConsistentHashRing, Member, Ring, RingMember};

const REPLICAS: usize = 64;

fn main() {
    // Three "cache nodes", each spread across 64 virtual points.
    let mut ring = ConsistentHashRing::new(hash128);
    for name in ["cache-a", "cache-b", "cache-c"] {
        ring.add_node(RingMember::new_physical(name), REPLICAS);
    }

    // Route a batch of keys and tally where each lands.
    let mut counts: HashMap<String, usize> = HashMap::new();
    for i in 0..1_000 {
        let key = format!("user:{i}");
        let owner = ring
            .get_node(key.as_bytes())
            .map(|m| m.to_string())
            .expect("ring is not empty");
        *counts.entry(owner).or_insert(0) += 1;
    }

    println!(
        "Key distribution over {} physical nodes:",
        ring.nodes().count()
    );
    let mut entries: Vec<_> = counts.into_iter().collect();
    entries.sort();
    for (node, n) in entries {
        println!("  {node:>8}  {n:>4} keys");
    }

    // Replication: the two distinct successors of a key, for read-repair.
    let replicas: Vec<String> = ring
        .get_nodes(b"user:0", 2)
        .iter()
        .map(|m| m.to_string())
        .collect();
    println!("Replicas for `user:0`: {replicas:?}");
}
