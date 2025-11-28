//! Vector clocks for causal ordering

use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;

/// Vector clock for tracking causality in distributed systems
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorClock {
    /// Clock map: Node DID -> sequence number
    pub clock: HashMap<Did, u64>,
}

impl VectorClock {
    /// Create a new empty vector clock
    pub fn new() -> Self {
        VectorClock {
            clock: HashMap::new(),
        }
    }

    /// Increment the clock for a given node
    pub fn increment(&mut self, node: &Did) {
        let count = self.clock.entry(node.clone()).or_insert(0);
        *count += 1;
    }

    /// Get the count for a node
    pub fn get(&self, node: &Did) -> u64 {
        *self.clock.get(node).unwrap_or(&0)
    }

    /// Merge another vector clock into this one (take max of each entry)
    pub fn merge(&mut self, other: &VectorClock) {
        for (node, count) in &other.clock {
            let entry = self.clock.entry(node.clone()).or_insert(0);
            *entry = (*entry).max(*count);
        }
    }

    /// Check if this clock happened before another (this < other)
    ///
    /// Returns true if:
    /// - All entries in this clock are <= corresponding entries in other
    /// - At least one entry in this clock is < corresponding entry in other
    pub fn happened_before(&self, other: &VectorClock) -> bool {
        let all_less_or_equal = true;
        let mut at_least_one_less = false;

        // Check all entries in this clock
        for (node, &count) in &self.clock {
            let other_count = other.get(node);
            if count > other_count {
                return false; // Found an entry where this > other
            }
            if count < other_count {
                at_least_one_less = true;
            }
        }

        // Check for entries in other but not in this (implicitly this=0 < other)
        for (node, &other_count) in &other.clock {
            if !self.clock.contains_key(node) && other_count > 0 {
                at_least_one_less = true;
            }
        }

        all_less_or_equal && at_least_one_less
    }

    /// Check if this clock happened after another (this > other)
    pub fn happened_after(&self, other: &VectorClock) -> bool {
        other.happened_before(self)
    }

    /// Check if two clocks are concurrent (neither happened before the other)
    pub fn is_concurrent(&self, other: &VectorClock) -> bool {
        !self.happened_before(other) && !other.happened_before(self) && self != other
    }

    /// Compare two vector clocks
    pub fn partial_cmp(&self, other: &VectorClock) -> Option<Ordering> {
        if self == other {
            Some(Ordering::Equal)
        } else if self.happened_before(other) {
            Some(Ordering::Less)
        } else if self.happened_after(other) {
            Some(Ordering::Greater)
        } else {
            None // Concurrent
        }
    }
}

impl Default for VectorClock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    #[test]
    fn test_new_clock() {
        let clock = VectorClock::new();
        assert!(clock.clock.is_empty());
    }

    #[test]
    fn test_increment() {
        let mut clock = VectorClock::new();
        let node = KeyPair::generate().unwrap().did().clone();

        clock.increment(&node);
        assert_eq!(clock.get(&node), 1);

        clock.increment(&node);
        assert_eq!(clock.get(&node), 2);
    }

    #[test]
    fn test_merge() {
        let node_a = KeyPair::generate().unwrap().did().clone();
        let node_b = KeyPair::generate().unwrap().did().clone();

        let mut clock1 = VectorClock::new();
        clock1.increment(&node_a);
        clock1.increment(&node_a);
        clock1.increment(&node_b);

        let mut clock2 = VectorClock::new();
        clock2.increment(&node_a);
        clock2.increment(&node_b);
        clock2.increment(&node_b);

        clock1.merge(&clock2);

        assert_eq!(clock1.get(&node_a), 2); // max(2, 1)
        assert_eq!(clock1.get(&node_b), 2); // max(1, 2)
    }

    #[test]
    fn test_happened_before() {
        let node_a = KeyPair::generate().unwrap().did().clone();
        let node_b = KeyPair::generate().unwrap().did().clone();

        let mut clock1 = VectorClock::new();
        clock1.increment(&node_a);

        let mut clock2 = VectorClock::new();
        clock2.increment(&node_a);
        clock2.increment(&node_b);

        assert!(clock1.happened_before(&clock2));
        assert!(!clock2.happened_before(&clock1));
    }

    #[test]
    fn test_concurrent() {
        let node_a = KeyPair::generate().unwrap().did().clone();
        let node_b = KeyPair::generate().unwrap().did().clone();

        let mut clock1 = VectorClock::new();
        clock1.increment(&node_a);

        let mut clock2 = VectorClock::new();
        clock2.increment(&node_b);

        assert!(clock1.is_concurrent(&clock2));
        assert!(clock2.is_concurrent(&clock1));
    }

    #[test]
    fn test_equal_clocks() {
        let node = KeyPair::generate().unwrap().did().clone();

        let mut clock1 = VectorClock::new();
        clock1.increment(&node);

        let mut clock2 = VectorClock::new();
        clock2.increment(&node);

        assert!(!clock1.happened_before(&clock2));
        assert!(!clock2.happened_before(&clock1));
        assert!(!clock1.is_concurrent(&clock2));
        assert_eq!(clock1, clock2);
    }

    #[test]
    fn test_partial_cmp() {
        let node_a = KeyPair::generate().unwrap().did().clone();
        let node_b = KeyPair::generate().unwrap().did().clone();

        let mut clock1 = VectorClock::new();
        clock1.increment(&node_a);

        let mut clock2 = VectorClock::new();
        clock2.increment(&node_a);
        clock2.increment(&node_b);

        assert_eq!(clock1.partial_cmp(&clock2), Some(Ordering::Less));
        assert_eq!(clock2.partial_cmp(&clock1), Some(Ordering::Greater));
        assert_eq!(clock1.partial_cmp(&clock1), Some(Ordering::Equal));

        let mut clock3 = VectorClock::new();
        clock3.increment(&node_b);

        assert_eq!(clock1.partial_cmp(&clock3), None); // Concurrent
    }
}
