//! Steward manager adapter for gateway
//!
//! This module provides an adapter between the gateway's API
//! and the StewardActor from icn-steward.

use crate::error::{GatewayError, Result};
use icn_identity::Did;
use icn_steward::{
    ActorStats, EnrollmentCeremony, EnrollmentResult, LocalCheckResult, RecoveryCeremony,
    RecoveryResult, StewardHandle, VuiReservationResult,
};

/// Re-export StewardHandle for use by the supervisor
pub type StewardHandleType = StewardHandle;

/// Steward manager that uses StewardActor via handle
pub struct StewardManager {
    handle: StewardHandle,
}

impl StewardManager {
    /// Create a new manager from StewardHandle
    pub fn new(handle: StewardHandle) -> Self {
        Self { handle }
    }

    /// Get this steward's DID
    pub fn steward_did(&self) -> &Did {
        self.handle.did()
    }

    /// Get steward statistics
    pub async fn get_stats(&self) -> Result<ActorStats> {
        self.handle
            .get_stats()
            .await
            .map_err(|e| GatewayError::InternalError(format!("StewardActor error: {e}")))
    }

    // =========================================================================
    // VUI Registry Operations
    // =========================================================================

    /// Check if a VUI is already registered (local check)
    pub async fn check_vui(&self, vui_hash: [u8; 32]) -> Result<LocalCheckResult> {
        self.handle
            .check_vui(vui_hash)
            .await
            .map_err(|e| GatewayError::InternalError(format!("StewardActor error: {e}")))
    }

    /// Register a VUI after successful enrollment
    pub async fn register_vui(&self, vui_hash: [u8; 32], registered_by: Did) -> Result<()> {
        self.handle
            .register_vui(vui_hash, registered_by)
            .await
            .map_err(|e| GatewayError::InternalError(format!("StewardActor error: {e}")))
    }

    /// Atomically check if VUI is available and reserve it (Issue #397)
    ///
    /// This prevents TOCTOU race conditions by combining the check and
    /// reservation into a single atomic operation.
    ///
    /// Use this instead of separate check_vui + register_vui calls when
    /// you need to ensure no race condition between the check and registration.
    pub async fn check_and_reserve_vui(
        &self,
        vui_hash: [u8; 32],
        registered_by: Did,
    ) -> Result<VuiReservationResult> {
        self.handle
            .check_and_reserve_vui(vui_hash, registered_by)
            .await
            .map_err(|e| GatewayError::InternalError(format!("StewardActor error: {e}")))
    }

    // =========================================================================
    // Enrollment Ceremony Operations
    // =========================================================================

    /// Start a new enrollment ceremony
    ///
    /// Returns the ceremony ID (32-byte hash)
    pub async fn start_enrollment(
        &self,
        vui_commitment: [u8; 32],
        pathway_hash: [u8; 8],
    ) -> Result<[u8; 32]> {
        self.handle
            .start_enrollment(vui_commitment, pathway_hash)
            .await
            .map_err(|e| GatewayError::InternalError(format!("StewardActor error: {e}")))
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
        self.handle
            .add_enrollment_share(
                ceremony_id,
                steward_did,
                encrypted_share,
                share_commitment,
                signature,
            )
            .await
            .map_err(|e| GatewayError::InternalError(format!("StewardActor error: {e}")))
    }

    /// Complete an enrollment ceremony
    pub async fn complete_enrollment(
        &self,
        ceremony_id: [u8; 32],
        blinded_signature: Vec<u8>,
    ) -> Result<EnrollmentResult> {
        self.handle
            .complete_enrollment(ceremony_id, blinded_signature)
            .await
            .map_err(|e| GatewayError::InternalError(format!("StewardActor error: {e}")))
    }

    /// Get enrollment ceremony status
    pub async fn get_enrollment_status(
        &self,
        ceremony_id: [u8; 32],
    ) -> Result<Option<EnrollmentCeremony>> {
        self.handle
            .get_enrollment_status(ceremony_id)
            .await
            .map_err(|e| GatewayError::InternalError(format!("StewardActor error: {e}")))
    }

    // =========================================================================
    // Recovery Ceremony Operations
    // =========================================================================

    /// Start a new recovery ceremony
    ///
    /// Returns the ceremony ID (32-byte hash)
    pub async fn start_recovery(
        &self,
        old_did: Did,
        new_did: Did,
        evidence_hash: [u8; 32],
        anchor_commitment: [u8; 32],
    ) -> Result<[u8; 32]> {
        self.handle
            .start_recovery(old_did, new_did, evidence_hash, anchor_commitment)
            .await
            .map_err(|e| GatewayError::InternalError(format!("StewardActor error: {e}")))
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
        self.handle
            .add_recovery_attestation(ceremony_id, steward_did, supports, reason, signature)
            .await
            .map_err(|e| GatewayError::InternalError(format!("StewardActor error: {e}")))
    }

    /// Complete a recovery ceremony
    pub async fn complete_recovery(&self, ceremony_id: [u8; 32]) -> Result<RecoveryResult> {
        self.handle
            .complete_recovery(ceremony_id)
            .await
            .map_err(|e| GatewayError::InternalError(format!("StewardActor error: {e}")))
    }

    /// Get recovery ceremony status
    pub async fn get_recovery_status(
        &self,
        ceremony_id: [u8; 32],
    ) -> Result<Option<RecoveryCeremony>> {
        self.handle
            .get_recovery_status(ceremony_id)
            .await
            .map_err(|e| GatewayError::InternalError(format!("StewardActor error: {e}")))
    }
}
