//! ICN Security - Byzantine fault detection and network security
// Allow unwrap/expect in test code - panics are acceptable for tests
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//!
//! Phase 18: Pre-Pilot Hardening
//!
//! This crate provides Byzantine fault detection, reputation management,
//! and security hardening for the ICN network.

pub mod misbehavior;

pub use misbehavior::{
    MisbehaviorDetector, MisbehaviorStats, MisbehaviorThresholds, ReputationScore,
    StorageFailureReason, TrustPenaltyCallback, Violation, ViolationRecord,
};
