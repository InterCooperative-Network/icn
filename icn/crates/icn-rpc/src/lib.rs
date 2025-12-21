//! ICN RPC - JSON-RPC API for daemon communication
#![allow(missing_docs)]
// Prevent panics in production code paths
#![deny(clippy::unwrap_used, clippy::expect_used)]
// Allow unwrap/expect in test code - panics are acceptable for tests
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//!
//! Provides a simple HTTP-based JSON-RPC server for icnctl <-> icnd communication.
//! The RPC server exposes NetworkActor operations for CLI access.
//!
//! ## Authentication
//!
//! RPC endpoints are protected by JWT authentication. Flow:
//! 1. Call `auth.challenge` with DID to get a nonce
//! 2. Sign the nonce with DID keypair
//! 3. Call `auth.verify` with signature to get JWT token
//! 4. Include `Authorization: Bearer <token>` header on subsequent requests
//!
//! See [`auth`] module for details.

pub mod auth;
pub mod client;
pub mod error_codes;
pub mod handler;
pub mod pagination;
pub mod receipt;
pub mod server;
pub mod types;

pub use auth::{required_scope_for_method, scopes, AuthError, RpcAuthManager, RpcTokenClaims};
pub use client::RpcClient;
pub use error_codes::{
    RpcErrorCode, INTERNAL_ERROR, INVALID_PARAMS, INVALID_REQUEST, METHOD_NOT_FOUND, NOT_FOUND,
    PARSE_ERROR, PERMISSION_DENIED, RATE_LIMITED, RESOURCE_NOT_AVAILABLE, VALIDATION_FAILED,
};
pub use pagination::{
    paginate, paginate_owned, PageRequest, PageResponse, ABSOLUTE_MAX_PAGE_SIZE,
    DEFAULT_MAX_PAGE_SIZE,
};
pub use receipt::{Operation, Outcome, Receipt, ReceiptId, ReceiptStore, Resources};
pub use server::RpcServer;
pub use types::{RpcError, RpcRequest, RpcResponse};
