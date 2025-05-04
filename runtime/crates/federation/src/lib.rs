/*!
# ICN Federation

This crate implements federation primitives for the Intercooperative Network (ICN),
including Guardian role management, federation establishment, TrustBundle creation,
and DAG anchoring.

The implementation follows the specification defined in the Federation Genesis Bootstrap
document.
*/

pub mod error;
pub mod genesis;
pub mod guardian;

// Re-export core structs
pub use genesis::FederationMetadata;
pub use guardian::{Guardian, GuardianCredential, GuardianQuorumConfig, QuorumType};