//! ICN Core - Actor runtime, supervisor, and shared infrastructure
//!
//! # Safety
//! This crate denies panicking on unwrap/expect to prevent runtime crashes.
#![allow(missing_docs)]
#![deny(clippy::unwrap_used, clippy::expect_used)]
// Allow unwrap/expect in test code - panics are acceptable for tests
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

#[cfg(test)]
mod meaning_firewall;

pub mod anti_entropy;
pub mod apps;
pub mod config;
pub mod dead_letter;
pub mod error;
pub mod events;
pub mod identity;
pub mod node;
pub mod policy;
pub mod replication;
pub mod resource_enforcer_actor;
pub mod restart;
pub mod runtime;
pub mod services;
pub mod storage_challenge;
pub mod supervisor;
pub mod trust_propagation;
pub mod upgrade;
pub mod upgrade_actor;

pub use anti_entropy::{spawn_anti_entropy_task, AntiEntropyConfig};
pub use apps::{
    AppBuilder, AppHandle, AppId, AppRuntime, AppStatus, ComputeDispatcher, Event, Manifest,
    Reducer, Request, Response, RuntimeError, Service, StateDelta, StateSnapshot,
};
pub use config::Config;
pub use dead_letter::{DeadLetterQueue, EntryStatus, FailedOperation, FailureType};
pub use error::{CoreError, Result};
pub use events::{EventBus, EventCallback, SystemEvent};
pub use identity::{IdentityActor, IdentityHandle, IdentityMsg};
pub use node::{
    capability_keys, create_node_profile, sense_extended_capabilities, sense_hardware,
    CapabilityValue, ExtendedCapabilities, NodePolicy, NodeProfile, NodeStage, ProfileMessage,
    ResourceCaps, RolePolicy, ServiceRole, TOPIC_NODE_PROFILES,
};
pub use policy::{
    Capability, CapabilityQuota, DefaultPolicySource, PolicySource, TrustPolicy, TrustScoreProvider,
};
pub use replication::{
    AdjusterConfig, RepairAction, ReplicationConfig, ReplicationHandle, ReplicationManager,
    ScopeHealth, ScopedReplicationAdjuster,
};
pub use resource_enforcer_actor::{
    EnforcementResult, EnforcementStats, ResourceAccessEnforcerActor, ResourceEnforcerConfig,
    ResourceEnforcerDeps, ResourceEnforcerHandle, RevocationEvent, RESOURCE_REVOCATIONS_TOPIC,
};
pub use runtime::Runtime;
pub use storage_challenge::{ChallengeScheduler, ChallengeSchedulerHandle};
pub use trust_propagation::{
    AttestationLimits, AttestationRateLimiter, TRUST_ATTESTATIONS_TOPIC, TRUST_REVOCATIONS_TOPIC,
};
pub use upgrade::{
    create_pending_upgrade, PendingUpgrade, UpgradeAdoptionStats, UpgradeCoordinator,
    CURRENT_VERSION,
};
pub use upgrade_actor::{
    UpgradeActor, UpgradeHandle, UpgradeMessage, UpgradeStatus, UPGRADE_TOPIC,
};
