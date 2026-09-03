//! The consistent hash ring (feature `ring`).

pub mod hash;

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fmt;
use std::mem::size_of;

use bytes::Bytes;

use self::hash::HashFn;

/// A node that can be placed on a consistent hash ring.
///
/// A node exists in two forms — a physical node and the virtual points derived
/// from it — but the trait abstracts over both so ring logic never has to know
/// which it's holding. The `P` type parameter is the "virtual pass" value (a
/// replica index) used to derive distinct virtual points.
pub trait Member<P> {
    /// Returns the inner bytes slice which represents this node
    fn inner(&self) -> &Bytes;

    /// Creates a virtual Member from a Physical instance.
    /// If a Virtual instance is provided, itself is returned.
    fn make_virtual(&self, password: P) -> Self;

    /// Returns the physical instance from a virtual instance
    fn physical_from_virtual(&self) -> Self;

    /// Creates a physical member from a provided key.
    fn new_physical(key: &str) -> Self;
}

/// The core consistent-hashing operations, independent of the underlying
/// ordered-map implementation.
///
/// Implementors place virtual points on an ordered space and resolve keys to the
/// first node clockwise from the key's hash. Keeping this a trait (rather than a
/// concrete struct) lets alternative layouts or data structures satisfy the same
/// contract.
pub trait Ring<P: Clone, M: Member<P>> {
    /// Places `member` on the ring, spread across `replicas` virtual points.
    fn add_node(&mut self, member: M, replicas: usize);
    fn delete_node(&mut self, member: M);

    /// Resolves `key` to the first node clockwise from its hash.
    fn get_node(&self, key: &[u8]) -> Option<&M>;

    /// Resolves `key` to the `k` *distinct physical* successors clockwise from
    /// its hash, for replication / read-repair. Returns fewer than `k` only if
    /// the cluster has fewer than `k` nodes; empty if `k == 0`.
    fn get_nodes(&self, key: &[u8], k: usize) -> Vec<&M>;

    /// Whether `member` (a physical node) is currently on the ring.
    fn contains_node(&self, member: &M) -> bool;

    /// The physical nodes currently on the ring, as borrowed name bytes, in
    /// unspecified order. Iterating borrows the ring; no allocation or clone.
    fn nodes(&self) -> impl Iterator<Item = &Bytes>;

    fn prune(&mut self);

    /// Number of virtual points on the ring (`nodes × replicas`).
    fn len(&self) -> usize;

    /// Whether the ring holds no virtual points.
    fn is_empty(&self) -> bool;
}

/// A node on the ring, in one of two forms.
///
/// - [`Physical`](RingMember::Physical) — the real node, identified by its name
///   bytes (e.g. `"10.0.0.1:6379"`). This is what a caller adds/removes and what
///   a lookup ultimately resolves to.
/// - [`Virtual`](RingMember::Virtual) — a synthetic point on the ring derived
///   from a physical node by appending a replica index to its name. Each
///   physical node spreads across many virtual points so load is evened out.
///
/// Virtual points are *derived data*: they can always be regenerated from the
/// physical node (see [`Member::make_virtual`]), so only physical nodes are
/// tracked as first-class state.
#[derive(Clone)]
pub enum RingMember {
    Physical(Bytes),
    Virtual(Bytes),
}

impl Member<usize> for RingMember {
    /// Returns a reference to the underlying Bytes, regardless of the variant
    fn inner(&self) -> &Bytes {
        match self {
            RingMember::Physical(b) | RingMember::Virtual(b) => b,
        }
    }

    fn make_virtual(&self, virtual_pass: usize) -> RingMember {
        match self {
            RingMember::Physical(_) => {
                let inner = self.inner().as_ref();
                let v = [inner, &virtual_pass.to_be_bytes()].concat();
                RingMember::Virtual(Bytes::from(v))
            }
            RingMember::Virtual(_) => self.clone(), // Ok to clone, Bytes cloning is cheap due to ref counter
        }
    }

    fn physical_from_virtual(&self) -> RingMember {
        match self {
            RingMember::Physical(_) => self.clone(),
            RingMember::Virtual(b) => {
                // A virtual node is `name ++ usize(replica)`; strip the trailing
                // `usize` to recover the physical name.
                let int_size = size_of::<usize>();
                let name = if b.len() >= int_size {
                    b.slice(..b.len() - int_size)
                } else {
                    b.clone()
                };
                RingMember::Physical(name)
            }
        }
    }

    fn new_physical(key: &str) -> RingMember {
        RingMember::Physical(Bytes::copy_from_slice(key.as_bytes()))
    }
}

impl fmt::Display for RingMember {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RingMember::Physical(b) => {
                // Physical nodes are just standard text bytes
                let text = String::from_utf8_lossy(b);
                write!(f, "{}", text)
            }
            RingMember::Virtual(b) => {
                // Get the size of usize integers
                // Virtual nodes are encoded by appending the virtual pass (replica index) to the node bytes representation
                let int_size = size_of::<usize>();

                // Ensure the bytes are long enough to contain our appended integer
                if b.len() >= int_size {
                    // Split the bytes: everything up to the last 8 bytes is the name
                    let (name_bytes, int_bytes) = b.split_at(b.len() - int_size);

                    let name = String::from_utf8_lossy(name_bytes);

                    // Convert the last 8 bytes back into a usize
                    // or return 0
                    let index = usize::from_be_bytes(
                        int_bytes.try_into().unwrap_or([0u8; size_of::<usize>()]),
                    );

                    // Print it out beautifully!
                    write!(f, "{}-{}", name, index)
                } else {
                    // Fallback if something went weird
                    write!(f, "{}", String::from_utf8_lossy(b))
                }
            }
        }
    }
}

/// A consistent hash ring.
///
/// Nodes live on a circular space keyed by the hash type `V`. A key is assigned
/// to the first node clockwise from the key's own hash (wrapping around the
/// end), which is why the underlying map must be ordered. Adding or removing one
/// node only remaps a small fraction of keys — unlike `hash(key) % n`, which
/// reshuffles everything.
///
/// Use [`hash128`](crate::ring::hash::hash128) (a `u128`) for `V` so that two
/// virtual points colliding is vanishingly unlikely — on a `u64` ring a
/// collision silently overwrites one of the two points.
pub struct ConsistentHashRing<V, M: Member<usize>> {
    /// Ordered map of `hash(virtual point) -> node`; `V` must be `Ord` so a key
    /// can find its successor via a range query.
    ring: BTreeMap<V, M>,
    /// Hashes both node names and keys onto the ring.
    hasher: HashFn<V>,
    /// Physical node name -> how many virtual points it owns, so `delete_node`
    /// can regenerate and remove exactly the right points.
    replica_counts: HashMap<Bytes, usize>,
}

impl<V, M: Member<usize>> ConsistentHashRing<V, M> {
    pub fn new(hasher: HashFn<V>) -> Self {
        ConsistentHashRing {
            ring: BTreeMap::new(),
            hasher,
            replica_counts: Default::default(),
        }
    }
}

impl<H: Ord + Clone, M: Member<usize>> Ring<usize, M> for ConsistentHashRing<H, M> {
    fn add_node(&mut self, member_def: M, replicas: usize) {
        assert!(replicas > 0, "replicas must be at least 1");
        let name = member_def.inner().clone();

        // Idempotent: re-adding a node first drops its old virtual points.
        if let Some(&existing) = self.replica_counts.get(&name) {
            for i in 0..existing {
                let v = member_def.make_virtual(i);
                let hash = (self.hasher)(v.inner().as_ref());
                self.ring.remove(&hash);
            }
        }

        for i in 0..replicas {
            let v = member_def.make_virtual(i);
            let hash = (self.hasher)(v.inner().as_ref());
            self.ring.insert(hash, v);
        }
        self.replica_counts.insert(name, replicas);
    }

    fn delete_node(&mut self, member_def: M) {
        let name = member_def.inner().clone();
        let Some(&replicas) = self.replica_counts.get(&name) else {
            return; // not in the cluster
        };

        for i in 0..replicas {
            let v = member_def.make_virtual(i);
            let hash = (self.hasher)(v.inner().as_ref());
            self.ring.remove(&hash);
        }
        self.replica_counts.remove(&name);
    }

    fn get_node(&self, key: &[u8]) -> Option<&M> {
        if self.ring.is_empty() {
            return None;
        }

        let hash = (self.hasher)(key);
        self.ring
            .range(hash..)
            .next()
            .or_else(|| self.ring.iter().next())
            .map(|(_, v)| v)
    }

    fn get_nodes(&self, key: &[u8], k: usize) -> Vec<&M> {
        if k == 0 || self.ring.is_empty() {
            return Vec::new();
        }

        let hash = (self.hasher)(key);
        // At most `k` results, and never more than the number of distinct
        // physical nodes.
        let mut out = Vec::with_capacity(k.min(self.replica_counts.len()));
        let mut seen = HashSet::new();

        // Walk clockwise from the key, wrapping past the end, and keep the
        // first `k` *distinct physical* nodes (replicas of the same node would
        // defeat replication, so they're skipped).
        for (_, v) in self
            .ring
            .range(hash.clone()..)
            .chain(self.ring.range(..hash))
        {
            let physical = v.physical_from_virtual();
            if seen.insert(physical.inner().clone()) {
                out.push(v);
                if out.len() == k {
                    break;
                }
            }
        }
        out
    }

    fn contains_node(&self, member: &M) -> bool {
        self.replica_counts.contains_key(member.inner())
    }

    fn nodes(&self) -> impl Iterator<Item = &Bytes> {
        self.replica_counts.keys()
    }

    fn prune(&mut self) {
        self.ring.clear();
        self.replica_counts.clear();
    }

    fn len(&self) -> usize {
        self.ring.len()
    }

    fn is_empty(&self) -> bool {
        self.ring.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::hash::hash128;
    use super::*;

    fn physical(name: &str) -> RingMember {
        RingMember::Physical(Bytes::copy_from_slice(name.as_bytes()))
    }

    #[test]
    fn test_ring() {
        let mut ring = ConsistentHashRing::new(hash128);

        ring.add_node(physical("node-a"), 32);
        ring.add_node(physical("node-b"), 32);
        assert_eq!(ring.len(), 64);

        // A key resolves to some physical node.
        let key = Bytes::from_static(b"some-key");
        let resolved = ring
            .get_node(&key)
            .map(|v| v.physical_from_virtual().to_string())
            .expect("ring is not empty");
        assert!(resolved == "node-a" || resolved == "node-b");

        // The same key always maps to the same node.
        let again = ring
            .get_node(&key)
            .map(|v| v.physical_from_virtual().to_string())
            .unwrap();
        assert_eq!(resolved, again);

        // Removing nodes empties their replicas.
        ring.delete_node(physical("node-a"));
        ring.delete_node(physical("node-b"));
        assert!(ring.is_empty());
        assert!(ring.get_node(&key).is_none());
    }

    #[test]
    fn per_node_replicas_and_idempotent_readd() {
        let mut ring = ConsistentHashRing::new(hash128);

        // Different nodes may carry different replica counts (weighting).
        ring.add_node(physical("heavy"), 40);
        ring.add_node(physical("light"), 8);
        assert_eq!(ring.len(), 48);

        // Re-adding the same node re-positions it rather than duplicating.
        ring.add_node(physical("heavy"), 40);
        assert_eq!(ring.len(), 48);

        // Removing an absent node is a no-op.
        ring.delete_node(physical("ghost"));
        assert_eq!(ring.len(), 48);
    }

    #[test]
    fn prune_clears_the_ring() {
        let mut ring = ConsistentHashRing::new(hash128);
        ring.add_node(physical("node-a"), 8);
        ring.prune();
        assert!(ring.is_empty());
    }

    #[test]
    fn lookup_wraps_around_past_the_last_node() {
        let mut ring = ConsistentHashRing::new(hash128);
        ring.add_node(physical("node-a"), 1);
        ring.add_node(physical("node-b"), 1);

        // Find a key whose hash is strictly greater than every virtual point on
        // the ring, so the clockwise search must wrap to the front.
        let key = (0..100_000u32)
            .map(|i| i.to_be_bytes())
            .find(|bytes| {
                let h = hash128(bytes);
                ring.ring.range(h..).next().is_none()
            })
            .expect("a wrap-around key must exist");

        // It still resolves — to the first node at the front of the ring.
        let got = ring
            .get_node(&key)
            .map(|v| v.physical_from_virtual().to_string())
            .expect("ring is not empty");
        assert!(got == "node-a" || got == "node-b");
    }

    #[test]
    fn get_nodes_returns_distinct_physical_successors() {
        let mut ring = ConsistentHashRing::new(hash128);
        ring.add_node(physical("node-a"), 8);
        ring.add_node(physical("node-b"), 8);
        ring.add_node(physical("node-c"), 8);

        // k == 2 gives two distinct physical nodes, even though each node owns
        // many virtual points.
        let two = ring.get_nodes(b"some-key", 2);
        assert_eq!(two.len(), 2);
        let names: Vec<String> = two
            .iter()
            .map(|m| m.physical_from_virtual().to_string())
            .collect();
        assert_ne!(names[0], names[1]);

        // k > node count clamps to the number of distinct physical nodes.
        assert_eq!(ring.get_nodes(b"some-key", 10).len(), 3);

        // k == 0 is empty, and an empty ring yields nothing.
        assert!(ring.get_nodes(b"some-key", 0).is_empty());
        ring.prune();
        assert!(ring.get_nodes(b"some-key", 3).is_empty());
    }

    #[test]
    fn accessors_report_membership_and_nodes() {
        let mut ring = ConsistentHashRing::new(hash128);
        ring.add_node(physical("node-a"), 4);
        ring.add_node(physical("node-b"), 4);

        assert!(ring.contains_node(&physical("node-a")));
        assert!(!ring.contains_node(&physical("ghost")));

        // nodes() borrows the ring; no allocation.
        let mut names: Vec<&str> = ring
            .nodes()
            .map(|b| std::str::from_utf8(b).unwrap())
            .collect();
        names.sort_unstable();
        assert_eq!(names, ["node-a", "node-b"]);

        ring.delete_node(physical("node-a"));
        assert!(!ring.contains_node(&physical("node-a")));
        assert_eq!(ring.nodes().count(), 1);
    }
}
