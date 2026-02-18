//! Shared API service layer for ICN
//!
//! This crate provides reusable service implementations that can be shared
//! between different API frontends (RPC, Gateway, etc.).
//!
//! # Architecture
//!
//! The icn-api crate serves as a shared service layer that sits between
//! transport-specific API adapters (icn-rpc, icn-gateway) and the core
//! daemon actors. This enables:
//!
//! - **Single source of truth** for business logic
//! - **Consistent behavior** across all API transports
//! - **Reduced maintenance** by avoiding duplicate implementations
//! - **Testable services** independent of transport concerns
//!
//! # Structure
//!
//! ```text
//! ┌─────────────────────────────────────────┐
//! │   Transport Adapters                    │
//! │   icn-rpc (JSON-RPC)                    │
//! │   icn-gateway (REST/WebSocket)          │
//! └──────────────┬──────────────────────────┘
//!                │
//!                ▼
//! ┌─────────────────────────────────────────┐
//! │   Shared Service Layer (icn-api)        │
//! │   - ComputeService                      │
//! │   - LedgerService                       │
//! │   - GovernanceService                   │
//! └──────────────┬──────────────────────────┘
//!                │
//!                ▼
//! ┌─────────────────────────────────────────┐
//! │   Daemon Actors                         │
//! │   ComputeActor, Ledger, Governance      │
//! └─────────────────────────────────────────┘
//! ```

pub mod compute;
pub mod error;
pub mod governance;
pub mod ledger;
pub mod scopes;

pub use compute::{ComputeService, SubmitTaskParams};
pub use error::ApiError;
pub use governance::GovernanceService;
pub use ledger::{AccountBalance, LedgerAccountDeltaView, LedgerEntryView, LedgerService};

/// API context passed to service methods
#[derive(Debug, Clone)]
pub struct ApiContext {
    /// Authenticated caller's DID
    pub caller_did: String,
    /// Caller's cooperative ID (if applicable)
    pub coop_id: Option<String>,
}
