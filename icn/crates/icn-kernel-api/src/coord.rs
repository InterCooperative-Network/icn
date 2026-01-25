//! Coordination Primitive
//!
//! Provides consensus groups, CRDTs, and atomic operations.
//!
//! # Design
//!
//! Coordination is always scoped - no global chain, no empire.
//! - **Consensus groups**: Raft-like strong consistency for small scale
//! - **CRDTs**: Eventual consistency for large scale
//! - **Atomic CAS**: Compare-and-swap for single-key operations
//!
//! # Non-Goals
//!
//! - Global consensus (scope is per-namespace, per-dataset)
//! - Cross-scope transactions (apps use sagas)
//! - Arbitrary merge functions (security risk)

use crate::types::{
    CrdtId, Decision, Duration, GroupId, LeaderLease, LogicalTimestamp, Namespace, NodeId,
};

/// Consensus group configuration.
#[derive(Clone, Debug)]
pub struct ConsensusConfig {
    /// Minimum number of nodes for quorum
    pub quorum_size: u32,
    /// Timeout for leader election
    pub election_timeout: Duration,
    /// Heartbeat interval
    pub heartbeat_interval: Duration,
    /// Maximum proposal size in bytes
    pub max_proposal_size: u64,
}

impl Default for ConsensusConfig {
    fn default() -> Self {
        Self {
            quorum_size: 2,
            election_timeout: Duration::from_millis(150),
            heartbeat_interval: Duration::from_millis(50),
            max_proposal_size: 1024 * 1024, // 1 MB
        }
    }
}

/// Supported CRDT types.
///
/// The kernel supports a finite set of CRDT families.
/// Arbitrary merge functions are not allowed (security risk).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CrdtType {
    /// Grow-only counter (increment only)
    GCounter,
    /// Positive-negative counter (increment and decrement)
    PNCounter,
    /// Observed-remove set (add and remove elements)
    OrSet,
    /// Last-writer-wins register
    LwwRegister,
    /// Multi-value register (preserves concurrent writes)
    MvRegister,
    /// Last-writer-wins map
    LwwMap,
}

/// CRDT operation.
#[derive(Clone, Debug)]
pub enum CrdtOp {
    /// Increment a counter
    Increment { amount: i64 },
    /// Decrement a PN-counter
    Decrement { amount: i64 },
    /// Add element to set
    Add { element: Vec<u8> },
    /// Remove element from set
    Remove { element: Vec<u8> },
    /// Set register value
    Set { value: Vec<u8>, timestamp: LogicalTimestamp },
    /// Set map key
    MapSet { key: Vec<u8>, value: Vec<u8>, timestamp: LogicalTimestamp },
    /// Remove map key
    MapRemove { key: Vec<u8>, timestamp: LogicalTimestamp },
}

/// CRDT value (read result).
#[derive(Clone, Debug)]
pub enum CrdtValue {
    /// Counter value
    Counter(i64),
    /// Set of elements
    Set(Vec<Vec<u8>>),
    /// Single register value
    Register(Vec<u8>),
    /// Multiple concurrent register values
    MultiRegister(Vec<Vec<u8>>),
    /// Map of key-value pairs
    Map(Vec<(Vec<u8>, Vec<u8>)>),
}

/// Group status.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupStatus {
    /// Group is forming (waiting for quorum)
    Forming,
    /// Group is healthy and has a leader
    Healthy {
        leader: NodeId,
    },
    /// Group is in election
    Electing,
    /// Group has lost quorum
    Degraded {
        available: u32,
        required: u32,
    },
}

/// Coordination service.
pub trait Coordination: Send + Sync {
    // ===== Consensus Groups =====

    /// Create a consensus group.
    fn create_group(
        &self,
        namespace: &Namespace,
        members: Vec<NodeId>,
        config: ConsensusConfig,
    ) -> Result<GroupId, CoordError>;

    /// Join an existing group.
    fn join_group(&self, group: &GroupId) -> Result<(), CoordError>;

    /// Leave a group.
    fn leave_group(&self, group: &GroupId) -> Result<(), CoordError>;

    /// Propose a value to the group.
    ///
    /// Blocks until the value is committed or rejected.
    fn propose(&self, group: &GroupId, value: &[u8]) -> Result<Decision, CoordError>;

    /// Get the current group status.
    fn group_status(&self, group: &GroupId) -> Result<GroupStatus, CoordError>;

    /// List groups this node is a member of.
    fn list_groups(&self) -> Result<Vec<GroupId>, CoordError>;

    // ===== CRDTs =====

    /// Create a CRDT in a namespace.
    fn create_crdt(
        &self,
        namespace: &Namespace,
        name: &str,
        crdt_type: CrdtType,
    ) -> Result<CrdtId, CoordError>;

    /// Apply an operation to a CRDT.
    fn update_crdt(&self, crdt: &CrdtId, op: CrdtOp) -> Result<(), CoordError>;

    /// Read a CRDT value.
    fn read_crdt(&self, crdt: &CrdtId) -> Result<CrdtValue, CoordError>;

    /// Merge remote CRDT state.
    fn merge_crdt(&self, crdt: &CrdtId, remote_state: &[u8]) -> Result<(), CoordError>;

    /// Export CRDT state for replication.
    fn export_crdt(&self, crdt: &CrdtId) -> Result<Vec<u8>, CoordError>;

    /// Delete a CRDT.
    fn delete_crdt(&self, crdt: &CrdtId) -> Result<(), CoordError>;

    /// List CRDTs in a namespace.
    fn list_crdts(&self, namespace: &Namespace) -> Result<Vec<CrdtId>, CoordError>;

    // ===== Leader Election =====

    /// Participate in leader election.
    fn elect_leader(&self, group: &GroupId, lease_duration: Duration)
        -> Result<LeaderLease, CoordError>;

    /// Check if we hold the leader lease.
    fn is_leader(&self, group: &GroupId) -> Result<bool, CoordError>;

    /// Get the current leader (if any).
    fn get_leader(&self, group: &GroupId) -> Result<Option<NodeId>, CoordError>;

    /// Renew a leader lease.
    fn renew_leader_lease(&self, lease: &LeaderLease) -> Result<LeaderLease, CoordError>;

    /// Resign from leadership.
    fn resign_leadership(&self, group: &GroupId) -> Result<(), CoordError>;
}

/// Errors from coordination operations.
#[derive(Debug, thiserror::Error)]
pub enum CoordError {
    /// Group not found
    #[error("Group not found: {0}")]
    GroupNotFound(String),

    /// Group already exists
    #[error("Group already exists: {0}")]
    GroupAlreadyExists(String),

    /// Not a member of the group
    #[error("Not a member of group: {0}")]
    NotMember(String),

    /// No quorum available
    #[error("No quorum: {available}/{required}")]
    NoQuorum { available: u32, required: u32 },

    /// Proposal rejected
    #[error("Proposal rejected: {0}")]
    ProposalRejected(String),

    /// Proposal too large
    #[error("Proposal too large: {size} bytes (max: {max})")]
    ProposalTooLarge { size: u64, max: u64 },

    /// CRDT not found
    #[error("CRDT not found: {0}")]
    CrdtNotFound(String),

    /// CRDT already exists
    #[error("CRDT already exists: {0}")]
    CrdtAlreadyExists(String),

    /// Invalid CRDT operation
    #[error("Invalid CRDT operation: {0}")]
    InvalidCrdtOp(String),

    /// Not the leader
    #[error("Not the leader")]
    NotLeader,

    /// Leader lease expired
    #[error("Leader lease expired")]
    LeaseExpired,

    /// Timeout
    #[error("Coordination timeout")]
    Timeout,

    /// Internal error
    #[error("Coordination error: {0}")]
    Internal(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_consensus_config_default() {
        let config = ConsensusConfig::default();
        assert_eq!(config.quorum_size, 2);
        assert_eq!(config.max_proposal_size, 1024 * 1024);
    }

    #[test]
    fn test_crdt_type() {
        assert_eq!(CrdtType::GCounter, CrdtType::GCounter);
        assert_ne!(CrdtType::GCounter, CrdtType::PNCounter);
    }

    #[test]
    fn test_crdt_op_variants() {
        let _inc = CrdtOp::Increment { amount: 5 };
        let _add = CrdtOp::Add {
            element: vec![1, 2, 3],
        };
        let _set = CrdtOp::Set {
            value: vec![4, 5],
            timestamp: 12345,
        };
    }

    #[test]
    fn test_crdt_value_variants() {
        let counter = CrdtValue::Counter(42);
        match counter {
            CrdtValue::Counter(v) => assert_eq!(v, 42),
            _ => panic!("Expected Counter"),
        }

        let set = CrdtValue::Set(vec![vec![1], vec![2]]);
        match set {
            CrdtValue::Set(v) => assert_eq!(v.len(), 2),
            _ => panic!("Expected Set"),
        }
    }

    #[test]
    fn test_group_status() {
        let healthy = GroupStatus::Healthy {
            leader: "node-1".to_string(),
        };
        match healthy {
            GroupStatus::Healthy { leader } => assert_eq!(leader, "node-1"),
            _ => panic!("Expected Healthy"),
        }

        let degraded = GroupStatus::Degraded {
            available: 1,
            required: 2,
        };
        match degraded {
            GroupStatus::Degraded {
                available,
                required,
            } => {
                assert_eq!(available, 1);
                assert_eq!(required, 2);
            }
            _ => panic!("Expected Degraded"),
        }
    }
}
