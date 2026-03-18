//! Steward Actor
//!
//! The StewardActor manages steward operations including:
//! - VUI registry for uniqueness checking
//! - Enrollment ceremony coordination
//! - Recovery ceremony coordination
//! - Token issuance

use anyhow::{bail, Context, Result};
use icn_identity::{Did, KeyPair};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tracing::{debug, info, instrument, warn};

use crate::ceremony::enrollment::{EnrollmentCeremony, ShareContribution};
use crate::ceremony::recovery::{RecoveryAttestation, RecoveryCeremony};
use crate::ceremony::CeremonyId;
use crate::config::StewardConfig;
use crate::handle::{StewardHandle, StewardMsg, StewardStats};
use crate::profile::{StewardProfile, StewardStatus};
use crate::token::EnrollmentToken;
use crate::vui_registry::VuiRegistry;

/// Callback for sending steward gossip messages
pub type SendGossipCallback = Arc<dyn Fn(crate::gossip::StewardMessage) + Send + Sync>;

/// Steward Actor state
pub struct StewardActor {
    /// This steward's DID
    own_did: Did,

    /// This steward's keypair for signing
    keypair: KeyPair,

    /// Steward profile (if active)
    profile: Option<StewardProfile>,

    /// Steward configuration
    config: StewardConfig,

    /// VUI registry for uniqueness checking
    vui_registry: VuiRegistry,

    /// Active enrollment ceremonies
    enrollment_ceremonies: HashMap<CeremonyId, EnrollmentCeremony>,

    /// Active recovery ceremonies
    recovery_ceremonies: HashMap<CeremonyId, RecoveryCeremony>,

    /// Message receiver
    rx: mpsc::Receiver<StewardMsg>,

    /// Gossip send callback (optional)
    send_gossip: Option<SendGossipCallback>,

    /// Stats counters
    enrollments_completed: u64,
    recoveries_completed: u64,
    tokens_issued: u64,
}

impl StewardActor {
    /// Spawn a new steward actor
    ///
    /// Returns a handle for interacting with the actor.
    #[instrument(skip(keypair, shutdown_tx, send_gossip))]
    pub async fn spawn(
        own_did: Did,
        keypair: KeyPair,
        config: StewardConfig,
        shutdown_tx: broadcast::Sender<()>,
        send_gossip: Option<SendGossipCallback>,
    ) -> Result<StewardHandle> {
        let (tx, rx) = mpsc::channel(256);

        info!("Steward actor starting for DID: {}", own_did);

        let actor = StewardActor {
            own_did: own_did.clone(),
            keypair,
            profile: None,
            config: config.clone(),
            vui_registry: VuiRegistry::new(config),
            enrollment_ceremonies: HashMap::new(),
            recovery_ceremonies: HashMap::new(),
            rx,
            send_gossip,
            enrollments_completed: 0,
            recoveries_completed: 0,
            tokens_issued: 0,
        };

        let handle = StewardHandle::new(tx, own_did);

        // Spawn the actor task
        // Note: We move shutdown_tx into the async block to keep the channel alive
        let mut shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move {
            // Keep shutdown_tx alive in this scope
            let _shutdown_tx = shutdown_tx;
            actor.run(&mut shutdown_rx).await;
        });

        Ok(handle)
    }

    /// Spawn with a steward profile (active steward)
    #[instrument(skip(keypair, shutdown_tx, send_gossip, profile))]
    pub async fn spawn_with_profile(
        own_did: Did,
        keypair: KeyPair,
        profile: StewardProfile,
        config: StewardConfig,
        shutdown_tx: broadcast::Sender<()>,
        send_gossip: Option<SendGossipCallback>,
    ) -> Result<StewardHandle> {
        let (tx, rx) = mpsc::channel(256);

        info!("Steward actor starting with profile for DID: {}", own_did);

        let actor = StewardActor {
            own_did: own_did.clone(),
            keypair,
            profile: Some(profile),
            config: config.clone(),
            vui_registry: VuiRegistry::new(config),
            enrollment_ceremonies: HashMap::new(),
            recovery_ceremonies: HashMap::new(),
            rx,
            send_gossip,
            enrollments_completed: 0,
            recoveries_completed: 0,
            tokens_issued: 0,
        };

        let handle = StewardHandle::new(tx, own_did);

        // Note: We move shutdown_tx into the async block to keep the channel alive
        let mut shutdown_rx = shutdown_tx.subscribe();
        tokio::spawn(async move {
            let _shutdown_tx = shutdown_tx;
            actor.run(&mut shutdown_rx).await;
        });

        Ok(handle)
    }

    /// Main actor loop
    async fn run(mut self, shutdown_rx: &mut broadcast::Receiver<()>) {
        info!("Steward actor running for {}", self.own_did);

        loop {
            tokio::select! {
                biased;

                msg = self.rx.recv() => {
                    match msg {
                        Some(m) => {
                            debug!("Steward actor received message");
                            self.handle_message(m).await;
                        }
                        None => {
                            info!("Steward actor: message channel closed");
                            break;
                        }
                    }
                }
                result = shutdown_rx.recv() => {
                    match result {
                        Ok(_) => {
                            info!("Steward actor shutting down via signal");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => {
                            debug!("Steward actor: shutdown sender dropped");
                        }
                        Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                            warn!("Steward actor: shutdown receiver lagged by {}", n);
                            continue; // Don't break on lag, continue processing
                        }
                    }
                    break;
                }
            }
        }

        // Cleanup
        self.cleanup().await;
    }

    /// Handle an incoming message
    #[instrument(skip(self, msg))]
    async fn handle_message(&mut self, msg: StewardMsg) {
        match msg {
            StewardMsg::GetProfile(tx) => {
                let _ = tx.send(self.profile.clone());
            }

            StewardMsg::CheckVui { vui_hash, response } => {
                let result = self.vui_registry.check_local(&vui_hash);
                let _ = response.send(result);
            }

            StewardMsg::StartEnrollment {
                vui_commitment,
                pathway_hash,
                response,
            } => {
                let result = self.start_enrollment(vui_commitment, pathway_hash);
                let _ = response.send(result);
            }

            StewardMsg::AddEnrollmentShare {
                ceremony_id,
                steward_did,
                encrypted_share,
                share_commitment,
                signature,
                response,
            } => {
                let result = self.add_enrollment_share(
                    ceremony_id,
                    steward_did,
                    encrypted_share,
                    share_commitment,
                    signature,
                );
                let _ = response.send(result);
            }

            StewardMsg::CompleteEnrollment {
                ceremony_id,
                blinded_signature,
                response,
            } => {
                let result = self.complete_enrollment(ceremony_id, blinded_signature);
                let _ = response.send(result);
            }

            StewardMsg::StartRecovery {
                old_did,
                new_did,
                evidence_hash,
                anchor_commitment,
                response,
            } => {
                let result =
                    self.start_recovery(old_did, new_did, evidence_hash, anchor_commitment);
                let _ = response.send(result);
            }

            StewardMsg::AddRecoveryAttestation {
                ceremony_id,
                steward_did,
                supports,
                reason,
                signature,
                response,
            } => {
                let result = self.add_recovery_attestation(
                    ceremony_id,
                    steward_did,
                    supports,
                    reason,
                    signature,
                );
                let _ = response.send(result);
            }

            StewardMsg::CompleteRecovery {
                ceremony_id,
                response,
            } => {
                let result = self.complete_recovery(ceremony_id);
                let _ = response.send(result);
            }

            StewardMsg::RegisterVui {
                vui_hash,
                registered_by,
                response,
            } => {
                let result = self.register_vui(vui_hash, registered_by);
                let _ = response.send(result);
            }

            StewardMsg::CheckAndReserveVui {
                vui_hash,
                registered_by,
                response,
            } => {
                let result = self.check_and_reserve_vui(vui_hash, registered_by);
                let _ = response.send(result);
            }

            StewardMsg::GetEnrollmentStatus {
                ceremony_id,
                response,
            } => {
                let ceremony = self.enrollment_ceremonies.get(&ceremony_id).cloned();
                let _ = response.send(ceremony);
            }

            StewardMsg::GetRecoveryStatus {
                ceremony_id,
                response,
            } => {
                let ceremony = self.recovery_ceremonies.get(&ceremony_id).cloned();
                let _ = response.send(ceremony);
            }

            StewardMsg::GetStats(tx) => {
                let stats = StewardStats {
                    active_enrollments: self.enrollment_ceremonies.len(),
                    active_recoveries: self.recovery_ceremonies.len(),
                    vui_registry_size: self.vui_registry.len(),
                    enrollments_completed: self.enrollments_completed,
                    recoveries_completed: self.recoveries_completed,
                    tokens_issued: self.tokens_issued,
                };
                let _ = tx.send(stats);
            }

            StewardMsg::IssueToken {
                vui_commitment,
                blinded_message,
                response,
            } => {
                let result = self.issue_token(vui_commitment, blinded_message);
                let _ = response.send(result);
            }

            StewardMsg::GetVuiRegistration { vui_hash, response } => {
                let registration = self.vui_registry.get_registration(&vui_hash).cloned();
                let _ = response.send(registration);
            }
        }
    }

    /// Start a new enrollment ceremony
    fn start_enrollment(
        &mut self,
        vui_commitment: [u8; 32],
        pathway_hash: [u8; 8],
    ) -> Result<[u8; 32]> {
        // Check concurrent ceremony limit
        if self.enrollment_ceremonies.len() >= self.config.max_concurrent_enrollments {
            bail!("Maximum concurrent enrollments reached");
        }

        // Generate ceremony ID
        let now = icn_time::current_timestamp_secs();
        let ceremony_id = EnrollmentCeremony::generate_ceremony_id(&vui_commitment, now);

        // Check for duplicate
        if self.enrollment_ceremonies.contains_key(&ceremony_id) {
            bail!("Ceremony already exists");
        }

        // Create ceremony
        let ceremony = EnrollmentCeremony::new(
            ceremony_id,
            self.own_did.clone(),
            vui_commitment,
            pathway_hash,
            self.config.vui_threshold,
        );

        debug!(
            "Started enrollment ceremony: {:?}",
            hex::encode(ceremony_id)
        );
        self.enrollment_ceremonies.insert(ceremony_id, ceremony);

        // Broadcast ceremony initiation
        if let Some(ref send_gossip) = self.send_gossip {
            let mut msg = crate::gossip::EnrollmentMessage::ParticipationRequest {
                ceremony_id,
                steward_did: self.own_did.clone(),
                vui_commitment,
                pathway_hash,
                threshold: self.config.vui_threshold,
                timestamp: now,
                signature: Vec::new(),
            };

            // Sign the message
            let signing_bytes = msg.signing_bytes();
            let signature = self.keypair.sign(&signing_bytes).to_vec();

            // Update with signature
            if let crate::gossip::EnrollmentMessage::ParticipationRequest {
                signature: sig, ..
            } = &mut msg
            {
                *sig = signature;
            }

            send_gossip(crate::gossip::StewardMessage::Enrollment(msg));
        }

        Ok(ceremony_id)
    }

    /// Add a pepper share contribution to an enrollment ceremony
    fn add_enrollment_share(
        &mut self,
        ceremony_id: [u8; 32],
        steward_did: Did,
        encrypted_share: Vec<u8>,
        share_commitment: [u8; 32],
        signature: Vec<u8>,
    ) -> Result<()> {
        let ceremony = self
            .enrollment_ceremonies
            .get_mut(&ceremony_id)
            .context("Ceremony not found")?;

        let contribution = ShareContribution {
            steward_did,
            encrypted_share,
            share_commitment,
            signature,
        };

        ceremony.add_share_contribution(contribution)?;

        debug!(
            "Added share contribution to ceremony: {:?}",
            hex::encode(ceremony_id)
        );

        Ok(())
    }

    /// Complete an enrollment ceremony
    fn complete_enrollment(
        &mut self,
        ceremony_id: [u8; 32],
        blinded_signature: Vec<u8>,
    ) -> Result<crate::EnrollmentResult> {
        let ceremony = self
            .enrollment_ceremonies
            .get_mut(&ceremony_id)
            .context("Ceremony not found")?;

        // Advance through phases if needed
        if ceremony.has_enough_contributions() {
            ceremony.begin_vui_computation()?;

            // In real implementation, VUI would be computed here
            // For now, use a placeholder hash
            let vui_hash = sha2::Sha256::digest(ceremony_id).into();
            ceremony.set_vui_hash(vui_hash)?;

            // Check uniqueness
            match self.vui_registry.check_local(&vui_hash) {
                crate::vui_registry::LocalCheckResult::Registered(_) => {
                    ceremony.fail("VUI already registered".to_string());
                    bail!("VUI already registered");
                }
                _ => {
                    ceremony.confirm_uniqueness()?;
                }
            }
        }

        let result = ceremony.complete(blinded_signature)?;

        // Remove completed ceremony
        self.enrollment_ceremonies.remove(&ceremony_id);
        self.enrollments_completed += 1;

        // Broadcast completion
        if let Some(ref send_gossip) = self.send_gossip {
            let msg = crate::gossip::EnrollmentMessage::CeremonyComplete {
                ceremony_id,
                coordinator_did: self.own_did.clone(),
                vui_hash: result.vui_hash,
                participants: result.participants.clone(),
                timestamp: result.completed_at,
                signature: vec![], // Would be signed in production
            };
            send_gossip(crate::gossip::StewardMessage::Enrollment(msg));
        }

        info!(
            "Completed enrollment ceremony: {:?}",
            hex::encode(ceremony_id)
        );

        Ok(result)
    }

    /// Start a new recovery ceremony
    fn start_recovery(
        &mut self,
        old_did: Did,
        new_did: Did,
        evidence_hash: [u8; 32],
        anchor_commitment: [u8; 32],
    ) -> Result<[u8; 32]> {
        // Check concurrent ceremony limit
        if self.recovery_ceremonies.len() >= self.config.max_concurrent_recoveries {
            bail!("Maximum concurrent recoveries reached");
        }

        // Generate ceremony ID
        let now = icn_time::current_timestamp_secs();
        let ceremony_id = RecoveryCeremony::generate_ceremony_id(&old_did, &new_did, now);

        // Check for duplicate
        if self.recovery_ceremonies.contains_key(&ceremony_id) {
            bail!("Ceremony already exists");
        }

        // Create ceremony
        let ceremony = RecoveryCeremony::new(
            ceremony_id,
            self.own_did.clone(),
            old_did.clone(),
            new_did.clone(),
            evidence_hash,
            anchor_commitment,
            self.config.vui_threshold,
        );

        debug!("Started recovery ceremony: {:?}", hex::encode(ceremony_id));
        self.recovery_ceremonies.insert(ceremony_id, ceremony);

        // Broadcast recovery request
        if let Some(ref send_gossip) = self.send_gossip {
            let mut msg = crate::gossip::RecoveryMessage::RecoveryRequest {
                ceremony_id,
                old_did,
                new_did,
                initiator_did: self.own_did.clone(),
                evidence_hash,
                timestamp: now,
                signature: Vec::new(),
            };

            // Sign the message
            let signing_bytes = msg.signing_bytes();
            let signature = self.keypair.sign(&signing_bytes).to_vec();

            // Update with signature
            if let crate::gossip::RecoveryMessage::RecoveryRequest { signature: sig, .. } = &mut msg
            {
                *sig = signature;
            }

            send_gossip(crate::gossip::StewardMessage::Recovery(msg));
        }

        Ok(ceremony_id)
    }

    /// Add an attestation to a recovery ceremony
    fn add_recovery_attestation(
        &mut self,
        ceremony_id: [u8; 32],
        steward_did: Did,
        supports: bool,
        reason: Option<String>,
        signature: Vec<u8>,
    ) -> Result<()> {
        let ceremony = self
            .recovery_ceremonies
            .get_mut(&ceremony_id)
            .context("Ceremony not found")?;

        let attestation = if supports {
            RecoveryAttestation::support(steward_did, signature)
        } else {
            RecoveryAttestation::reject(
                steward_did,
                reason.unwrap_or_else(|| "No reason provided".to_string()),
                signature,
            )
        };

        ceremony.add_attestation(attestation)?;

        debug!(
            "Added attestation to recovery ceremony: {:?}",
            hex::encode(ceremony_id)
        );

        Ok(())
    }

    /// Complete a recovery ceremony
    fn complete_recovery(&mut self, ceremony_id: [u8; 32]) -> Result<crate::RecoveryResult> {
        let ceremony = self
            .recovery_ceremonies
            .get_mut(&ceremony_id)
            .context("Ceremony not found")?;

        // Advance through phases
        ceremony.begin_verification()?;
        ceremony.confirm_evidence()?;
        ceremony.begin_revocation()?;

        let result = ceremony.complete()?;

        // Remove completed ceremony
        self.recovery_ceremonies.remove(&ceremony_id);
        self.recoveries_completed += 1;

        // Broadcast completion
        if let Some(ref send_gossip) = self.send_gossip {
            let msg = crate::gossip::RecoveryMessage::RecoveryComplete {
                ceremony_id,
                old_did: result.old_did.clone(),
                new_did: result.new_did.clone(),
                attesters: result.attesters.clone(),
                timestamp: result.completed_at,
                signature: vec![], // Would be signed in production
            };
            send_gossip(crate::gossip::StewardMessage::Recovery(msg));
        }

        info!(
            "Completed recovery ceremony: {:?}",
            hex::encode(ceremony_id)
        );

        Ok(result)
    }

    /// Register a VUI after successful enrollment
    fn register_vui(&mut self, vui_hash: [u8; 32], registered_by: Did) -> Result<()> {
        self.vui_registry
            .register(vui_hash, registered_by, self.own_did.clone())?;

        // Broadcast new VUI
        if let Some(ref send_gossip) = self.send_gossip {
            let now = icn_time::current_timestamp_secs();
            let msg = crate::gossip::VuiSyncMessage::NewVui {
                from_did: self.own_did.clone(),
                vui_hash,
                timestamp: now,
                signature: vec![], // Would be signed in production
            };
            send_gossip(crate::gossip::StewardMessage::VuiSync(msg));
        }

        debug!("Registered VUI: {:?}", hex::encode(vui_hash));

        Ok(())
    }

    /// Atomically check if VUI is available and reserve it (Issue #397)
    ///
    /// This method is processed sequentially by the actor, ensuring no race
    /// condition between check and reservation on this node.
    ///
    /// Note: For true distributed consistency across multiple gateway nodes,
    /// a distributed consensus mechanism would be needed. This fix addresses
    /// the single-node race condition identified in #397.
    fn check_and_reserve_vui(
        &mut self,
        vui_hash: [u8; 32],
        registered_by: Did,
    ) -> Result<crate::handle::VuiReservationResult> {
        use crate::handle::VuiReservationResult;

        // Check current state
        match self.vui_registry.check_local(&vui_hash) {
            crate::vui_registry::LocalCheckResult::NotRegistered => {
                // VUI is available - register it atomically
                match self
                    .vui_registry
                    .register(vui_hash, registered_by, self.own_did.clone())
                {
                    Ok(()) => {
                        // Broadcast new VUI for distributed sync with message-level signature
                        // for defense-in-depth (transport layer also provides QUIC/TLS auth)
                        if let Some(ref send_gossip) = self.send_gossip {
                            let msg = crate::gossip::VuiSyncMessage::new_vui_signed(
                                self.own_did.clone(),
                                vui_hash,
                                &self.keypair,
                            );
                            send_gossip(crate::gossip::StewardMessage::VuiSync(msg));
                        }

                        debug!("Reserved VUI atomically: {:?}", hex::encode(vui_hash));

                        Ok(VuiReservationResult::Reserved)
                    }
                    Err(e) => {
                        // This shouldn't happen since we just checked, but handle it
                        warn!(
                            "Failed to reserve VUI after check: {:?} - {}",
                            hex::encode(vui_hash),
                            e
                        );
                        bail!("VUI reservation failed: {e}")
                    }
                }
            }
            crate::vui_registry::LocalCheckResult::Registered(registration) => {
                debug!("VUI already registered: {:?}", hex::encode(vui_hash));
                Ok(VuiReservationResult::AlreadyRegistered(registration))
            }
            crate::vui_registry::LocalCheckResult::PossiblyRegistered => {
                debug!(
                    "VUI possibly registered (Bloom positive): {:?}",
                    hex::encode(vui_hash)
                );
                Ok(VuiReservationResult::PossiblyRegistered)
            }
        }
    }

    /// Issue an enrollment token
    fn issue_token(
        &mut self,
        vui_commitment: [u8; 32],
        blinded_message: Vec<u8>,
    ) -> Result<EnrollmentToken> {
        // Check if steward is active
        if let Some(ref profile) = self.profile {
            if !matches!(profile.status, StewardStatus::Active) {
                bail!("Steward is not active");
            }
        }

        // In production, this would:
        // 1. Verify the blinded message format
        // 2. Create a blind signature using the steward's signing key
        // 3. Return the token with the blind signature

        // For now, create a placeholder token
        let token = EnrollmentToken::new(
            blinded_message, // Placeholder - would be blind signature
            self.own_did.clone(),
            vui_commitment,
            Some(self.config.token_validity_secs),
        );

        self.tokens_issued += 1;

        debug!("Issued enrollment token: {:?}", token.token_id_hex());

        Ok(token)
    }

    /// Cleanup on shutdown
    async fn cleanup(&mut self) {
        // Mark incomplete ceremonies as failed
        for (id, ceremony) in self.enrollment_ceremonies.iter_mut() {
            warn!(
                "Enrollment ceremony {} incomplete at shutdown",
                hex::encode(id)
            );
            ceremony.fail("Steward shutdown".to_string());
        }

        for (id, ceremony) in self.recovery_ceremonies.iter_mut() {
            warn!(
                "Recovery ceremony {} incomplete at shutdown",
                hex::encode(id)
            );
            ceremony.fail("Steward shutdown".to_string());
        }

        info!(
            "Steward actor cleanup complete. Stats: enrollments={}, recoveries={}, tokens={}",
            self.enrollments_completed, self.recoveries_completed, self.tokens_issued
        );
    }

    /// Prune expired ceremonies (call periodically)
    pub fn prune_expired_ceremonies(&mut self) -> (usize, usize) {
        let enrollment_before = self.enrollment_ceremonies.len();
        let recovery_before = self.recovery_ceremonies.len();

        self.enrollment_ceremonies
            .retain(|_, c| !c.state.is_expired());
        self.recovery_ceremonies
            .retain(|_, c| !c.state.is_expired());

        let enrollments_pruned = enrollment_before - self.enrollment_ceremonies.len();
        let recoveries_pruned = recovery_before - self.recovery_ceremonies.len();

        if enrollments_pruned > 0 || recoveries_pruned > 0 {
            debug!(
                "Pruned {} expired enrollments, {} expired recoveries",
                enrollments_pruned, recoveries_pruned
            );
        }

        (enrollments_pruned, recoveries_pruned)
    }
}

// Need sha2 for hashing
use sha2::Digest;

#[cfg(test)]
mod tests {
    use super::*;

    fn test_did() -> (Did, KeyPair) {
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let did = keypair.did().clone();
        (did, keypair)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_steward_actor_spawn() {
        let (own_did, keypair) = test_did();
        let config = StewardConfig::default();
        let (shutdown_tx, _shutdown_rx) = broadcast::channel::<()>(16);

        let handle = StewardActor::spawn(own_did.clone(), keypair, config, shutdown_tx, None)
            .await
            .unwrap();

        // Let the actor task start
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert_eq!(handle.did(), &own_did);

        // Get stats
        let stats = handle.get_stats().await.unwrap();
        assert_eq!(stats.active_enrollments, 0);
        assert_eq!(stats.active_recoveries, 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_enrollment_lifecycle() {
        let (own_did, keypair) = test_did();
        let config = StewardConfig::with_threshold(2, 3);
        let (shutdown_tx, _shutdown_rx) = broadcast::channel(16);

        let handle = StewardActor::spawn(own_did.clone(), keypair, config, shutdown_tx, None)
            .await
            .unwrap();

        // Let the actor task start
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Start enrollment
        let vui_commitment = [42u8; 32];
        let pathway_hash = [1u8; 8];
        let ceremony_id = handle
            .start_enrollment(vui_commitment, pathway_hash)
            .await
            .unwrap();

        // Check status
        let ceremony = handle.get_enrollment_status(ceremony_id).await.unwrap();
        assert!(ceremony.is_some());

        // Add contributions
        let (steward1, _) = test_did();
        handle
            .add_enrollment_share(
                ceremony_id,
                steward1,
                vec![1, 2, 3],
                [1u8; 32],
                vec![4, 5, 6],
            )
            .await
            .unwrap();

        let (steward2, _) = test_did();
        handle
            .add_enrollment_share(
                ceremony_id,
                steward2,
                vec![7, 8, 9],
                [2u8; 32],
                vec![10, 11, 12],
            )
            .await
            .unwrap();

        // Complete enrollment
        let result = handle
            .complete_enrollment(ceremony_id, vec![0xAB, 0xCD])
            .await
            .unwrap();

        assert_eq!(result.ceremony_id, ceremony_id);
        assert_eq!(result.participants.len(), 2);

        // Stats should be updated
        let stats = handle.get_stats().await.unwrap();
        assert_eq!(stats.enrollments_completed, 1);
        assert_eq!(stats.active_enrollments, 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_recovery_lifecycle() {
        let (own_did, keypair) = test_did();
        let config = StewardConfig::with_threshold(2, 3);
        let (shutdown_tx, _shutdown_rx) = broadcast::channel(16);

        let handle = StewardActor::spawn(own_did.clone(), keypair, config, shutdown_tx, None)
            .await
            .unwrap();

        // Let the actor task start
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Start recovery
        let (old_did, _) = test_did();
        let (new_did, _) = test_did();
        let evidence_hash = [42u8; 32];
        let anchor_commitment = [43u8; 32];

        let ceremony_id = handle
            .start_recovery(
                old_did.clone(),
                new_did.clone(),
                evidence_hash,
                anchor_commitment,
            )
            .await
            .unwrap();

        // Check status
        let ceremony = handle.get_recovery_status(ceremony_id).await.unwrap();
        assert!(ceremony.is_some());

        // Add attestations
        let (steward1, _) = test_did();
        handle
            .add_recovery_attestation(ceremony_id, steward1, true, None, vec![1, 2, 3])
            .await
            .unwrap();

        let (steward2, _) = test_did();
        handle
            .add_recovery_attestation(ceremony_id, steward2, true, None, vec![4, 5, 6])
            .await
            .unwrap();

        // Complete recovery
        let result = handle.complete_recovery(ceremony_id).await.unwrap();

        assert_eq!(result.ceremony_id, ceremony_id);
        assert_eq!(result.old_did, old_did);
        assert_eq!(result.new_did, new_did);
        assert_eq!(result.attesters.len(), 2);

        // Stats should be updated
        let stats = handle.get_stats().await.unwrap();
        assert_eq!(stats.recoveries_completed, 1);
        assert_eq!(stats.active_recoveries, 0);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_vui_registration() {
        let (own_did, keypair) = test_did();
        let config = StewardConfig::default();
        let (shutdown_tx, _shutdown_rx) = broadcast::channel(16);

        let handle = StewardActor::spawn(own_did.clone(), keypair, config, shutdown_tx, None)
            .await
            .unwrap();

        // Let the actor task start
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Register VUI
        let vui_hash = [42u8; 32];
        let (user_did, _) = test_did();
        handle
            .register_vui(vui_hash, user_did.clone())
            .await
            .unwrap();

        // Check registration
        let registration = handle.get_vui_registration(vui_hash).await.unwrap();
        assert!(registration.is_some());
        let reg = registration.unwrap();
        assert_eq!(reg.vui_hash, vui_hash);
        assert_eq!(reg.registered_by, user_did);

        // Check local uniqueness
        let check = handle.check_vui(vui_hash).await.unwrap();
        assert!(matches!(
            check,
            crate::vui_registry::LocalCheckResult::Registered(_)
        ));

        // Stats should show 1 VUI
        let stats = handle.get_stats().await.unwrap();
        assert_eq!(stats.vui_registry_size, 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_token_issuance() {
        let (own_did, keypair) = test_did();
        let config = StewardConfig::default();
        let (shutdown_tx, _shutdown_rx) = broadcast::channel(16);

        let handle = StewardActor::spawn(own_did.clone(), keypair, config, shutdown_tx, None)
            .await
            .unwrap();

        // Let the actor task start
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        // Issue token
        let vui_commitment = [42u8; 32];
        let blinded_message = vec![1, 2, 3, 4];
        let token = handle
            .issue_token(vui_commitment, blinded_message)
            .await
            .unwrap();

        assert_eq!(token.vui_commitment, vui_commitment);
        assert!(token.is_valid());

        // Stats should show 1 token issued
        let stats = handle.get_stats().await.unwrap();
        assert_eq!(stats.tokens_issued, 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_atomic_vui_reservation() {
        // Test for Issue #397 - atomic check-and-reserve VUI
        let (own_did, keypair) = test_did();
        let config = StewardConfig::default();
        let (shutdown_tx, _shutdown_rx) = broadcast::channel(16);

        let handle = StewardActor::spawn(own_did.clone(), keypair, config, shutdown_tx, None)
            .await
            .unwrap();

        // Let the actor task start
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let vui_hash = [42u8; 32];
        let (user_did, _) = test_did();

        // First reservation should succeed
        let result = handle
            .check_and_reserve_vui(vui_hash, user_did.clone())
            .await
            .unwrap();

        match result {
            crate::handle::VuiReservationResult::Reserved => {
                // Expected - VUI was available
            }
            other => panic!("Expected Reserved, got {other:?}"),
        }

        // Second reservation of same VUI should fail with AlreadyRegistered
        let (another_user, _) = test_did();
        let result2 = handle
            .check_and_reserve_vui(vui_hash, another_user)
            .await
            .unwrap();

        match result2 {
            crate::handle::VuiReservationResult::AlreadyRegistered(reg) => {
                // Expected - VUI was already registered
                assert_eq!(reg.vui_hash, vui_hash);
                assert_eq!(reg.registered_by, user_did);
            }
            other => panic!("Expected AlreadyRegistered, got {other:?}"),
        }

        // Stats should show 1 VUI
        let stats = handle.get_stats().await.unwrap();
        assert_eq!(stats.vui_registry_size, 1);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn test_atomic_reservation_prevents_race() {
        // Verify that concurrent reservation attempts are handled correctly
        let (own_did, keypair) = test_did();
        let config = StewardConfig::default();
        let (shutdown_tx, _shutdown_rx) = broadcast::channel(16);

        let handle = StewardActor::spawn(own_did.clone(), keypair, config, shutdown_tx, None)
            .await
            .unwrap();

        // Let the actor task start
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        let vui_hash = [99u8; 32];

        // Spawn multiple concurrent reservation attempts
        let handle1 = handle.clone();
        let handle2 = handle.clone();
        let handle3 = handle.clone();

        let (user1, _) = test_did();
        let (user2, _) = test_did();
        let (user3, _) = test_did();

        let user1_clone = user1.clone();
        let user2_clone = user2.clone();
        let user3_clone = user3.clone();

        let task1 =
            tokio::spawn(async move { handle1.check_and_reserve_vui(vui_hash, user1_clone).await });

        let task2 =
            tokio::spawn(async move { handle2.check_and_reserve_vui(vui_hash, user2_clone).await });

        let task3 =
            tokio::spawn(async move { handle3.check_and_reserve_vui(vui_hash, user3_clone).await });

        let (r1, r2, r3) = tokio::join!(task1, task2, task3);

        let results = [
            r1.unwrap().unwrap(),
            r2.unwrap().unwrap(),
            r3.unwrap().unwrap(),
        ];

        // Exactly one should be Reserved
        let reserved_count = results
            .iter()
            .filter(|r| matches!(r, crate::handle::VuiReservationResult::Reserved))
            .count();

        // The rest should be AlreadyRegistered
        let already_registered_count = results
            .iter()
            .filter(|r| matches!(r, crate::handle::VuiReservationResult::AlreadyRegistered(_)))
            .count();

        assert_eq!(reserved_count, 1, "Exactly one reservation should succeed");
        assert_eq!(
            already_registered_count, 2,
            "Two reservations should fail with AlreadyRegistered"
        );

        // Stats should show 1 VUI
        let stats = handle.get_stats().await.unwrap();
        assert_eq!(stats.vui_registry_size, 1);
    }
}
