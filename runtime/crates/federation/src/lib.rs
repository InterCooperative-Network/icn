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
pub mod dag_anchor;
pub mod receipt;
pub mod recovery;
pub mod dag_client;

// Re-export core structs
pub use genesis::FederationMetadata;
pub use guardian::{Guardian, GuardianCredential, GuardianQuorumConfig, QuorumType};
pub use dag_anchor::GenesisAnchor;
pub use receipt::{FederationReceipt, MinimalFederationReceipt};

// Re-export receipt verification functions
pub use receipt::verification::{generate_federation_receipt, verify_federation_receipt, verify_minimal_receipt};

// Re-export recovery types and functions
pub use recovery::{RecoveryEvent, RecoveryEventType, FederationKeyRotationEvent, 
                  GuardianSuccessionEvent, DisasterRecoveryAnchor, MetadataUpdateEvent};
pub use recovery::recovery::{create_key_rotation_event, create_guardian_succession_event, 
                           create_disaster_recovery_anchor, create_metadata_update_event,
                           verify_recovery_event, anchor_recovery_event};

// Re-export DAG client types and functions
pub use dag_client::{FederationDagEvent, FederationDagNode, DagClient, InMemoryDagClient, FederationReplayEngine};
pub use dag_client::validation::{validate_event_chain, validate_event};