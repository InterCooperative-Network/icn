//! ICN Core - Actor runtime, supervisor, and shared infrastructure

pub mod anti_entropy;
pub mod config;
pub mod dead_letter;
pub mod events;
pub mod governance;
pub mod identity;
pub mod node;
pub mod policy;
pub mod replication;
pub mod runtime;
pub mod supervisor;
pub mod trust_propagation;
pub mod upgrade;
pub mod upgrade_actor;

pub use anti_entropy::{spawn_anti_entropy_task, AntiEntropyConfig};
pub use config::Config;
pub use dead_letter::{DeadLetterQueue, EntryStatus, FailedOperation, FailureType};
pub use events::{EventBus, EventCallback, SystemEvent};
pub use governance::{GovernanceActor, GovernanceCommand, GovernanceConfigLite, GovernanceHandle};
pub use identity::{IdentityActor, IdentityHandle, IdentityMsg};
pub use node::{
    capability_keys, create_node_profile, sense_extended_capabilities, sense_hardware,
    CapabilityValue, ExtendedCapabilities, NodePolicy, NodeProfile, NodeStage, ProfileMessage,
    ResourceCaps, RolePolicy, ServiceRole, TOPIC_NODE_PROFILES,
};
pub use policy::{Capability, DefaultPolicySource, PolicySource, TrustPolicy};
pub use replication::{ReplicationConfig, ReplicationHandle, ReplicationManager};
pub use runtime::Runtime;
pub use trust_propagation::{AttestationLimits, AttestationRateLimiter, TRUST_ATTESTATIONS_TOPIC};
pub use upgrade::{
    proposal_to_pending_upgrade, PendingUpgrade, UpgradeAdoptionStats, UpgradeCoordinator,
    CURRENT_VERSION,
};
pub use upgrade_actor::{
    UpgradeActor, UpgradeHandle, UpgradeMessage, UpgradeStatus, UPGRADE_TOPIC,
};
