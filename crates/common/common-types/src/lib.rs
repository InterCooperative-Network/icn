//! Common type definitions for the ICN project
//!
//! This crate provides shared type definitions that are used across multiple
//! components in the ICN ecosystem, including runtime, wallet, agoranet, and mesh.

pub mod error;
pub mod identity;
pub mod crypto;
pub mod network;

/// Re-export common types for convenience
pub use error::Error;
pub use identity::Identity;
pub use network::NetworkAddress; 