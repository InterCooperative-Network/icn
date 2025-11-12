//! ICN Core - Actor runtime, supervisor, and shared infrastructure

pub mod anti_entropy;
pub mod config;
pub mod identity;
pub mod runtime;
pub mod supervisor;
pub mod trust_propagation;

pub use anti_entropy::{spawn_anti_entropy_task, AntiEntropyConfig};
pub use config::Config;
pub use identity::{IdentityActor, IdentityHandle, IdentityMsg};
pub use runtime::Runtime;
pub use trust_propagation::{AttestationLimits, AttestationRateLimiter, TRUST_ATTESTATIONS_TOPIC};
