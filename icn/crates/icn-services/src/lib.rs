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

#[cfg(test)]
mod tests {
    //! Smoke tests verifying facade re-exports work correctly.

    #[test]
    fn facade_exports_api_types() {
        // Verify api module types are accessible via facade
        let _: fn() -> &'static str = || {
            // ApiError exists in api module
            std::any::type_name::<crate::api::ApiError>()
        };
    }

    #[test]
    fn facade_exports_rpc_types() {
        // Verify rpc module types are accessible via facade
        let _: fn() -> &'static str = || {
            // RpcServer exists in rpc module
            std::any::type_name::<crate::rpc::server::RpcServer>()
        };
    }

    #[test]
    fn facade_exports_gateway_types() {
        // Verify gateway module types are accessible via facade
        let _: fn() -> &'static str = || {
            // GatewayServer exists in gateway module
            std::any::type_name::<crate::gateway::server::GatewayServer>()
        };
    }
}
