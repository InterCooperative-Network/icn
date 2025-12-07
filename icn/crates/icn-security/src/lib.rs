//! ICN Security - Byzantine fault detection and network security
//!
//! Phase 18: Pre-Pilot Hardening
//!
//! This crate provides Byzantine fault detection, reputation management,
//! and security hardening for the ICN network.

pub mod misbehavior;

pub use misbehavior::{
    MisbehaviorDetector, MisbehaviorStats, MisbehaviorThresholds, ReputationScore,
    TrustPenaltyCallback, Violation, ViolationRecord,
};
