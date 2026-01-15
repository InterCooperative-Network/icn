//! Agreement Manager
//!
//! Business logic for managing inter-cooperative agreements including lifecycle
//! state transitions, signature collection, and event notifications.

use super::store::AgreementStoreOps;
use super::types::{
    Agreement, AgreementId, AgreementParty, AgreementSignature, AgreementStatus, AgreementTerms,
    AgreementType, Amendment, AmendmentChange, AmendmentStatus, PartyRole, TerminationReason,
};
use crate::error::{FederationError, Result};
use crate::types::current_timestamp;
use icn_identity::{Did, KeyPair};
use icn_obs::metrics::agreement as metrics;
use std::sync::Arc;
use tracing::{debug, info};

/// Callback type for agreement events
pub type AgreementEventCallback = Box<dyn Fn(AgreementEvent) + Send + Sync>;

/// Events emitted by the agreement manager
#[derive(Debug, Clone)]
pub enum AgreementEvent {
    /// A new agreement was created
    Created {
        agreement_id: AgreementId,
        proposer: Did,
    },

    /// An agreement was proposed (moved from Draft to Proposed)
    Proposed {
        agreement_id: AgreementId,
        awaiting_signatures: Vec<Did>,
    },

    /// A party signed an agreement
    Signed {
        agreement_id: AgreementId,
        signer: Did,
        remaining_signers: Vec<Did>,
    },

    /// An agreement became active (all parties signed)
    Activated {
        agreement_id: AgreementId,
        activated_at: u64,
    },

    /// An agreement was suspended
    Suspended {
        agreement_id: AgreementId,
        suspended_by: Did,
        reason: String,
    },

    /// An agreement was resumed
    Resumed {
        agreement_id: AgreementId,
        resumed_by: Did,
    },

    /// An agreement was terminated
    Terminated {
        agreement_id: AgreementId,
        reason: TerminationReason,
    },

    /// An amendment was proposed
    AmendmentProposed {
        agreement_id: AgreementId,
        amendment_id: String,
        proposed_by: Did,
    },

    /// An amendment was signed
    AmendmentSigned {
        agreement_id: AgreementId,
        amendment_id: String,
        signer: Did,
    },

    /// An amendment was ratified
    AmendmentRatified {
        agreement_id: AgreementId,
        amendment_id: String,
        new_version: u32,
    },
}

/// Manager for inter-cooperative agreements
pub struct AgreementManager<S: AgreementStoreOps> {
    store: Arc<S>,
    own_coop_id: String,
    own_did: Did,
    keypair: Option<Arc<KeyPair>>,
    event_callback: Option<AgreementEventCallback>,
}

impl<S: AgreementStoreOps> AgreementManager<S> {
    /// Create a new agreement manager
    pub fn new(store: Arc<S>, own_coop_id: String, own_did: Did) -> Self {
        Self {
            store,
            own_coop_id,
            own_did,
            keypair: None,
            event_callback: None,
        }
    }

    /// Set the keypair for signing operations
    pub fn with_keypair(mut self, keypair: Arc<KeyPair>) -> Self {
        self.keypair = Some(keypair);
        self
    }

    /// Set the event callback
    pub fn with_event_callback(mut self, callback: AgreementEventCallback) -> Self {
        self.event_callback = Some(callback);
        self
    }

    /// Emit an event
    fn emit_event(&self, event: AgreementEvent) {
        debug!("Agreement event: {:?}", event);
        if let Some(ref callback) = self.event_callback {
            callback(event);
        }
    }

    /// Create a new draft agreement
    ///
    /// The caller is automatically added as the proposer.
    pub fn create_draft(
        &self,
        title: impl Into<String>,
        description: impl Into<String>,
        agreement_type: AgreementType,
    ) -> Result<Agreement> {
        let agreement = Agreement::new(title, description, agreement_type).with_party(
            self.own_did.clone(),
            &self.own_coop_id,
            PartyRole::Proposer,
        );

        self.store.store_agreement(&agreement)?;

        // Record metrics
        metrics::agreements_created_inc(agreement.agreement_type.type_name());

        self.emit_event(AgreementEvent::Created {
            agreement_id: agreement.id.clone(),
            proposer: self.own_did.clone(),
        });

        info!("Created draft agreement {}", agreement.id);
        Ok(agreement)
    }

    /// Add a party to a draft agreement
    pub fn add_party(
        &self,
        agreement_id: &AgreementId,
        party: AgreementParty,
    ) -> Result<Agreement> {
        let mut agreement = self.get_agreement_required(agreement_id)?;

        if !agreement.status.is_draft() {
            return Err(FederationError::InvalidState(
                "Can only add parties to draft agreements".to_string(),
            ));
        }

        // Check for duplicate
        if agreement.parties.iter().any(|p| p.did == party.did) {
            return Err(FederationError::InvalidState(
                "Party already exists in agreement".to_string(),
            ));
        }

        agreement.parties.push(party);
        agreement.updated_at = current_timestamp();
        self.store.store_agreement(&agreement)?;

        Ok(agreement)
    }

    /// Set terms on a draft agreement
    pub fn set_terms(
        &self,
        agreement_id: &AgreementId,
        terms: AgreementTerms,
    ) -> Result<Agreement> {
        let mut agreement = self.get_agreement_required(agreement_id)?;

        if !agreement.status.is_draft() {
            return Err(FederationError::InvalidState(
                "Can only set terms on draft agreements".to_string(),
            ));
        }

        agreement.terms = terms;
        agreement.updated_at = current_timestamp();
        self.store.store_agreement(&agreement)?;

        Ok(agreement)
    }

    /// Propose a draft agreement (transition to Proposed status)
    pub fn propose(&self, agreement_id: &AgreementId) -> Result<Agreement> {
        let mut agreement = self.get_agreement_required(agreement_id)?;

        // Verify caller is the proposer
        if let Some(proposer) = agreement.proposer() {
            if proposer.did != self.own_did {
                return Err(FederationError::Unauthorized(
                    "Only the proposer can propose an agreement".to_string(),
                ));
            }
        } else {
            return Err(FederationError::InvalidState(
                "Agreement has no proposer".to_string(),
            ));
        }

        agreement
            .propose()
            .map_err(|e| FederationError::InvalidState(e.to_string()))?;

        self.store.store_agreement(&agreement)?;

        let awaiting = agreement.pending_signers();
        self.emit_event(AgreementEvent::Proposed {
            agreement_id: agreement.id.clone(),
            awaiting_signatures: awaiting.clone(),
        });

        info!(
            "Proposed agreement {} (awaiting {} signatures)",
            agreement.id,
            awaiting.len()
        );
        Ok(agreement)
    }

    /// Sign an agreement
    ///
    /// If all required parties have signed, the agreement is automatically activated.
    pub fn sign(&self, agreement_id: &AgreementId) -> Result<Agreement> {
        let mut agreement = self.get_agreement_required(agreement_id)?;

        // Verify agreement is in Proposed status
        if !agreement.status.is_proposed() {
            return Err(FederationError::InvalidState(
                "Can only sign proposed agreements".to_string(),
            ));
        }

        // Verify caller is a party who needs to sign
        let is_signer = agreement
            .parties
            .iter()
            .any(|p| p.did == self.own_did && p.role.requires_signature());

        if !is_signer {
            return Err(FederationError::Unauthorized(
                "Caller is not a party requiring signature".to_string(),
            ));
        }

        // Check if already signed
        if agreement.has_signed(&self.own_did) {
            return Err(FederationError::InvalidState(
                "Already signed this agreement".to_string(),
            ));
        }

        // Sign the agreement
        let keypair = self.keypair.as_ref().ok_or_else(|| {
            FederationError::Config("No keypair configured for signing".to_string())
        })?;

        agreement.sign(keypair, &self.own_coop_id);
        let remaining = agreement.pending_signers();

        // Record signature metric
        metrics::agreement_signatures_inc();

        self.emit_event(AgreementEvent::Signed {
            agreement_id: agreement.id.clone(),
            signer: self.own_did.clone(),
            remaining_signers: remaining.clone(),
        });

        // Check if we should auto-activate
        if agreement.try_auto_activate() {
            if let AgreementStatus::Active { activated_at, .. } = &agreement.status {
                // Record activation metric
                metrics::agreements_activated_inc();

                self.emit_event(AgreementEvent::Activated {
                    agreement_id: agreement.id.clone(),
                    activated_at: *activated_at,
                });
                info!("Agreement {} activated (all parties signed)", agreement.id);
            }
        }

        self.store.store_agreement(&agreement)?;

        debug!(
            "Signed agreement {} ({} signers remaining)",
            agreement.id,
            remaining.len()
        );
        Ok(agreement)
    }

    /// Record a signature received from another party (e.g., via gossip)
    pub fn record_signature(
        &self,
        agreement_id: &AgreementId,
        signature: AgreementSignature,
    ) -> Result<Agreement> {
        let mut agreement = self.get_agreement_required(agreement_id)?;

        // Verify agreement is in Proposed status
        if !agreement.status.is_proposed() {
            return Err(FederationError::InvalidState(
                "Can only add signatures to proposed agreements".to_string(),
            ));
        }

        // Verify signer is a party
        let is_party = agreement.parties.iter().any(|p| p.did == signature.signer);
        if !is_party {
            return Err(FederationError::InvalidState(
                "Signer is not a party to this agreement".to_string(),
            ));
        }

        // Don't add duplicate signatures
        if agreement.has_signed(&signature.signer) {
            return Ok(agreement);
        }

        // Verify signature version matches
        if signature.version_signed != agreement.version {
            return Err(FederationError::InvalidState(format!(
                "Signature version {} doesn't match agreement version {}",
                signature.version_signed, agreement.version
            )));
        }

        // Add the signature
        agreement.signatures.push(signature.clone());
        let remaining = agreement.pending_signers();

        // Record signature metric
        metrics::agreement_signatures_inc();

        self.emit_event(AgreementEvent::Signed {
            agreement_id: agreement.id.clone(),
            signer: signature.signer.clone(),
            remaining_signers: remaining.clone(),
        });

        // Check for auto-activation
        if agreement.try_auto_activate() {
            if let AgreementStatus::Active { activated_at, .. } = &agreement.status {
                // Record activation metric
                metrics::agreements_activated_inc();

                self.emit_event(AgreementEvent::Activated {
                    agreement_id: agreement.id.clone(),
                    activated_at: *activated_at,
                });
                info!("Agreement {} activated (all parties signed)", agreement.id);
            }
        }

        self.store.store_agreement(&agreement)?;
        Ok(agreement)
    }

    /// Suspend an active agreement
    pub fn suspend(
        &self,
        agreement_id: &AgreementId,
        reason: impl Into<String>,
    ) -> Result<Agreement> {
        let mut agreement = self.get_agreement_required(agreement_id)?;

        // Verify caller is a party
        if !agreement.parties.iter().any(|p| p.did == self.own_did) {
            return Err(FederationError::Unauthorized(
                "Only parties can suspend an agreement".to_string(),
            ));
        }

        let reason_str = reason.into();
        agreement
            .suspend(&reason_str, self.own_did.clone())
            .map_err(|e| FederationError::InvalidState(e.to_string()))?;

        self.store.store_agreement(&agreement)?;

        // Record suspension metric
        metrics::agreements_suspended_inc();

        self.emit_event(AgreementEvent::Suspended {
            agreement_id: agreement.id.clone(),
            suspended_by: self.own_did.clone(),
            reason: reason_str,
        });

        info!("Suspended agreement {}", agreement.id);
        Ok(agreement)
    }

    /// Resume a suspended agreement
    pub fn resume(&self, agreement_id: &AgreementId) -> Result<Agreement> {
        let mut agreement = self.get_agreement_required(agreement_id)?;

        agreement
            .resume(&self.own_did)
            .map_err(|e| FederationError::InvalidState(e.to_string()))?;

        self.store.store_agreement(&agreement)?;

        // Record resume metric
        metrics::agreements_resumed_inc();

        self.emit_event(AgreementEvent::Resumed {
            agreement_id: agreement.id.clone(),
            resumed_by: self.own_did.clone(),
        });

        info!("Resumed agreement {}", agreement.id);
        Ok(agreement)
    }

    /// Terminate an agreement
    pub fn terminate(
        &self,
        agreement_id: &AgreementId,
        reason: TerminationReason,
    ) -> Result<Agreement> {
        let mut agreement = self.get_agreement_required(agreement_id)?;

        // Verify caller is a party
        if !agreement.parties.iter().any(|p| p.did == self.own_did) {
            return Err(FederationError::Unauthorized(
                "Only parties can terminate an agreement".to_string(),
            ));
        }

        agreement
            .terminate(reason.clone())
            .map_err(|e| FederationError::InvalidState(e.to_string()))?;

        self.store.store_agreement(&agreement)?;

        // Record termination metric with reason
        metrics::agreements_terminated_inc(reason.as_str());

        self.emit_event(AgreementEvent::Terminated {
            agreement_id: agreement.id.clone(),
            reason,
        });

        info!("Terminated agreement {}", agreement.id);
        Ok(agreement)
    }

    /// Reject a proposed agreement (return to Draft)
    pub fn reject(&self, agreement_id: &AgreementId) -> Result<Agreement> {
        let mut agreement = self.get_agreement_required(agreement_id)?;

        // Verify caller is a party
        if !agreement.parties.iter().any(|p| p.did == self.own_did) {
            return Err(FederationError::Unauthorized(
                "Only parties can reject an agreement".to_string(),
            ));
        }

        agreement
            .reject()
            .map_err(|e| FederationError::InvalidState(e.to_string()))?;

        self.store.store_agreement(&agreement)?;

        info!("Rejected agreement {} (returned to draft)", agreement.id);
        Ok(agreement)
    }

    /// Delete a draft agreement
    pub fn delete_draft(&self, agreement_id: &AgreementId) -> Result<()> {
        let agreement = self.get_agreement_required(agreement_id)?;

        if !agreement.status.is_draft() {
            return Err(FederationError::InvalidState(
                "Can only delete draft agreements".to_string(),
            ));
        }

        // Verify caller is the proposer
        if let Some(proposer) = agreement.proposer() {
            if proposer.did != self.own_did {
                return Err(FederationError::Unauthorized(
                    "Only the proposer can delete a draft".to_string(),
                ));
            }
        }

        self.store.delete_agreement(agreement_id)?;
        info!("Deleted draft agreement {}", agreement_id);
        Ok(())
    }

    // === Query Methods ===

    /// Get an agreement by ID
    pub fn get_agreement(&self, id: &AgreementId) -> Result<Option<Agreement>> {
        self.store.get_agreement(id)
    }

    /// Get an agreement by ID, returning an error if not found
    fn get_agreement_required(&self, id: &AgreementId) -> Result<Agreement> {
        self.store
            .get_agreement(id)?
            .ok_or_else(|| FederationError::NotFound(format!("Agreement not found: {id}")))
    }

    /// List all agreements
    pub fn list_agreements(&self) -> Result<Vec<Agreement>> {
        self.store.list_agreements()
    }

    /// List agreements where this cooperative is a party
    pub fn list_my_agreements(&self) -> Result<Vec<Agreement>> {
        self.store.list_agreements_for_party(&self.own_did)
    }

    /// List agreements by status
    pub fn list_by_status(&self, status_type: &str) -> Result<Vec<Agreement>> {
        self.store.list_agreements_by_status(status_type)
    }

    /// Get agreements pending signature from this cooperative
    pub fn pending_signatures(&self) -> Result<Vec<Agreement>> {
        let agreements = self.list_my_agreements()?;
        Ok(agreements
            .into_iter()
            .filter(|a| a.status.is_proposed() && !a.has_signed(&self.own_did))
            .collect())
    }

    // === Amendment Methods ===

    /// Propose an amendment to an active agreement
    pub fn propose_amendment(
        &self,
        agreement_id: &AgreementId,
        description: impl Into<String>,
        changes: Vec<AmendmentChange>,
    ) -> Result<Amendment> {
        let agreement = self.get_agreement_required(agreement_id)?;

        // Must be active
        if !agreement.status.is_active() {
            return Err(FederationError::InvalidState(
                "Can only amend active agreements".to_string(),
            ));
        }

        // Verify caller is a party
        if !agreement.parties.iter().any(|p| p.did == self.own_did) {
            return Err(FederationError::Unauthorized(
                "Only parties can propose amendments".to_string(),
            ));
        }

        let mut amendment = Amendment::new(agreement_id.clone(), description, self.own_did.clone());

        for change in changes {
            amendment.changes.push(change);
        }

        self.store.store_amendment(&amendment)?;

        // Record amendment proposed metric
        metrics::amendments_proposed_inc();

        self.emit_event(AgreementEvent::AmendmentProposed {
            agreement_id: agreement_id.clone(),
            amendment_id: amendment.id.clone(),
            proposed_by: self.own_did.clone(),
        });

        info!(
            "Proposed amendment {} for agreement {}",
            amendment.id, agreement_id
        );
        Ok(amendment)
    }

    /// Sign an amendment
    pub fn sign_amendment(
        &self,
        agreement_id: &AgreementId,
        amendment_id: &str,
    ) -> Result<Amendment> {
        let agreement = self.get_agreement_required(agreement_id)?;
        let mut amendments = self.store.get_amendments(agreement_id)?;

        let amendment = amendments
            .iter_mut()
            .find(|a| a.id == amendment_id)
            .ok_or_else(|| {
                FederationError::NotFound(format!("Amendment not found: {amendment_id}"))
            })?;

        if amendment.status != AmendmentStatus::Proposed {
            return Err(FederationError::InvalidState(
                "Can only sign proposed amendments".to_string(),
            ));
        }

        // Verify caller is a party
        if !agreement.parties.iter().any(|p| p.did == self.own_did) {
            return Err(FederationError::Unauthorized(
                "Only parties can sign amendments".to_string(),
            ));
        }

        // Check if already signed
        if amendment
            .signatures
            .iter()
            .any(|s| s.signer == self.own_did)
        {
            return Err(FederationError::InvalidState(
                "Already signed this amendment".to_string(),
            ));
        }

        // Add signature (simplified - in production would verify signature properly)
        let sig = AgreementSignature::new(
            self.own_did.clone(),
            &self.own_coop_id,
            vec![], // Signature bytes - simplified
            agreement.version,
        );
        amendment.add_signature(sig);

        self.emit_event(AgreementEvent::AmendmentSigned {
            agreement_id: agreement_id.clone(),
            amendment_id: amendment_id.to_string(),
            signer: self.own_did.clone(),
        });

        // Check if all parties have signed
        let required_signers: Vec<_> = agreement
            .parties
            .iter()
            .filter(|p| p.role.requires_signature())
            .map(|p| &p.did)
            .collect();

        if amendment.is_fully_signed(
            &required_signers
                .iter()
                .map(|d| (*d).clone())
                .collect::<Vec<_>>(),
        ) {
            amendment.status = AmendmentStatus::Ratified;

            // Apply the amendment to the agreement
            let mut updated_agreement = agreement.clone();
            let amendment_clone = amendment.clone();
            updated_agreement.apply_amendment(amendment_clone)?;
            self.store.store_agreement(&updated_agreement)?;

            // Record amendment ratification metric
            metrics::amendments_ratified_inc();

            self.emit_event(AgreementEvent::AmendmentRatified {
                agreement_id: agreement_id.clone(),
                amendment_id: amendment_id.to_string(),
                new_version: updated_agreement.version,
            });

            info!(
                "Ratified amendment {} for agreement {} (new version: {})",
                amendment_id, agreement_id, updated_agreement.version
            );
        }

        self.store.store_amendment(amendment)?;
        Ok(amendment.clone())
    }

    /// Get amendments for an agreement
    pub fn get_amendments(&self, agreement_id: &AgreementId) -> Result<Vec<Amendment>> {
        self.store.get_amendments(agreement_id)
    }

    /// Check for expired agreements and mark them as terminated
    pub fn check_expirations(&self) -> Result<Vec<AgreementId>> {
        let now = current_timestamp();
        let active = self.list_by_status("active")?;
        let mut expired = Vec::new();

        for agreement in active {
            if agreement.status.is_expired(now) {
                let mut updated = agreement;
                if updated.terminate(TerminationReason::Expired).is_ok() {
                    self.store.store_agreement(&updated)?;

                    // Record expiration metric
                    metrics::agreements_terminated_inc("expired");

                    self.emit_event(AgreementEvent::Terminated {
                        agreement_id: updated.id.clone(),
                        reason: TerminationReason::Expired,
                    });

                    expired.push(updated.id);
                }
            }
        }

        if !expired.is_empty() {
            info!("Marked {} agreements as expired", expired.len());
        }

        Ok(expired)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agreement::{InMemoryAgreementStore, TradeItem};
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn create_manager() -> (AgreementManager<InMemoryAgreementStore>, Arc<KeyPair>) {
        let store = Arc::new(InMemoryAgreementStore::new());
        let keypair = Arc::new(KeyPair::generate().unwrap());
        let did = keypair.did().clone();
        let manager = AgreementManager::new(store, "test-coop".to_string(), did)
            .with_keypair(keypair.clone());
        (manager, keypair)
    }

    #[test]
    fn test_create_and_propose() {
        let (manager, _) = create_manager();

        // Create draft
        let agreement = manager
            .create_draft(
                "Test Agreement",
                "Testing",
                AgreementType::Trade {
                    items: vec![TradeItem::new("item", 10, "units", 100, "USD")],
                    currency: "USD".to_string(),
                },
            )
            .unwrap();

        assert!(agreement.status.is_draft());
        assert_eq!(agreement.parties.len(), 1);

        // Add counterparty
        let counterparty_did = KeyPair::generate().unwrap().did().clone();
        let agreement = manager
            .add_party(
                &agreement.id,
                AgreementParty::new(counterparty_did, "other-coop", PartyRole::Counterparty),
            )
            .unwrap();

        assert_eq!(agreement.parties.len(), 2);

        // Propose
        let agreement = manager.propose(&agreement.id).unwrap();
        assert!(agreement.status.is_proposed());
    }

    #[test]
    fn test_signing_workflow() {
        let (manager1, _kp1) = create_manager();

        // Manager 2 for the counterparty
        let store2 = Arc::new(InMemoryAgreementStore::new());
        let kp2 = Arc::new(KeyPair::generate().unwrap());
        let manager2 =
            AgreementManager::new(store2.clone(), "other-coop".to_string(), kp2.did().clone())
                .with_keypair(kp2.clone());

        // Manager 1 creates and proposes
        let agreement = manager1
            .create_draft(
                "Trade Agreement",
                "For testing",
                AgreementType::Credit {
                    credit_limit: 5000,
                    interest_rate_bps: 300,
                    currency: "USD".to_string(),
                },
            )
            .unwrap();

        let agreement = manager1
            .add_party(
                &agreement.id,
                AgreementParty::new(kp2.did().clone(), "other-coop", PartyRole::Counterparty),
            )
            .unwrap();

        let agreement = manager1.propose(&agreement.id).unwrap();

        // Copy to manager2's store
        store2.store_agreement(&agreement).unwrap();

        // Manager 1 signs
        let agreement = manager1.sign(&agreement.id).unwrap();
        assert!(!agreement.status.is_active()); // Not yet, need counterparty

        // Update manager2's copy
        store2.store_agreement(&agreement).unwrap();

        // Manager 2 signs - should activate
        let agreement = manager2.sign(&agreement.id).unwrap();
        assert!(agreement.status.is_active());
    }

    #[test]
    fn test_suspend_and_resume() {
        let (manager, _kp) = create_manager();

        // Create agreement with a separate counterparty
        let counterparty_kp = KeyPair::generate().unwrap();
        let agreement = manager
            .create_draft(
                "Test",
                "Test",
                AgreementType::Custom {
                    agreement_type_name: "test".to_string(),
                    terms_json: "{}".to_string(),
                },
            )
            .unwrap();

        let agreement = manager
            .add_party(
                &agreement.id,
                AgreementParty::new(
                    counterparty_kp.did().clone(),
                    "other-coop",
                    PartyRole::Counterparty,
                ),
            )
            .unwrap();

        let agreement = manager.propose(&agreement.id).unwrap();
        let mut agreement = manager.sign(&agreement.id).unwrap();

        // Add counterparty signature to activate
        agreement.signatures.push(AgreementSignature::new(
            counterparty_kp.did().clone(),
            "other-coop",
            vec![1, 2, 3], // Mock signature
            1,
        ));
        // Force activate by setting status directly (skip verification for test)
        agreement.status = AgreementStatus::Active {
            activated_at: current_timestamp(),
            expires_at: None,
        };
        manager.store.store_agreement(&agreement).unwrap();

        // Suspend
        let agreement = manager
            .suspend(&agreement.id, "Testing suspension")
            .unwrap();
        assert!(agreement.status.is_suspended());

        // Resume
        let agreement = manager.resume(&agreement.id).unwrap();
        assert!(agreement.status.is_active());
    }

    #[test]
    fn test_event_callback() {
        let store = Arc::new(InMemoryAgreementStore::new());
        let keypair = Arc::new(KeyPair::generate().unwrap());
        let did = keypair.did().clone();

        let event_count = Arc::new(AtomicUsize::new(0));
        let count_clone = event_count.clone();

        let manager = AgreementManager::new(store, "test-coop".to_string(), did)
            .with_keypair(keypair)
            .with_event_callback(Box::new(move |_event| {
                count_clone.fetch_add(1, Ordering::SeqCst);
            }));

        // Create should emit Created event
        let agreement = manager
            .create_draft(
                "Test",
                "Test",
                AgreementType::Custom {
                    agreement_type_name: "test".to_string(),
                    terms_json: "{}".to_string(),
                },
            )
            .unwrap();

        assert_eq!(event_count.load(Ordering::SeqCst), 1);

        // Add party and propose
        let kp2 = KeyPair::generate().unwrap();
        manager
            .add_party(
                &agreement.id,
                AgreementParty::new(kp2.did().clone(), "other", PartyRole::Counterparty),
            )
            .unwrap();

        manager.propose(&agreement.id).unwrap();
        assert_eq!(event_count.load(Ordering::SeqCst), 2); // Proposed event
    }

    #[test]
    fn test_pending_signatures() {
        let (manager, _kp) = create_manager();

        // Create and propose an agreement
        let agreement = manager
            .create_draft(
                "Test",
                "Test",
                AgreementType::Credit {
                    credit_limit: 1000,
                    interest_rate_bps: 100,
                    currency: "USD".to_string(),
                },
            )
            .unwrap();

        let kp2 = KeyPair::generate().unwrap();
        manager
            .add_party(
                &agreement.id,
                AgreementParty::new(kp2.did().clone(), "other", PartyRole::Counterparty),
            )
            .unwrap();

        manager.propose(&agreement.id).unwrap();

        // Should show as pending
        let pending = manager.pending_signatures().unwrap();
        assert_eq!(pending.len(), 1);

        // Sign it
        manager.sign(&agreement.id).unwrap();

        // No longer pending for us
        let pending = manager.pending_signatures().unwrap();
        assert_eq!(pending.len(), 0);
    }
}
