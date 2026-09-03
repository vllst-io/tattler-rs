//! `tattler` — a distributed-systems toolkit built on consistent hashing.
//!
//! # The problem
//!
//! A distributed cache or K-V store must decide which node owns each key. The
//! naive approach hashes the key and takes the modulus over the node count:
//!
//! ```text
//! node = hash(key) % nodes.len()
//! ```
//!
//! This breaks the moment a node joins or leaves: `nodes.len()` changes, so
//! nearly *every* key suddenly maps to a different node, causing a full rebalance
//! and a storm of cache misses.
//!
//! # The ring
//!
//! Consistent hashing instead hashes the *nodes themselves* onto a fixed circular
//! space — the ring — and assigns a key to the first node clockwise from the
//! key's own hash:
//!
//! ```text
//! node = the first node whose hash >= hash(key), wrapping around the end
//! ```
//!
//! Because a node's position depends only on its name, adding or removing a node
//! only reassigns the keys that previously belonged to that node — roughly
//! `1 / n` of the total — leaving every other key untouched. The mapping changes
//! as little as possible, which is the entire point.
//!
//! # Virtual nodes
//!
//! One node sitting at a single point can crowd its neighbors and skew load, so
//! each physical node is spread across many *virtual* points (replicas), each
//! derived from the node's name plus a replica index. More replicas give a
//! smoother, more even distribution across the ring.
//!
//! # Features
//!
//! The crate is split into optional, feature-gated subsystems:
//!
//! - `ring` (default) — the consistent hash ring itself.
//! - `router` — a thread-safe cluster router over the ring.
//! - `memberlist` — gossip-based cluster membership (not yet implemented).

#[cfg(feature = "ring")]
pub mod ring;

#[cfg(feature = "memberlist")]
pub mod memberlist;

#[cfg(feature = "router")]
pub mod router;

// `Bytes` appears in the ring's public API, so re-export it so downstream code
// doesn't need to add `bytes` as its own dependency.
#[cfg(feature = "ring")]
pub use bytes::Bytes;
