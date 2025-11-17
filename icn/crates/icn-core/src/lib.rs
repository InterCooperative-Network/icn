//! ICN Core - Actor runtime, supervisor, and shared infrastructure

pub mod anti_entropy;
pub mod config;
pub mod events;
pub mod governance;
pub mod identity;
pub mod policy;
pub mod runtime;
pub mod supervisor;
pub mod trust_propagation;

pub use anti_entropy::{spawn_anti_entropy_task, AntiEntropyConfig};
pub use config::Config;
pub use events::{EventBus, EventCallback, SystemEvent};
pub use governance::{GovernanceActor, GovernanceCommand, GovernanceConfigLite, GovernanceHandle};
pub use identity::{IdentityActor, IdentityHandle, IdentityMsg};
pub use policy::{Capability, DefaultPolicySource, PolicySource, TrustPolicy};
pub use runtime::Runtime;
pub use trust_propagation::{AttestationLimits, AttestationRateLimiter, TRUST_ATTESTATIONS_TOPIC};
