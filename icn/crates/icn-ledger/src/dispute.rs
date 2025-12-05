//! Dispute resolution system for ledger entries
//!
//! Provides mechanisms for:
//! - Filing disputes against journal entries
//! - Mediating and resolving disputes
//! - Tracking dispute history
//! - Write-offs and debt forgiveness

use crate::types::{ContentHash, Dispute, DisputeOutcome, DisputeStatus};
use anyhow::{Context, Result};
use icn_identity::Did;
use icn_store::Store;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::info;

/// Key prefix for disputes in storage
const DISPUTE_PREFIX: &str = "ledger:dispute:";

/// Dispute manager for tracking and resolving entry disputes
pub struct DisputeManager {
    /// Storage backend
    store: Arc<dyn Store>,

    /// In-memory cache of active disputes
    active_disputes: HashMap<ContentHash, Dispute>,
}

impl DisputeManager {
    /// Create a new dispute manager
    pub fn new(store: Arc<dyn Store>) -> Result<Self> {
        let mut manager = DisputeManager {
            store,
            active_disputes: HashMap::new(),
        };

        // Load active disputes from storage
        manager.load_active_disputes()?;

        Ok(manager)
    }

    /// File a dispute against a journal entry
    ///
    /// # Arguments
    /// * `entry_hash` - Hash of the entry being disputed
    /// * `filed_by` - DID of the person filing the dispute
    /// * `reason` - Explanation of why the entry is being disputed
    /// * `filed_at` - Unix timestamp when dispute was filed
    ///
    /// # Returns
    /// The created dispute record
    pub fn file_dispute(
        &mut self,
        entry_hash: ContentHash,
        filed_by: Did,
        reason: String,
        filed_at: u64,
    ) -> Result<Dispute> {
        // Check if dispute already exists
        if self.active_disputes.contains_key(&entry_hash) {
            anyhow::bail!("Dispute already exists for entry {entry_hash}");
        }

        let dispute = Dispute {
            entry_hash: entry_hash.clone(),
            filed_by: filed_by.clone(),
            reason: reason.clone(),
            filed_at,
            status: DisputeStatus::Contested {
                filed_by: filed_by.clone(),
                reason: reason.clone(),
                filed_at,
            },
            evidence: Vec::new(),
            mediator: None,
        };

        // Store dispute
        self.save_dispute(&dispute)?;

        // Add to active disputes cache
        self.active_disputes
            .insert(entry_hash.clone(), dispute.clone());

        info!("Dispute filed against entry {} by {}", entry_hash, filed_by);

        Ok(dispute)
    }

    /// Add evidence to an existing dispute
    pub fn add_evidence(&mut self, entry_hash: &ContentHash, evidence: String) -> Result<()> {
        let dispute = self
            .active_disputes
            .get_mut(entry_hash)
            .context("Dispute not found")?;

        dispute.evidence.push(evidence);

        // Clone for saving to avoid borrow checker issues
        let dispute_clone = dispute.clone();
        self.save_dispute(&dispute_clone)?;

        Ok(())
    }

    /// Assign a mediator to a dispute
    pub fn assign_mediator(&mut self, entry_hash: &ContentHash, mediator: Did) -> Result<()> {
        let dispute = self
            .active_disputes
            .get_mut(entry_hash)
            .context("Dispute not found")?;

        dispute.mediator = Some(mediator.clone());

        // Clone for saving to avoid borrow checker issues
        let dispute_clone = dispute.clone();
        self.save_dispute(&dispute_clone)?;

        info!("Mediator {} assigned to dispute {}", mediator, entry_hash);

        Ok(())
    }

    /// Resolve a dispute
    ///
    /// # Arguments
    /// * `entry_hash` - Hash of the disputed entry
    /// * `mediator` - DID of the mediator making the decision
    /// * `outcome` - The resolution outcome
    /// * `resolved_at` - Unix timestamp of resolution
    pub fn resolve_dispute(
        &mut self,
        entry_hash: &ContentHash,
        mediator: Did,
        outcome: DisputeOutcome,
        resolved_at: u64,
    ) -> Result<()> {
        let mut dispute = self
            .active_disputes
            .remove(entry_hash)
            .context("Dispute not found")?;

        // Update dispute status
        dispute.status = DisputeStatus::Resolved {
            mediator: mediator.clone(),
            outcome: outcome.clone(),
            resolved_at,
        };

        // Save to historical disputes
        self.save_dispute(&dispute)?;

        info!(
            "Dispute {} resolved by {}: {:?}",
            entry_hash, mediator, outcome
        );

        Ok(())
    }

    /// Get a dispute by entry hash
    pub fn get_dispute(&self, entry_hash: &ContentHash) -> Option<&Dispute> {
        self.active_disputes.get(entry_hash)
    }

    /// Get all active disputes
    pub fn get_active_disputes(&self) -> Vec<&Dispute> {
        self.active_disputes.values().collect()
    }

    /// Get all disputes filed by a specific DID
    pub fn get_disputes_by_filer(&self, filer: &Did) -> Vec<&Dispute> {
        self.active_disputes
            .values()
            .filter(|d| &d.filed_by == filer)
            .collect()
    }

    /// Check if an entry has an active dispute
    pub fn has_active_dispute(&self, entry_hash: &ContentHash) -> bool {
        self.active_disputes.contains_key(entry_hash)
    }

    /// Get total count of active disputes
    pub fn active_dispute_count(&self) -> usize {
        self.active_disputes.len()
    }

    /// Escalate a dispute to governance for community vote
    ///
    /// This is used when:
    /// - The dispute value exceeds mediator authority
    /// - There is a mediator conflict of interest
    /// - The disputed party rejects the mediator's decision
    /// - The dispute involves community-wide policy
    ///
    /// # Arguments
    /// * `entry_hash` - Hash of the disputed entry
    /// * `escalation_reason` - Why this dispute needs community vote
    /// * `proposal_id` - The governance proposal ID tracking this escalation
    /// * `escalated_at` - Unix timestamp when escalation occurred
    ///
    /// # Returns
    /// The updated dispute record with Escalated status
    pub fn escalate_to_governance(
        &mut self,
        entry_hash: &ContentHash,
        escalation_reason: String,
        proposal_id: String,
        escalated_at: u64,
    ) -> Result<Dispute> {
        let mut dispute = self
            .active_disputes
            .remove(entry_hash)
            .context("Dispute not found")?;

        // Verify dispute is in a state that can be escalated
        match &dispute.status {
            DisputeStatus::Contested { .. } => {
                // OK to escalate
            }
            DisputeStatus::Resolved { .. } => {
                anyhow::bail!("Cannot escalate an already resolved dispute");
            }
            DisputeStatus::Escalated { .. } => {
                anyhow::bail!("Dispute is already escalated");
            }
            DisputeStatus::Normal => {
                anyhow::bail!("Cannot escalate a dispute with Normal status");
            }
        }

        // Update status to Escalated
        dispute.status = DisputeStatus::Escalated {
            proposal_id: proposal_id.clone(),
            escalation_reason: escalation_reason.clone(),
            escalated_at,
        };

        // Save the escalated dispute
        self.save_dispute(&dispute)?;

        info!(
            "Dispute {} escalated to governance (proposal: {}): {}",
            entry_hash, proposal_id, escalation_reason
        );

        Ok(dispute)
    }

    /// Resolve an escalated dispute based on governance outcome
    ///
    /// Called when a governance proposal for dispute resolution is closed.
    ///
    /// # Arguments
    /// * `entry_hash` - Hash of the disputed entry
    /// * `outcome` - The dispute outcome determined by governance vote
    /// * `resolved_at` - Unix timestamp when resolved
    pub fn resolve_escalated_dispute(
        &mut self,
        entry_hash: &ContentHash,
        outcome: DisputeOutcome,
        resolved_at: u64,
    ) -> Result<()> {
        // Load from storage since escalated disputes are not in active_disputes
        let key = format!("{}{}", DISPUTE_PREFIX, entry_hash.to_hex());
        let value = self
            .store
            .get(key.as_bytes())?
            .context("Dispute not found in storage")?;
        let mut dispute: Dispute =
            serde_json::from_slice(&value).context("Failed to deserialize dispute")?;

        // Verify dispute is escalated
        match &dispute.status {
            DisputeStatus::Escalated { .. } => {
                // OK to resolve
            }
            _ => {
                anyhow::bail!("Can only resolve escalated disputes with this method");
            }
        }

        // Use "governance" as the mediator for escalated disputes
        let governance_did = Did::from_str("did:icn:governance").unwrap_or_else(|_| {
            // Fallback: use a placeholder DID
            dispute.filed_by.clone()
        });

        dispute.status = DisputeStatus::Resolved {
            mediator: governance_did,
            outcome: outcome.clone(),
            resolved_at,
        };

        self.save_dispute(&dispute)?;

        info!(
            "Escalated dispute {} resolved via governance: {:?}",
            entry_hash, outcome
        );

        Ok(())
    }

    /// Save a dispute to storage
    fn save_dispute(&self, dispute: &Dispute) -> Result<()> {
        let key = format!("{}{}", DISPUTE_PREFIX, dispute.entry_hash.to_hex());
        let value = serde_json::to_vec(dispute)?;
        self.store.put(key.as_bytes(), &value)?;
        Ok(())
    }

    /// Load active disputes from storage
    fn load_active_disputes(&mut self) -> Result<()> {
        let prefix = DISPUTE_PREFIX.as_bytes();
        let items = self.store.scan(prefix)?;

        for (_key, value) in items {
            let dispute: Dispute =
                serde_json::from_slice(&value).context("Failed to deserialize dispute")?;

            // Only load active (contested) disputes
            if matches!(dispute.status, DisputeStatus::Contested { .. }) {
                self.active_disputes
                    .insert(dispute.entry_hash.clone(), dispute);
            }
        }

        info!("Loaded {} active disputes", self.active_disputes.len());

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use icn_store::SledStore;

    fn test_store() -> Arc<dyn Store> {
        Arc::new(SledStore::temporary().unwrap())
    }

    #[test]
    fn test_file_dispute() {
        let store = test_store();
        let mut manager = DisputeManager::new(store).unwrap();

        let entry_hash = ContentHash::from_bytes([1u8; 32]);
        let filer = KeyPair::generate().unwrap().did().clone();
        let reason = "Incorrect amount charged".to_string();
        let filed_at = 1000;

        let dispute = manager
            .file_dispute(entry_hash.clone(), filer.clone(), reason.clone(), filed_at)
            .unwrap();

        assert_eq!(dispute.entry_hash, entry_hash);
        assert_eq!(dispute.filed_by, filer);
        assert_eq!(dispute.reason, reason);
        assert_eq!(dispute.filed_at, filed_at);
        assert!(matches!(dispute.status, DisputeStatus::Contested { .. }));
        assert!(manager.has_active_dispute(&entry_hash));
        assert_eq!(manager.active_dispute_count(), 1);
    }

    #[test]
    fn test_duplicate_dispute_fails() {
        let store = test_store();
        let mut manager = DisputeManager::new(store).unwrap();

        let entry_hash = ContentHash::from_bytes([1u8; 32]);
        let filer = KeyPair::generate().unwrap().did().clone();

        // File first dispute
        manager
            .file_dispute(
                entry_hash.clone(),
                filer.clone(),
                "Reason 1".to_string(),
                1000,
            )
            .unwrap();

        // Try to file duplicate
        let result = manager.file_dispute(
            entry_hash.clone(),
            filer.clone(),
            "Reason 2".to_string(),
            2000,
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("already exists"));
    }

    #[test]
    fn test_add_evidence() {
        let store = test_store();
        let mut manager = DisputeManager::new(store).unwrap();

        let entry_hash = ContentHash::from_bytes([1u8; 32]);
        let filer = KeyPair::generate().unwrap().did().clone();

        manager
            .file_dispute(entry_hash.clone(), filer, "Reason".to_string(), 1000)
            .unwrap();

        manager
            .add_evidence(
                &entry_hash,
                "Email confirmation showing different amount".to_string(),
            )
            .unwrap();

        let dispute = manager.get_dispute(&entry_hash).unwrap();
        assert_eq!(dispute.evidence.len(), 1);
    }

    #[test]
    fn test_assign_mediator() {
        let store = test_store();
        let mut manager = DisputeManager::new(store).unwrap();

        let entry_hash = ContentHash::from_bytes([1u8; 32]);
        let filer = KeyPair::generate().unwrap().did().clone();
        let mediator = KeyPair::generate().unwrap().did().clone();

        manager
            .file_dispute(entry_hash.clone(), filer, "Reason".to_string(), 1000)
            .unwrap();

        manager
            .assign_mediator(&entry_hash, mediator.clone())
            .unwrap();

        let dispute = manager.get_dispute(&entry_hash).unwrap();
        assert_eq!(dispute.mediator, Some(mediator));
    }

    #[test]
    fn test_resolve_dispute() {
        let store = test_store();
        let mut manager = DisputeManager::new(store).unwrap();

        let entry_hash = ContentHash::from_bytes([1u8; 32]);
        let filer = KeyPair::generate().unwrap().did().clone();
        let mediator = KeyPair::generate().unwrap().did().clone();

        manager
            .file_dispute(entry_hash.clone(), filer, "Reason".to_string(), 1000)
            .unwrap();

        let outcome = DisputeOutcome::Upheld;
        manager
            .resolve_dispute(&entry_hash, mediator.clone(), outcome, 2000)
            .unwrap();

        // Should no longer be in active disputes
        assert!(!manager.has_active_dispute(&entry_hash));
        assert_eq!(manager.active_dispute_count(), 0);
    }

    #[test]
    fn test_get_disputes_by_filer() {
        let store = test_store();
        let mut manager = DisputeManager::new(store).unwrap();

        let filer1 = KeyPair::generate().unwrap().did().clone();
        let filer2 = KeyPair::generate().unwrap().did().clone();

        manager
            .file_dispute(
                ContentHash::from_bytes([1u8; 32]),
                filer1.clone(),
                "Reason 1".to_string(),
                1000,
            )
            .unwrap();

        manager
            .file_dispute(
                ContentHash::from_bytes([2u8; 32]),
                filer1.clone(),
                "Reason 2".to_string(),
                1001,
            )
            .unwrap();

        manager
            .file_dispute(
                ContentHash::from_bytes([3u8; 32]),
                filer2.clone(),
                "Reason 3".to_string(),
                1002,
            )
            .unwrap();

        let filer1_disputes = manager.get_disputes_by_filer(&filer1);
        assert_eq!(filer1_disputes.len(), 2);

        let filer2_disputes = manager.get_disputes_by_filer(&filer2);
        assert_eq!(filer2_disputes.len(), 1);
    }

    #[test]
    fn test_escalate_to_governance() {
        let store = test_store();
        let mut manager = DisputeManager::new(store).unwrap();

        let entry_hash = ContentHash::from_bytes([1u8; 32]);
        let filer = KeyPair::generate().unwrap().did().clone();

        // File a dispute first
        manager
            .file_dispute(entry_hash.clone(), filer, "Incorrect charge".to_string(), 1000)
            .unwrap();

        assert!(manager.has_active_dispute(&entry_hash));

        // Escalate to governance
        let escalated = manager
            .escalate_to_governance(
                &entry_hash,
                "Value exceeds mediator authority".to_string(),
                "proposal-123".to_string(),
                2000,
            )
            .unwrap();

        // Should no longer be in active disputes
        assert!(!manager.has_active_dispute(&entry_hash));

        // Status should be Escalated
        match escalated.status {
            DisputeStatus::Escalated {
                proposal_id,
                escalation_reason,
                escalated_at,
            } => {
                assert_eq!(proposal_id, "proposal-123");
                assert_eq!(escalation_reason, "Value exceeds mediator authority");
                assert_eq!(escalated_at, 2000);
            }
            _ => panic!("Expected Escalated status"),
        }
    }

    #[test]
    fn test_escalate_already_escalated_fails() {
        let store = test_store();
        let mut manager = DisputeManager::new(store).unwrap();

        let entry_hash = ContentHash::from_bytes([1u8; 32]);
        let filer = KeyPair::generate().unwrap().did().clone();

        manager
            .file_dispute(entry_hash.clone(), filer, "Reason".to_string(), 1000)
            .unwrap();

        // First escalation succeeds
        manager
            .escalate_to_governance(
                &entry_hash,
                "First escalation".to_string(),
                "proposal-1".to_string(),
                2000,
            )
            .unwrap();

        // Second escalation should fail (dispute no longer in active_disputes)
        let result = manager.escalate_to_governance(
            &entry_hash,
            "Second escalation".to_string(),
            "proposal-2".to_string(),
            3000,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_escalated_dispute() {
        let store = test_store();
        let mut manager = DisputeManager::new(store).unwrap();

        let entry_hash = ContentHash::from_bytes([1u8; 32]);
        let filer = KeyPair::generate().unwrap().did().clone();

        manager
            .file_dispute(entry_hash.clone(), filer, "Reason".to_string(), 1000)
            .unwrap();

        manager
            .escalate_to_governance(
                &entry_hash,
                "Escalation reason".to_string(),
                "proposal-123".to_string(),
                2000,
            )
            .unwrap();

        // Resolve via governance
        manager
            .resolve_escalated_dispute(&entry_hash, DisputeOutcome::Upheld, 3000)
            .unwrap();

        // Load from storage to verify
        let key = format!("ledger:dispute:{}", entry_hash.to_hex());
        let value = manager.store.get(key.as_bytes()).unwrap().unwrap();
        let dispute: Dispute = serde_json::from_slice(&value).unwrap();

        match dispute.status {
            DisputeStatus::Resolved { outcome, .. } => {
                assert_eq!(outcome, DisputeOutcome::Upheld);
            }
            _ => panic!("Expected Resolved status"),
        }
    }
}
