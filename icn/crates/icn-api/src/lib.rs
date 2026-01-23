//! Shared API service layer for ICN
//!
//! This crate provides reusable service implementations that can be shared
//! between different API frontends (RPC, Gateway, etc.).

pub mod compute;
pub mod error;

pub use compute::{ComputeService, SubmitTaskParams};
pub use error::ApiError;

/// API context passed to service methods
#[derive(Debug, Clone)]
pub struct ApiContext {
    /// Authenticated caller's DID
    pub caller_did: String,
    /// Caller's cooperative ID (if applicable)
    pub coop_id: Option<String>,
}
