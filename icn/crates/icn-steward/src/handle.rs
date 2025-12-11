//! Steward Handle
//!
//! Provides an async interface for interacting with the StewardActor.

use anyhow::{Context, Result};
use icn_identity::Did;
use tokio::sync::{mpsc, oneshot};

use crate::ceremony::{EnrollmentCeremony, RecoveryCeremony};
use crate::profile::StewardProfile;
use crate::token::EnrollmentToken;
use crate::vui_registry::{LocalCheckResult, VuiRegistration};
use crate::{EnrollmentResult, RecoveryResult};

/// Messages that can be sent to the steward actor
#[derive(Debug)]
pub enum StewardMsg {
    /// Get this steward's profile
    GetProfile(oneshot::Sender<Option<StewardProfile>>),

    /// Check if a VUI is registered (local check)
    CheckVui {
        vui_hash: [u8; 32],
        response: oneshot::Sender<LocalCheckResult>,
    },

    /// Start a new enrollment ceremony
    StartEnrollment {
        vui_commitment: [u8; 32],
        pathway_hash: [u8; 8],
        response: oneshot::Sender<Result<[u8; 32]>>, // Returns ceremony ID
    },

    /// Add a pepper share contribution to an enrollment ceremony
    AddEnrollmentShare {
        ceremony_id: [u8; 32],
        steward_did: Did,
        encrypted_share: Vec<u8>,
        share_commitment: [u8; 32],
        signature: Vec<u8>,
        response: oneshot::Sender<Result<()>>,
    },

    /// Complete an enrollment ceremony
    CompleteEnrollment {
        ceremony_id: [u8; 32],
        blinded_signature: Vec<u8>,
        response: oneshot::Sender<Result<EnrollmentResult>>,
    },

    /// Start a new recovery ceremony
    StartRecovery {
        old_did: Did,
        new_did: Did,
        evidence_hash: [u8; 32],
        anchor_commitment: [u8; 32],
        response: oneshot::Sender<Result<[u8; 32]>>, // Returns ceremony ID
    },

    /// Add an attestation to a recovery ceremony
    AddRecoveryAttestation {
        ceremony_id: [u8; 32],
        steward_did: Did,
        supports: bool,
        reason: Option<String>,
        signature: Vec<u8>,
        response: oneshot::Sender<Result<()>>,
    },

    /// Complete a recovery ceremony
    CompleteRecovery {
        ceremony_id: [u8; 32],
        response: oneshot::Sender<Result<RecoveryResult>>,
    },

    /// Register a VUI after successful enrollment
    RegisterVui {
        vui_hash: [u8; 32],
        registered_by: Did,
        response: oneshot::Sender<Result<()>>,
    },

    /// Get enrollment ceremony status
    GetEnrollmentStatus {
        ceremony_id: [u8; 32],
        response: oneshot::Sender<Option<EnrollmentCeremony>>,
    },

    /// Get recovery ceremony status
    GetRecoveryStatus {
        ceremony_id: [u8; 32],
        response: oneshot::Sender<Option<RecoveryCeremony>>,
    },

    /// Get stats
    GetStats(oneshot::Sender<StewardStats>),

    /// Issue an enrollment token (for blind signature flow)
    IssueToken {
        vui_commitment: [u8; 32],
        blinded_message: Vec<u8>,
        response: oneshot::Sender<Result<EnrollmentToken>>,
    },

    /// Get VUI registration details
    GetVuiRegistration {
        vui_hash: [u8; 32],
        response: oneshot::Sender<Option<VuiRegistration>>,
    },
}

/// Steward operational statistics
#[derive(Debug, Clone, Default)]
pub struct StewardStats {
    /// Number of active enrollment ceremonies
    pub active_enrollments: usize,

    /// Number of active recovery ceremonies
    pub active_recoveries: usize,

    /// Total VUIs in registry
    pub vui_registry_size: usize,

    /// Total enrollments completed
    pub enrollments_completed: u64,

    /// Total recoveries completed
    pub recoveries_completed: u64,

    /// Total tokens issued
    pub tokens_issued: u64,
}

/// Handle to interact with the steward actor
#[derive(Clone)]
pub struct StewardHandle {
    tx: mpsc::Sender<StewardMsg>,
    own_did: Did,
}

impl StewardHandle {
    /// Create a new handle from a message sender
    pub fn new(tx: mpsc::Sender<StewardMsg>, own_did: Did) -> Self {
        Self { tx, own_did }
    }

    /// Get this steward's DID
    pub fn did(&self) -> &Did {
        &self.own_did
    }

    /// Get this steward's profile
    pub async fn get_profile(&self) -> Result<Option<StewardProfile>> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(StewardMsg::GetProfile(tx))
            .await
            .context("Steward actor closed")?;
        rx.await.context("Response channel closed")
    }

    /// Check if a VUI is registered (local check)
    pub async fn check_vui(&self, vui_hash: [u8; 32]) -> Result<LocalCheckResult> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(StewardMsg::CheckVui {
                vui_hash,
                response: tx,
            })
            .await
            .context("Steward actor closed")?;
        rx.await.context("Response channel closed")
    }

    /// Start a new enrollment ceremony
    pub async fn start_enrollment(
        &self,
        vui_commitment: [u8; 32],
        pathway_hash: [u8; 8],
    ) -> Result<[u8; 32]> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(StewardMsg::StartEnrollment {
                vui_commitment,
                pathway_hash,
                response: tx,
            })
            .await
            .context("Steward actor closed")?;
        rx.await.context("Response channel closed")?
    }

    /// Add a pepper share contribution to an enrollment ceremony
    pub async fn add_enrollment_share(
        &self,
        ceremony_id: [u8; 32],
        steward_did: Did,
        encrypted_share: Vec<u8>,
        share_commitment: [u8; 32],
        signature: Vec<u8>,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(StewardMsg::AddEnrollmentShare {
                ceremony_id,
                steward_did,
                encrypted_share,
                share_commitment,
                signature,
                response: tx,
            })
            .await
            .context("Steward actor closed")?;
        rx.await.context("Response channel closed")?
    }

    /// Complete an enrollment ceremony
    pub async fn complete_enrollment(
        &self,
        ceremony_id: [u8; 32],
        blinded_signature: Vec<u8>,
    ) -> Result<EnrollmentResult> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(StewardMsg::CompleteEnrollment {
                ceremony_id,
                blinded_signature,
                response: tx,
            })
            .await
            .context("Steward actor closed")?;
        rx.await.context("Response channel closed")?
    }

    /// Start a new recovery ceremony
    pub async fn start_recovery(
        &self,
        old_did: Did,
        new_did: Did,
        evidence_hash: [u8; 32],
        anchor_commitment: [u8; 32],
    ) -> Result<[u8; 32]> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(StewardMsg::StartRecovery {
                old_did,
                new_did,
                evidence_hash,
                anchor_commitment,
                response: tx,
            })
            .await
            .context("Steward actor closed")?;
        rx.await.context("Response channel closed")?
    }

    /// Add an attestation to a recovery ceremony
    pub async fn add_recovery_attestation(
        &self,
        ceremony_id: [u8; 32],
        steward_did: Did,
        supports: bool,
        reason: Option<String>,
        signature: Vec<u8>,
    ) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(StewardMsg::AddRecoveryAttestation {
                ceremony_id,
                steward_did,
                supports,
                reason,
                signature,
                response: tx,
            })
            .await
            .context("Steward actor closed")?;
        rx.await.context("Response channel closed")?
    }

    /// Complete a recovery ceremony
    pub async fn complete_recovery(&self, ceremony_id: [u8; 32]) -> Result<RecoveryResult> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(StewardMsg::CompleteRecovery {
                ceremony_id,
                response: tx,
            })
            .await
            .context("Steward actor closed")?;
        rx.await.context("Response channel closed")?
    }

    /// Register a VUI after successful enrollment
    pub async fn register_vui(&self, vui_hash: [u8; 32], registered_by: Did) -> Result<()> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(StewardMsg::RegisterVui {
                vui_hash,
                registered_by,
                response: tx,
            })
            .await
            .context("Steward actor closed")?;
        rx.await.context("Response channel closed")?
    }

    /// Get enrollment ceremony status
    pub async fn get_enrollment_status(
        &self,
        ceremony_id: [u8; 32],
    ) -> Result<Option<EnrollmentCeremony>> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(StewardMsg::GetEnrollmentStatus {
                ceremony_id,
                response: tx,
            })
            .await
            .context("Steward actor closed")?;
        rx.await.context("Response channel closed")
    }

    /// Get recovery ceremony status
    pub async fn get_recovery_status(
        &self,
        ceremony_id: [u8; 32],
    ) -> Result<Option<RecoveryCeremony>> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(StewardMsg::GetRecoveryStatus {
                ceremony_id,
                response: tx,
            })
            .await
            .context("Steward actor closed")?;
        rx.await.context("Response channel closed")
    }

    /// Get steward stats
    pub async fn get_stats(&self) -> Result<StewardStats> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(StewardMsg::GetStats(tx))
            .await
            .context("Steward actor closed")?;
        rx.await.context("Response channel closed")
    }

    /// Issue an enrollment token (for blind signature flow)
    pub async fn issue_token(
        &self,
        vui_commitment: [u8; 32],
        blinded_message: Vec<u8>,
    ) -> Result<EnrollmentToken> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(StewardMsg::IssueToken {
                vui_commitment,
                blinded_message,
                response: tx,
            })
            .await
            .context("Steward actor closed")?;
        rx.await.context("Response channel closed")?
    }

    /// Get VUI registration details
    pub async fn get_vui_registration(
        &self,
        vui_hash: [u8; 32],
    ) -> Result<Option<VuiRegistration>> {
        let (tx, rx) = oneshot::channel();
        self.tx
            .send(StewardMsg::GetVuiRegistration {
                vui_hash,
                response: tx,
            })
            .await
            .context("Steward actor closed")?;
        rx.await.context("Response channel closed")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_steward_stats_default() {
        let stats = StewardStats::default();
        assert_eq!(stats.active_enrollments, 0);
        assert_eq!(stats.active_recoveries, 0);
        assert_eq!(stats.vui_registry_size, 0);
    }
}
