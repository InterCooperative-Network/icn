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
//! │   - LedgerService (TODO)                │
//! │   - GovernanceService (TODO)            │
//! └──────────────┬──────────────────────────┘
//!                │
//!                ▼
//! ┌─────────────────────────────────────────┐
//! │   Daemon Actors                         │
//! │   ComputeActor, Ledger, etc.            │
//! └─────────────────────────────────────────┘
//! ```
//!
//! # Usage
//!
//! Services are initialized with handles to daemon actors and provide
//! high-level operations that handle validation, error mapping, and
//! business logic:
//!
//! ```rust,no_run
//! use icn_api::{ComputeService, ApiContext, SubmitTaskParams};
//!
//! # async fn example(compute_handle: icn_compute::ComputeHandle) -> Result<(), icn_api::ApiError> {
//! let service = ComputeService::new(compute_handle);
//!
//! let ctx = ApiContext {
//!     caller_did: "did:icn:example".to_string(),
//!     coop_id: Some("my-coop".to_string()),
//! };
//!
//! let params = SubmitTaskParams {
//!     task_id: "task-1".to_string(),
//!     code: Some("{}".to_string()),
//!     wasm_bytes: None,
//!     code_type: icn_api::compute::CodeTypeParam::Ccl,
//!     inputs: serde_json::Value::Null,
//!     fuel_limit: 10_000,
//!     priority: icn_api::compute::TaskPriorityParam::Normal,
//!     deadline_ms: None,
//!     payment_rate: None,
//!     payment_currency: None,
//!     coop_id: None,
//!     resource_profile: None,
//! };
//!
//! let task_id = service.submit_task(&ctx, params).await?;
//! # Ok(())
//! # }
//! ```

pub mod compute;
pub mod error;
pub mod scopes;

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
