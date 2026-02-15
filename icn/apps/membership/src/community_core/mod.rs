//! Community core module - community management and civic engine
//!
//! This module contains the community types, actor, lifecycle, membership,
//! resources, and storage that were originally in the `icn-community` crate.
#![allow(missing_docs)]

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
