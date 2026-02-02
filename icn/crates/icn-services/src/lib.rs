//! ICN Services - Unified services layer
//!
//! This crate provides a unified interface to ICN's service layers.
//! It re-exports `icn-api`, `icn-rpc`, and `icn-gateway` under a single namespace.
//!
//! # Modules
//!
//! - `api`: Shared API service layer
//! - `rpc`: gRPC API server
//! - `gateway`: REST + WebSocket gateway
//!
//! # Example
//!
//! ```rust,ignore
//! use icn_services::{gateway, rpc};
//! ```

#![deny(clippy::unwrap_used, clippy::expect_used)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

/// API service layer re-exports
pub use icn_api as api;

/// RPC server re-exports
pub use icn_rpc as rpc;

/// Gateway re-exports
pub use icn_gateway as gateway;
