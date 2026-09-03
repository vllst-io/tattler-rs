//! A thread-safe cluster router built on the consistent hash ring (feature
//! `router`).

use std::sync::RwLock;

use bytes::Bytes;

use crate::ring::hash::hash64;
use crate::ring::{ConsistentHashRing, Member, Ring, RingMember};

/// Routes keys to the physical node responsible for them.
///
/// Reads (routing) are the hot path, so the ring is shared behind a read-write
/// lock: many concurrent readers, one writer during topology changes. `Router`
/// is `Send + Sync`; share it as `std::sync::Arc<Router>` across threads.
pub struct Router {
    ring: RwLock<ConsistentHashRing<u64, RingMember>>,
}

impl Router {
    /// Creates an empty router (no nodes).
    pub fn new() -> Self {
        Router {
            ring: RwLock::new(ConsistentHashRing::new(hash64)),
        }
    }

    /// Adds a physical node, spread across `replicas` virtual points.
    ///
    /// # Panics
    ///
    /// Panics if `replicas == 0`.
    pub fn add_node(&self, name: &str, replicas: usize) {
        self.ring
            .write()
            .expect("ring poisoned")
            .add_node(RingMember::new_physical(name), replicas);
    }

    /// Removes a physical node and all of its virtual points. A no-op if the
    /// node is not present.
    pub fn remove_node(&self, name: &str) {
        self.ring
            .write()
            .expect("ring poisoned")
            .delete_node(RingMember::new_physical(name));
    }

    /// Routes `key` to the physical node responsible for it, returning the
    /// node's name bytes — or `None` if the cluster is empty.
    pub fn route(&self, key: &[u8]) -> Option<Bytes> {
        self.ring
            .read()
            .expect("ring poisoned")
            .get_node(key)
            .map(|m| m.physical_from_virtual().inner().clone())
    }
}

impl Default for Router {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn routes_consistently_and_removes() {
        let router = Router::new();
        router.add_node("node-a", 64);
        router.add_node("node-b", 64);

        // The same key always maps to the same node.
        let first = router.route(b"some-key").unwrap();
        for _ in 0..100 {
            assert_eq!(router.route(b"some-key"), Some(first.clone()));
        }

        // Removing a node hands its keys to the survivor.
        router.remove_node("node-a");
        assert_eq!(
            router.route(b"some-key").unwrap(),
            Bytes::from_static(b"node-b")
        );

        // An empty cluster routes to nothing.
        router.remove_node("node-b");
        assert!(router.route(b"some-key").is_none());
    }

    #[test]
    fn is_shareable_across_threads() {
        let router = Arc::new(Router::new());
        for name in ["a", "b", "c"] {
            router.add_node(name, 32);
        }

        let mut handles = Vec::new();
        for t in 0..8 {
            let router = Arc::clone(&router);
            handles.push(thread::spawn(move || {
                for i in 0..1_000 {
                    let key = format!("t{t}-k{i}");
                    let node = router.route(key.as_bytes()).expect("cluster non-empty");
                    let name = String::from_utf8_lossy(&node).to_string();
                    assert!(name == "a" || name == "b" || name == "c");
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }
    }
}
