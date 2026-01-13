//! ICN Gateway - REST + WebSocket API for cooperative applications
//!
//! # Safety
//! This crate denies panicking on unwrap/expect to prevent runtime crashes.
#![allow(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
// Allow unwrap/expect in test code - panics are acceptable for tests
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
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

// Initialize i18n with locale files from the locales directory.
// TODO(i18n): Integrate t!() macro into error.rs for user-facing API error messages.
// The locale file (locales/en.yaml) is ready; integration requires updating GatewayError
// to use translation keys instead of hardcoded strings. See issue tracking this work.
rust_i18n::i18n!("locales", fallback = "en");

pub mod api;
pub mod audit;
pub mod auth;
pub mod commons_mgr;
pub mod commons_store;
pub mod community_mgr;
pub mod compute_events;
pub mod compute_mgr;
pub mod coop;
pub mod email_client;
pub mod entity_audit;
pub mod entity_mgr;
pub mod error;
pub mod events;
pub mod fcm_client;
pub mod federation_mgr;
pub mod governance_mgr;
pub mod identity_mgr;
pub mod invite;
pub mod ledger_events;
pub mod ledger_mgr;
pub mod logging;
pub mod middleware;
pub mod models;
pub mod notification_listener;
pub mod notification_processor;
pub mod notification_queue;
pub mod notification_triggers;
pub mod notifications;
pub mod openapi;
pub mod pagination;
pub mod rate_limit;
pub mod security;
pub mod server;
pub mod session;
pub mod steward_mgr;
pub mod treasury_mgr;
pub mod trust_mgr;
pub mod validation;
pub mod websocket;

pub use auth::TokenClaims;
pub use commons_mgr::CommonsManager;
pub use commons_store::SledCommonsStore;
pub use commons_store::{CommonsStore, CommonsStoreBackend, InMemoryCommonsStore};
pub use community_mgr::CommunityManager;
pub use compute_events::{create_forwarding_callback, forward_compute_event};
pub use entity_audit::{EntityAuditManager, PruneStats};
pub use entity_mgr::EntityHandle;
pub use error::{GatewayError, Result};
pub use events::{
    BalanceChangeDetail, EventBroadcaster, GatewayEvent, SequencedEvent, WebSocketConfig,
};
pub use identity_mgr::{DeviceInfo, IdentityManager, RegisterDeviceRequest};
pub use ledger_events::LedgerEventBridge;
pub use notification_queue::{
    DeliveryStatus, NotificationChannel, NotificationPriority, NotificationQueue,
    NotificationStatsSnapshot, NotificationType, QueuedNotification,
};
pub use pagination::{
    create_list_response, Cursor, Cursored, Direction, ListPagination, ListQuery, ListResponse,
    PaginatedList, PaginationRequest, PaginationResponse, ResponseMeta, SortField,
    DEFAULT_PAGE_SIZE, MAX_PAGE_SIZE,
};
pub use rate_limit::{
    category_rate_limit_middleware, CategoryRateLimiter, EndpointCategory, RateLimitConfig,
    RateLimiter, VelocityLimitConfig, VelocityLimiter,
};
pub use server::GatewayServer;
pub use steward_mgr::{StewardHandleType, StewardManager};
pub use treasury_mgr::{GatewayTreasuryManager, LedgerHandle, TreasuryHandle};
pub use websocket::ServerMessage;
