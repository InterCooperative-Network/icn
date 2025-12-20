//! ICN RPC - JSON-RPC API for daemon communication
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
pub mod handler;
pub mod pagination;
pub mod receipt;
pub mod server;
pub mod types;

pub use auth::{required_scope_for_method, scopes, AuthError, RpcAuthManager, RpcTokenClaims};
pub use client::RpcClient;
pub use pagination::{
    paginate, paginate_owned, PageRequest, PageResponse, ABSOLUTE_MAX_PAGE_SIZE,
    DEFAULT_MAX_PAGE_SIZE,
};
pub use receipt::{Operation, Outcome, Receipt, ReceiptId, ReceiptStore, Resources};
pub use server::RpcServer;
pub use types::{RpcError, RpcRequest, RpcResponse};
