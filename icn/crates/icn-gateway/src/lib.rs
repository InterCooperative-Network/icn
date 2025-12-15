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
pub mod commons_mgr;
pub mod commons_store;
pub mod compute_events;
pub mod compute_mgr;
pub mod coop;
pub mod error;
pub mod events;
pub mod federation_mgr;
pub mod governance_mgr;
pub mod identity_mgr;
pub mod invite;
pub mod ledger_events;
pub mod ledger_mgr;
pub mod middleware;
pub mod models;
pub mod notification_listener;
pub mod notifications;
pub mod pagination;
pub mod rate_limit;
pub mod security;
pub mod server;
pub mod trust_mgr;
pub mod validation;
pub mod websocket;

pub use auth::TokenClaims;
pub use commons_mgr::CommonsManager;
pub use commons_store::{CommonsStore, CommonsStoreBackend, InMemoryCommonsStore};
#[cfg(feature = "sled-storage")]
pub use commons_store::SledCommonsStore;
pub use compute_events::{create_forwarding_callback, forward_compute_event};
pub use error::{GatewayError, Result};
pub use events::{BalanceChangeDetail, EventBroadcaster, GatewayEvent, SequencedEvent};
pub use ledger_events::LedgerEventBridge;
pub use pagination::{
    Cursor, Cursored, Direction, PaginatedList, PaginationRequest, PaginationResponse,
    DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE,
};
pub use identity_mgr::{DeviceInfo, IdentityManager, RegisterDeviceRequest};
pub use rate_limit::{
    category_rate_limit_middleware, CategoryRateLimiter, EndpointCategory, RateLimitConfig,
    RateLimiter,
};
pub use server::GatewayServer;
