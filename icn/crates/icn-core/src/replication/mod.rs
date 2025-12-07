//! Replication Manager for monitoring and maintaining data durability
//!
//! Phase 17: Storage Hardening & Replication

mod manager;

pub use manager::{ReplicationConfig, ReplicationHandle, ReplicationManager};
