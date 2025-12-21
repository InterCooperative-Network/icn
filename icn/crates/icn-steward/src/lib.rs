//! ICN Steward Network
// Allow unwrap/expect in test code - panics are acceptable for tests
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]
//!
//! The steward network provides essential services for SDIS (Sovereign Digital Identity System):
//!
//! - **VUI Computation**: Threshold PRF for Verifiable Unique Identifiers
//! - **Enrollment Ceremonies**: Proof-of-Personhood verification
//! - **Token Issuance**: Blind signatures for privacy-preserving enrollment
//! - **Key Recovery**: Social recovery through steward attestations
//! - **VUI Registry**: Distributed uniqueness checking
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────┐
//! │                      Steward Network                             │
//! ├─────────────────────────────────────────────────────────────────┤
//! │                                                                  │
//! │  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐           │
//! │  │  Steward A   │  │  Steward B   │  │  Steward C   │  ...      │
//! │  │              │  │              │  │              │           │
//! │  │ PepperShare  │  │ PepperShare  │  │ PepperShare  │           │
//! │  │     [1]      │  │     [2]      │  │     [3]      │           │
//! │  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘           │
//! │         │                 │                 │                    │
//! │         └─────────────────┼─────────────────┘                    │
//! │                           │                                      │
//! │                    ┌──────▼───────┐                              │
//! │                    │ VUI Registry │                              │
//! │                    │   (Bloom)    │                              │
//! │                    └──────────────┘                              │
//! └─────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Modules
//!
//! - [`actor`] - StewardActor for managing steward operations
//! - [`handle`] - StewardHandle for async interaction with actor
//! - [`profile`] - StewardProfile with status, jurisdiction, and statistics
//! - [`token`] - EnrollmentToken for blind signature-based enrollment
//! - [`vui_registry`] - Distributed VUI uniqueness checking
//! - [`ceremony`] - Enrollment and recovery ceremonies
//! - [`enrollment`] - End-to-end enrollment service
//! - [`recovery`] - End-to-end identity recovery service
//! - [`gossip`] - Steward-specific gossip messages
//! - [`config`] - StewardConfig for runtime configuration

pub mod actor;
pub mod ceremony;
pub mod config;
pub mod enrollment;
pub mod gossip;
pub mod handle;
pub mod profile;
pub mod recovery;
pub mod token;
pub mod vui_registry;

// Re-exports
pub use actor::StewardActor;
pub use ceremony::enrollment::EnrollmentResult;
pub use ceremony::recovery::RecoveryResult;
pub use ceremony::{CeremonyError, CeremonyState, EnrollmentCeremony, RecoveryCeremony};
pub use config::StewardConfig;
pub use gossip::{
    EnrollmentMessage, RecoveryMessage, StewardAnnouncement, StewardMessage, VuiSyncMessage,
};
pub use handle::{StewardHandle, StewardMsg, StewardStats as ActorStats};
pub use profile::{JurisdictionTier, StewardProfile, StewardStats, StewardStatus};
pub use token::{EnrollmentToken, TokenIssuanceRecord, TokenRequest, TokenResponse};
pub use vui_registry::{DistributedCheckResult, LocalCheckResult, VuiRegistry, VuiRegistryError};

// Enrollment service exports
pub use enrollment::{
    BlindedTokenRequest, CeremonyStartResponse, EnrollmentClient, EnrollmentConfig,
    EnrollmentRequest, EnrollmentService, EnrollmentStats,
};

// Recovery service exports
pub use recovery::{
    RecoveryClient, RecoveryConfig, RecoveryEvidence, RecoveryRequest, RecoveryService,
    RecoveryStartResponse, RecoveryStats, RevocationRecord,
};

/// Gossip topics for steward network
pub mod topics {
    /// Topic for steward announcements and status updates
    pub const STEWARD_ANNOUNCE: &str = "steward:announce";

    /// Topic for VUI registry synchronization
    pub const VUI_SYNC: &str = "steward:vui-sync";

    /// Topic for enrollment ceremony coordination
    pub const ENROLLMENT: &str = "steward:enrollment";

    /// Topic for recovery ceremony coordination
    pub const RECOVERY: &str = "steward:recovery";
}
