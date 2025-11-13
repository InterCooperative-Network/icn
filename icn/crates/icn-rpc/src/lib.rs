//! ICN RPC - JSON-RPC API for daemon communication
//!
//! Provides a simple HTTP-based JSON-RPC server for icnctl <-> icnd communication.
//! The RPC server exposes NetworkActor operations for CLI access.

pub mod client;
pub mod receipt;
pub mod server;
pub mod types;

pub use client::RpcClient;
pub use receipt::{Operation, Outcome, Receipt, ReceiptId, ReceiptStore, Resources};
pub use server::RpcServer;
pub use types::{RpcError, RpcRequest, RpcResponse};
