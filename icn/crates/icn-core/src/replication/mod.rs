//! Replication Manager for monitoring and maintaining data durability
//!
//! Phase 17: Storage Hardening & Replication

mod adjuster;
mod manager;

pub use adjuster::{AdjusterConfig, RepairAction, ScopeHealth, ScopedReplicationAdjuster};
pub use manager::{ReplicationConfig, ReplicationHandle, ReplicationManager};
