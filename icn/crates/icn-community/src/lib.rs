//! # ICN Community Management
//!
//! This crate provides community structures that group cooperatives
//! and individuals for shared governance, resources, and mutual support.
#![allow(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
// Allow unwrap/expect in test code - panics are acceptable for tests
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod error;
pub mod lifecycle;
pub mod membership;
pub mod resources;
pub mod store;
pub mod types;

pub use error::{CommunityError, Result};
pub use lifecycle::{CommunityLifecycle, FormationRequest};
pub use membership::{MemberApplication, MembershipManager};
pub use resources::{ResourceAllocation, ResourceManager};
pub use store::{CommunityQuery, CommunityStore};
pub use types::{Community, CommunityStatus, CommunityType, ResourcePool};
