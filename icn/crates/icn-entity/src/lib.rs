//! ICN Entity - Unified Cooperative Entity Model
//!
//! This crate provides a recursive entity abstraction that works at all scales:
//! - **Individuals** (backed by DIDs)
//! - **Cooperatives** (worker, consumer, producer, multi-stakeholder)
//! - **Federations** (cooperatives of cooperatives)
//!
//! # Overview
//!
//! The key insight is that membership is recursive: a cooperative's members
//! can themselves be cooperatives. This enables arbitrary organizational
//! hierarchies while maintaining a simple, unified model.
//!
//! # EntityId Format
//!
//! Entity IDs follow the format `entity:icn:<type>:<identifier>`:
//!
//! - `entity:icn:individual:z5TrA8...` - Individual (wraps DID)
//! - `entity:icn:cooperative:food-coop-2024` - Cooperative
//! - `entity:icn:federation:midwest-fed` - Federation
//!
//! # DID Interoperability
//!
//! EntityIds are designed to be compatible with DIDs:
//! - For individuals: `EntityId::from_did(did)` wraps the DID
//! - For organizations: EntityIds are independent (no keypair)
//! - `EntityId::to_did()` returns `Some(Did)` for individuals, `None` for orgs
//!
//! # Backward Compatibility
//!
//! The [`AccountId`] enum provides backward compatibility during migration:
//!
//! ```ignore
//! enum AccountId {
//!     Did(Did),        // Legacy DID-based accounts
//!     Entity(EntityId), // New entity-based accounts
//! }
//! ```
//!
//! # Example
//!
//! ```
//! use icn_entity::{CooperativeEntity, EntityId, Membership, MembershipRole};
//! use icn_identity::KeyPair;
//!
//! // Create a cooperative
//! let coop = CooperativeEntity::cooperative("food-coop", "Community Food Coop")
//!     .unwrap()
//!     .with_description("A worker-owned grocery store");
//!
//! // Create an individual member
//! let keypair = KeyPair::generate().unwrap();
//! let member = CooperativeEntity::individual(keypair.did(), "Alice");
//!
//! // Create membership
//! let membership = Membership::active(
//!     member.id.clone(),
//!     coop.id.clone(),
//!     MembershipRole::Worker,
//! );
//!
//! assert!(membership.can_vote());
//! assert!(membership.can_propose());
//! ```
//!
//! # Modules
//!
//! - [`entity`] - Core types: `EntityId`, `EntityType`, `CooperativeEntity`
//! - [`labor_exchange`] - Labor exchange types for worker mobility
//! - [`membership`] - Membership types: `Membership`, `MembershipRole`
//! - [`registry`] - Registry trait and in-memory implementation
//! - [`lifecycle`] - Lifecycle events and state transitions
//! - [`error`] - Error types

pub mod entity;
pub mod error;
pub mod labor_exchange;
pub mod lifecycle;
pub mod membership;
pub mod registry;
pub mod sled_registry;

// Re-export main types at crate root
pub use entity::{
    AccountId, AccountReference, CooperativeEntity, EntityId, EntityStatus, EntityType,
};
pub use error::{EntityError, Result};
pub use labor_exchange::{
    AssignmentApprovals, AssignmentId, AssignmentStatus, AssignmentType, Availability,
    CreditRouting, LaborAssignment, LaborNeed, LaborPool, PoolRegistration, RoutingMode,
};
pub use lifecycle::{EntityLifecycle, LifecycleEvent};
pub use membership::{Membership, MembershipCapability, MembershipRole, MembershipStatus};
pub use registry::{EntityRegistry, EntityRegistryHandle, InMemoryRegistry};
pub use sled_registry::SledEntityRegistry;
