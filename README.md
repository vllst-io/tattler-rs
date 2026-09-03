# tattler

A modern Rust toolkit for building distributed systems on top of **consistent
hashing**.

When a cluster of nodes shares a keyspace — a distributed cache, a sharded
key–value store, a session store — every request must be routed to the node that
owns the key. `tattler` provides the pieces to do that correctly and cheaply.

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
| `router`     | —       | `Router` (also enables `ring`)  | `bytes` (via `ring`) |
| `memberlist` | —       | gossip membership (placeholder) | —                    |

## Usage

Add `tattler` to your `Cargo.toml`:

```toml
[dependencies]
tattler = "0.1"
```

Note: only the ring feature is available for now.

## License

TBD.
