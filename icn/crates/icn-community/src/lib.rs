//! # ICN Community Management
//!
//! This crate provides community structures that group cooperatives
//! and individuals for shared governance, resources, and mutual support.

pub mod types;
pub mod lifecycle;
pub mod membership;
pub mod resources;
pub mod store;
pub mod error;

pub use types::{Community, CommunityType, CommunityStatus, ResourcePool};
pub use lifecycle::{CommunityLifecycle, FormationRequest};
pub use membership::{MembershipManager, MemberApplication};
pub use resources::{ResourceManager, ResourceAllocation};
pub use store::{CommunityStore, CommunityQuery};
pub use error::{CommunityError, Result};
