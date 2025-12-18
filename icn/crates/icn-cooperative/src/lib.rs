//! # ICN Cooperative Management
//!
//! This crate provides comprehensive lifecycle management for cooperatives,
//! including formation, membership, governance, and dissolution.

pub mod error;
pub mod lifecycle;
pub mod membership;
pub mod store;
pub mod types;

pub use error::{CooperativeError, Result};
pub use lifecycle::{CooperativeLifecycle, DissolutionRequest, FormationRequest};
pub use membership::{MembershipAction, MembershipApplication, MembershipManager};
pub use store::{CooperativeQuery, CooperativeStore};
pub use types::{Cooperative, CooperativeStatus, CooperativeType, MembershipTier};
