//! ICN RPC - JSON-RPC API for daemon communication
//!
//! Provides a simple HTTP-based JSON-RPC server for icnctl <-> icnd communication.
//! The RPC server exposes NetworkActor operations for CLI access.

pub mod server;
pub mod client;
pub mod types;

pub use server::RpcServer;
pub use client::RpcClient;
pub use types::{RpcRequest, RpcResponse, RpcError};
