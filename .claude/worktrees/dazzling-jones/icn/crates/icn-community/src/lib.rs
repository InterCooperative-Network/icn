//! # ICN Community Management
//!
//! This crate provides community structures that group cooperatives
//! and individuals for shared governance, resources, and mutual support.
//!
//! Communities serve as the **civic engine** of the ICN, complementing
//! cooperatives (economic engine) and federations (coordination layer).
//!
//! ## Architecture
//!
//! ```text
//! CommunityHandle ──► CommunityActor ──► CommunityStore
//!                           │
//!                           ▼
//!                    GossipActor (community:updates)
//! ```
//!
//! ## Usage
//!
//! ```ignore
//! let handle = CommunityHandle::new(actor_tx);
//! let community = handle.create(
//!     None,
//!     "Downtown Makers".to_string(),
//!     CommunityType::Geographic,
//!     founder_did.to_string(),
//!     MemberType::Individual(founder_did.to_string()),
//!     "charter content".to_string(),
//! ).await?;
//! ```
#![allow(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
// Allow unwrap/expect in test code - panics are acceptable for tests
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod actor;
pub mod error;
pub mod handle;
pub mod lifecycle;
pub mod membership;
pub mod resources;
pub mod store;
pub mod types;

pub use actor::{CommunityActor, CommunityMessage, COMMUNITY_TOPIC};
pub use error::{CommunityError, Result};
pub use handle::CommunityHandle;
pub use lifecycle::{CommunityLifecycle, FormationRequest};
pub use membership::{MemberApplication, MembershipManager};
pub use resources::{ResourceAllocation, ResourceManager};
pub use store::{CommunityQuery, CommunityStore};
pub use types::{Community, CommunityStatus, CommunityType, MemberType, ResourcePool};
