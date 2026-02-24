//! # ICN Authorization
//!
//! Canonical capability graph model for unifying ICN's authorization systems.
//!
//! ## Architecture
//!
//! - `model` -- Domain-agnostic types: subjects, actions, resources, constraints, edges.
//!   Designed to be liftable to `icn-kernel-api` in a future phase.
//! - `graph` -- Builder and query: `CapabilitySource` trait, `GraphBuilder`, `CapabilityGraph`.
//! - `error` -- Error types for validation failures.
//!
//! ## Meaning Firewall
//!
//! This is an **app-layer** crate. It interprets domain semantics (what capabilities mean).
//! The kernel sees only hashes produced by this crate, never the capability model itself.

pub mod error;
pub mod graph;
pub mod model;

pub use error::AuthzError;
pub use graph::{CapabilitySource, GraphBuilder};
pub use model::{
    Action, CapabilityEdge, CapabilityGraph, Constraint, Decision, EdgeSource, ResourceId,
    ResourceKind, SubjectId,
};
