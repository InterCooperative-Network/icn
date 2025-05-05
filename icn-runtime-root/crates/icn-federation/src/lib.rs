/*!
# ICN Federation

This crate implements federation primitives for the Intercooperative Network (ICN),
including federation establishment, TrustBundle creation, and DAG anchoring
through a quorum-based verification system.

The implementation follows the specification defined in the Federation Genesis Bootstrap
document.
*/

pub mod error;
pub mod genesis;
pub mod quorum;
pub mod dag_anchor;
pub mod receipt;
pub mod recovery;
pub mod dag_client;

// Re-export core structs
pub use genesis::{FederationMetadata, FederationEstablishmentCredential, GenesisTrustBundle};
pub use quorum::{SignerQuorumConfig, QuorumType};
pub use dag_anchor::GenesisAnchor;
pub use receipt::{FederationReceipt, MinimalFederationReceipt};

// Re-export receipt verification functions
pub use receipt::verification::{generate_federation_receipt, verify_federation_receipt, verify_minimal_receipt};

// Re-export recovery types and functions
pub use recovery::{RecoveryEvent, RecoveryEventType, FederationKeyRotationEvent, 
                 SuccessionEvent, DisasterRecoveryAnchor, MetadataUpdateEvent};

// Re-export DAG client types and functions
pub use dag_client::{FederationDagEvent, FederationDagNode, DagClient, InMemoryDagClient, FederationReplayEngine};
pub use dag_client::validation::{validate_event_chain, validate_event};

// Public re-exports
pub use error::{FederationError, FederationResult};