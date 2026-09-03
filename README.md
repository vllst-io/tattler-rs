# tattler

[![CI](https://github.com/vllst-io/tattler-rs/actions/workflows/ci.yaml/badge.svg)](https://github.com/vllst-io/tattler-rs/actions/workflows/ci.yaml)
[![crates.io](https://img.shields.io/crates/v/tattler.svg)](https://crates.io/crates/tattler)
[![docs.rs](https://docs.rs/tattler/badge.svg)](https://docs.rs/tattler)
[![MSRV](https://img.shields.io/badge/MSRV-1.97-blue.svg)](https://crates.io/crates/tattler)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)

A modern Rust toolkit for building distributed systems on top of **consistent
hashing**.

When a cluster of nodes shares a keyspace — a distributed cache, a sharded
key–value store, a session store — every request must be routed to the node that
owns the key. `tattler` provides the pieces to do that correctly and cheaply.

```rust
use tattler::ring::{hash::hash128, ConsistentHashRing, Member, Ring, RingMember};

let mut ring = ConsistentHashRing::new(hash128);
ring.add_node(RingMember::new_physical("cache-1"), 32);
ring.add_node(RingMember::new_physical("cache-2"), 32);

// A lookup returns the physical node responsible for the key.
let owner = ring.get_node(b"user:42").unwrap();
assert_eq!(owner.to_string(), "cache-1" /* or "cache-2" */);

// For replication / read-repair, ask for `k` distinct successors instead.
let replicas = ring.get_nodes(b"user:42", 2);
```

## Why consistent hashing?

The naive approach, `hash(key) % nodes.len()`, breaks the moment a node joins or
leaves: the divisor changes, so nearly *every* key remaps to a different node.
That means a full rebalance and a storm of cache misses.

Consistent hashing hashes the *nodes themselves* onto a fixed circular space — a
**ring** — and assigns each key to the first node clockwise from the key's own
hash. Because a node's position depends only on its name, adding or removing a
node reassigns only the keys that belonged to it (roughly `1 / n` of the total),
leaving every other mapping untouched.

## Features

The crate is split into optional, Cargo-feature-gated subsystems so you can
depend on exactly what you need.

- **`ring`** *(default)* — the consistent hash ring.
  - `ConsistentHashRing` built on an ordered `BTreeMap` for `O(log n)` lookups.
  - Fast, non-cryptographic **xxHash** (`twox-hash`) for dispersion, on a
    **128-bit position space** (`hash128`) so virtual-point collisions are
    vanishingly rare.
  - **Virtual nodes**: each physical node spreads across `N` replicas for an even,
    tunable distribution (per-node replica counts enable weighted nodes).
  - Idempotent add / reliable remove: replica counts are tracked per physical
    node, so a node and all its virtual points can be removed exactly.
  - **Replication-aware lookups**: `get_nodes(key, k)` returns the `k` distinct
    physical successors clockwise from `key` (for replication / read-repair);
    `contains_node` and `nodes()` report current membership.

- **`router`** — a thread-safe cluster router over the ring.
  - Routes any key to its owning node with a lock-free read path (`RwLock`:
    many readers, one writer).
  - `Send + Sync`; share it as `Arc<Router>` across worker threads.
  - Allocation-free `route(&[u8])` hot path.

- **`memberlist`** — gossip-based cluster membership *(not yet implemented)*.
  - Planned: failure detection, membership dissemination, and automatic ring
    updates when the cluster topology changes.

### Feature matrix

| Feature      | Default | Enables                         | Dependencies pulled  |
|--------------|---------|---------------------------------|----------------------|
| `ring`       | ✅      | the ring + `hash128`            | `bytes`, `twox-hash` |
| `router`     | WIP     | `Router` (also enables `ring`)  | `bytes` (via `ring`) |
| `memberlist` | WIP     | gossip membership (placeholder) | —                    |

## How `tattler` differs

- **128-bit positions.** The ring keys on a `u128` (`hash128`, xxHash3), so two
  virtual points colliding is vanishingly unlikely. On a `u64` ring a collision
  silently overwrites one of the two points.
- **Weighted virtual nodes.** Each physical node carries its *own* replica
  count, so `add_node("heavy", 40)` and `add_node("light", 8)` spread load
  unevenly on purpose — no separate weighting layer.
- **Lookups return the physical node.** Virtual points are derived data; the
  ring collapses them on the way out, so callers never deal with a
  `Virtual` variant.
- **Replication built in.** `get_nodes(key, k)` walks clockwise and returns `k`
  *distinct physical* successors for read-repair, deduplicating replicas of the
  same node.
- **Feature-gated.** Depend on just `ring`, add `router` for the shared
  thread-safe wrapper, and pull `memberlist` (gossip) when it lands.

## Usage

Add `tattler` to your `Cargo.toml`:

```toml
[dependencies]
tattler = "0.1"          # ring (default)
```

- `ring` is the default feature 
- `router` for the thread-safe in-cluster message router (WIP).
- `memberlist` is planned but not yet implemented (WIP).

MSRV is **1.97**. For a runnable demo that routes keys across a toy ring:

```sh
cargo run --example cache
```

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
