//! Entity core module - unified cooperative entity model
//!
//! This module contains the core entity types, registry, and lifecycle
//! management that were originally in the `icn-entity` crate.

pub mod actor;
pub mod entity;
pub mod error;
pub mod handle;
pub mod labor_exchange;
pub mod lifecycle;
pub mod membership;
pub mod registry;
pub mod sled_registry;
