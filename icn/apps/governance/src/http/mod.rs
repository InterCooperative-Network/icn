//! HTTP layer for the governance app.
//!
//! Provides actix-web route configuration and request/response models for the
//! governance domain. This module is intentionally decoupled from gateway
//! internals — it depends only on `icn-http-kit` for auth, pagination, and
//! error types.

pub mod configure;
pub mod handlers;
pub mod models;
pub mod validation;

pub use configure::{configure, GovernanceContext};
