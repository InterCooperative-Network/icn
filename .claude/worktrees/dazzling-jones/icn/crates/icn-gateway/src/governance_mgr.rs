//! Governance Manager — thin re-export from `icn-governance-actor`.
//!
//! The implementation moved to `apps/governance` as part of Phase 4 of the
//! kernel/app separation arc. Gateway code that previously imported from here
//! continues to work via these re-exports.

pub use icn_governance_actor::manager::{GovernanceHandle, GovernanceManager};
