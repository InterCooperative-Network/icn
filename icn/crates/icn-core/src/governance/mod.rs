//! Governance actor for decentralized decision-making
//!
//! This module provides a production-ready governance actor that:
//! - Subscribes to governance gossip messages
//! - Persists domains, proposals, and votes to the node's store
//! - Handles proposal lifecycle (create → open → vote → close)
//! - Evaluates outcomes via governance profiles
//! - Broadcasts state changes to peers

pub mod actor;

pub use actor::{GovernanceActor, GovernanceCommand, GovernanceConfigLite, GovernanceHandle};
