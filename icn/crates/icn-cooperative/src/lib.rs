//! # ICN Cooperative Management
//!
//! This crate provides comprehensive lifecycle management for cooperatives,
//! including formation, membership, governance, and dissolution.

pub mod types;
pub mod lifecycle;
pub mod membership;
pub mod store;
pub mod error;

pub use types::{Cooperative, CooperativeType, MembershipTier, CooperativeStatus};
pub use lifecycle::{CooperativeLifecycle, FormationRequest, DissolutionRequest};
pub use membership::{MembershipManager, MembershipApplication, MembershipAction};
pub use store::{CooperativeStore, CooperativeQuery};
pub use error::{CooperativeError, Result};
