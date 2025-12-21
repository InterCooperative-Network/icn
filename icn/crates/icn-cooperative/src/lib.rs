//! # ICN Cooperative Management
//!
//! This crate provides comprehensive lifecycle management for cooperatives,
//! including formation, membership, governance, and dissolution.
#![allow(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
// Allow unwrap/expect in test code - panics are acceptable for tests
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

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
