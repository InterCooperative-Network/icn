//! Time Primitive
//!
//! Provides logical clocks, scheduling, and leases.
//!
//! # Design
//!
//! Time in distributed systems is complex. This primitive provides:
//! - Logical timestamps for ordering (not wall-clock truth)
//! - Scheduling at logical times or after durations
//! - Leases for time-limited resource access
//!
//! # Non-Goals
//!
//! - Wall-clock truth (impossible in distributed systems)
//! - Calendar/timezone concepts
//! - "Vote ends at 5pm" semantics (app layer interprets deadlines)

use crate::types::{CallbackId, Duration, Lease, LogicalTimestamp, ResourceId, ScheduleId};
use std::cmp::Ordering;

/// Callback for scheduled events.
pub type ScheduleCallback = Box<dyn Fn() + Send + Sync>;

/// Time service for ordering and scheduling.
pub trait TimeService: Send + Sync {
    /// Get the current logical timestamp.
    ///
    /// This is NOT wall-clock time. It's a monotonic counter
    /// that provides ordering within the node's scope.
    fn now(&self) -> LogicalTimestamp;

    /// Compare two logical timestamps.
    ///
    /// Returns ordering, or None if timestamps are from
    /// different causal chains (incomparable).
    fn compare(&self, a: LogicalTimestamp, b: LogicalTimestamp) -> Option<Ordering>;

    /// Advance the logical clock.
    ///
    /// Used when receiving messages from other nodes to
    /// maintain causality.
    fn advance(&self, observed: LogicalTimestamp);

    /// Schedule a callback at a logical time.
    fn schedule_at(
        &self,
        time: LogicalTimestamp,
        callback: CallbackId,
    ) -> Result<ScheduleId, TimeError>;

    /// Schedule a callback after a duration.
    fn schedule_after(
        &self,
        duration: Duration,
        callback: CallbackId,
    ) -> Result<ScheduleId, TimeError>;

    /// Schedule a recurring callback.
    fn schedule_recurring(
        &self,
        interval: Duration,
        callback: CallbackId,
    ) -> Result<ScheduleId, TimeError>;

    /// Cancel a scheduled callback.
    fn cancel(&self, schedule_id: &ScheduleId) -> Result<(), TimeError>;

    /// Acquire a lease on a resource.
    ///
    /// Leases provide time-limited exclusive access.
    fn acquire_lease(&self, resource: &ResourceId, duration: Duration) -> Result<Lease, TimeError>;

    /// Renew an existing lease.
    fn renew_lease(&self, lease: &Lease, duration: Duration) -> Result<Lease, TimeError>;

    /// Release a lease early.
    fn release_lease(&self, lease: Lease) -> Result<(), TimeError>;

    /// Check if a lease is still valid.
    fn is_lease_valid(&self, lease: &Lease) -> bool;
}

/// Vector clock for causal ordering.
///
/// Vector clocks track causality across multiple nodes.
/// Each node maintains its own counter.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VectorClock {
    /// Mapping from node ID to counter
    pub clocks: std::collections::HashMap<String, u64>,
}

impl VectorClock {
    /// Create a new empty vector clock.
    pub fn new() -> Self {
        Self {
            clocks: std::collections::HashMap::new(),
        }
    }

    /// Increment the counter for a node.
    pub fn increment(&mut self, node_id: &str) {
        *self.clocks.entry(node_id.to_string()).or_insert(0) += 1;
    }

    /// Get the counter for a node.
    pub fn get(&self, node_id: &str) -> u64 {
        self.clocks.get(node_id).copied().unwrap_or(0)
    }

    /// Merge with another vector clock (take max of each counter).
    pub fn merge(&mut self, other: &VectorClock) {
        for (node_id, counter) in &other.clocks {
            let current = self.clocks.entry(node_id.clone()).or_insert(0);
            *current = (*current).max(*counter);
        }
    }

    /// Compare two vector clocks.
    ///
    /// Returns:
    /// - Some(Less) if self happens-before other
    /// - Some(Greater) if other happens-before self
    /// - Some(Equal) if they are identical
    /// - None if they are concurrent (incomparable)
    pub fn partial_compare(&self, other: &VectorClock) -> Option<Ordering> {
        let mut less = false;
        let mut greater = false;

        // Check all keys from both clocks
        let all_keys: std::collections::HashSet<_> = self
            .clocks
            .keys()
            .chain(other.clocks.keys())
            .cloned()
            .collect();

        for key in all_keys {
            let self_val = self.get(&key);
            let other_val = other.get(&key);

            if self_val < other_val {
                less = true;
            }
            if self_val > other_val {
                greater = true;
            }
        }

        match (less, greater) {
            (false, false) => Some(Ordering::Equal),
            (true, false) => Some(Ordering::Less),
            (false, true) => Some(Ordering::Greater),
            (true, true) => None, // Concurrent
        }
    }
}

impl Default for VectorClock {
    fn default() -> Self {
        Self::new()
    }
}

/// Hybrid logical clock (HLC).
///
/// Combines wall-clock time with logical counters to provide
/// timestamps that are both monotonic and roughly aligned
/// with real time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct HybridTimestamp {
    /// Wall-clock component (milliseconds since epoch)
    pub wall_time: u64,
    /// Logical counter for same wall-time
    pub counter: u32,
    /// Node ID (for uniqueness)
    pub node: u32,
}

impl HybridTimestamp {
    /// Create a new hybrid timestamp.
    pub fn new(wall_time: u64, counter: u32, node: u32) -> Self {
        Self {
            wall_time,
            counter,
            node,
        }
    }

    /// Create a timestamp for the current time.
    pub fn now(node: u32) -> Self {
        let wall_time = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        Self::new(wall_time, 0, node)
    }

    /// Update based on a received timestamp.
    ///
    /// Returns a new timestamp that is causally after both
    /// the current time and the received timestamp.
    pub fn update(&self, received: &HybridTimestamp, node: u32) -> Self {
        let current_wall = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        let max_wall = current_wall.max(self.wall_time).max(received.wall_time);

        let counter = if max_wall == self.wall_time && max_wall == received.wall_time {
            self.counter.max(received.counter) + 1
        } else if max_wall == self.wall_time {
            self.counter + 1
        } else if max_wall == received.wall_time {
            received.counter + 1
        } else {
            0
        };

        Self::new(max_wall, counter, node)
    }
}

/// Errors from time operations.
#[derive(Debug, thiserror::Error)]
pub enum TimeError {
    /// Schedule not found
    #[error("Schedule not found: {0}")]
    ScheduleNotFound(String),

    /// Lease not found
    #[error("Lease not found: {0}")]
    LeaseNotFound(String),

    /// Lease expired
    #[error("Lease expired")]
    LeaseExpired,

    /// Lease held by another
    #[error("Resource already leased")]
    ResourceBusy,

    /// Invalid duration
    #[error("Invalid duration")]
    InvalidDuration,

    /// Clock skew detected
    #[error("Clock skew detected: {0}")]
    ClockSkew(String),

    /// Internal error
    #[error("Time error: {0}")]
    Internal(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_clock_increment() {
        let mut vc = VectorClock::new();
        vc.increment("node-1");
        vc.increment("node-1");
        vc.increment("node-2");

        assert_eq!(vc.get("node-1"), 2);
        assert_eq!(vc.get("node-2"), 1);
        assert_eq!(vc.get("node-3"), 0);
    }

    #[test]
    fn test_vector_clock_merge() {
        let mut vc1 = VectorClock::new();
        vc1.increment("node-1");
        vc1.increment("node-1");

        let mut vc2 = VectorClock::new();
        vc2.increment("node-2");
        vc2.increment("node-2");
        vc2.increment("node-2");

        vc1.merge(&vc2);

        assert_eq!(vc1.get("node-1"), 2);
        assert_eq!(vc1.get("node-2"), 3);
    }

    #[test]
    fn test_vector_clock_compare() {
        let mut vc1 = VectorClock::new();
        vc1.increment("node-1");

        let mut vc2 = VectorClock::new();
        vc2.increment("node-1");
        vc2.increment("node-1");

        assert_eq!(vc1.partial_compare(&vc2), Some(Ordering::Less));
        assert_eq!(vc2.partial_compare(&vc1), Some(Ordering::Greater));

        // Concurrent
        let mut vc3 = VectorClock::new();
        vc3.increment("node-2");
        assert_eq!(vc1.partial_compare(&vc3), None);
    }

    #[test]
    fn test_hybrid_timestamp_ordering() {
        let ts1 = HybridTimestamp::new(1000, 0, 1);
        let ts2 = HybridTimestamp::new(1000, 1, 1);
        let ts3 = HybridTimestamp::new(1001, 0, 1);

        assert!(ts1 < ts2);
        assert!(ts2 < ts3);
        assert!(ts1 < ts3);
    }

    #[test]
    fn test_hybrid_timestamp_update() {
        let ts1 = HybridTimestamp::new(1000, 0, 1);
        let ts2 = HybridTimestamp::new(1000, 1, 2);
        let ts3 = ts1.update(&ts2, 1);

        assert!(ts3 > ts1);
        assert!(ts3 > ts2);
    }
}
