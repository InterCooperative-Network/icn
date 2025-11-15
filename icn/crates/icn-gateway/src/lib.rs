//! ICN Gateway - REST + WebSocket API for cooperative applications
//!
//! This crate provides a developer-facing HTTP API layer on top of the ICN substrate.
//! Co-ops build apps (web/mobile) that talk to this gateway, which handles:
//!
//! - Authentication (DID-based, capability tokens)
//! - Coop namespace isolation
//! - Ledger operations (balance, payments, history)
//! - Event streaming (WebSocket)
//!
//! This is NOT an app runtime. Apps run externally and call this API.

pub mod api;
pub mod auth;
pub mod coop;
pub mod error;
pub mod ledger_mgr;
pub mod models;
pub mod server;

pub use error::{GatewayError, Result};
pub use server::GatewayServer;