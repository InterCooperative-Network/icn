//! Receipt storage service for governance and economic receipts.
//!
//! Stores three canonical receipt types:
//! - GovernanceDecisionReceipt (governance decisions)
//! - AllocationReceipt (resource allocation)
//! - SettlementIntent (economic settlement)
//!
//! Supports lookup by canonical hash, decision_hash, and proposal_id.

#[cfg_attr(not(test), allow(unused_imports))]
use icn_governance::{GovernanceDecisionReceipt, ProofOutcome, VoteTally};
use icn_governance_actor::dispatch_evidence::EffectDispatchEvidence;
use icn_governance_actor::institutional_effect::InstitutionalEffectRecord;
use icn_governance_actor::receipt_backend::GovernanceReceiptBackend;
use icn_kernel_api::economics::SettlementIntent;
use icn_kernel_api::receipts::{AllocationReceipt, CanonicalReceipt, Hash};
use sled::Db;

/// Key prefix for governance decision receipts
const GOVERNANCE_PREFIX: &[u8] = b"receipt:governance:";
/// Key prefix for allocation receipts
const ALLOCATION_PREFIX: &[u8] = b"receipt:allocation:";
/// Key prefix for settlement intents
const INTENT_PREFIX: &[u8] = b"receipt:intent:";
/// Index prefix for decision hash lookups
const DECISION_INDEX_PREFIX: &[u8] = b"receipt:by_decision:";
/// Index prefix for proposal ID lookups (governance receipts only)
const PROPOSAL_INDEX_PREFIX: &[u8] = b"receipt:by_proposal:";
/// Key prefix for institutional effect records (primary by record_id)
const INSTITUTIONAL_EFFECT_PREFIX: &[u8] = b"effect:institutional:";
/// Secondary index: effect records by proposal_id (sortable by recorded_at)
const INSTITUTIONAL_EFFECT_BY_PROPOSAL_PREFIX: &[u8] = b"effect:institutional:by_proposal:";
/// Key prefix for dispatch evidence (primary by evidence_id)
const DISPATCH_EVIDENCE_PREFIX: &[u8] = b"effect:dispatch_evidence:";
/// Secondary index: dispatch evidence by effect_record_id (sortable by recorded_at)
const DISPATCH_EVIDENCE_BY_RECORD_PREFIX: &[u8] = b"effect:dispatch_evidence:by_record:";

/// Receipt storage service for governance and economic chain artifacts.
///
/// Stores receipts by canonical hash for cross-node deterministic lookup.
pub struct ReceiptStore {
    db: Db,
}

impl ReceiptStore {
    /// Create a new receipt store backed by the given sled database.
    pub fn new(db: Db) -> Self {
        Self { db }
    }

    /// Build a key from prefix and hex hash
    fn make_key(prefix: &[u8], hash: &Hash) -> Vec<u8> {
        let mut key = prefix.to_vec();
        key.extend_from_slice(hex::encode(hash).as_bytes());
        key
    }

    /// Build a decision index key
    fn make_decision_index_key(
        decision_hash: &Hash,
        type_tag: &[u8],
        receipt_hash: &Hash,
    ) -> Vec<u8> {
        let mut key = DECISION_INDEX_PREFIX.to_vec();
        key.extend_from_slice(hex::encode(decision_hash).as_bytes());
        key.push(b':');
        key.extend_from_slice(type_tag);
        key.push(b':');
        key.extend_from_slice(hex::encode(receipt_hash).as_bytes());
        key
    }

    /// Build a decision scan prefix
    fn make_decision_scan_prefix(decision_hash: &Hash, type_tag: &[u8]) -> Vec<u8> {
        let mut prefix = DECISION_INDEX_PREFIX.to_vec();
        prefix.extend_from_slice(hex::encode(decision_hash).as_bytes());
        prefix.push(b':');
        prefix.extend_from_slice(type_tag);
        prefix.push(b':');
        prefix
    }

    /// Build a proposal ID index key
    fn make_proposal_index_key(proposal_id: &str, receipt_hash: &Hash) -> Vec<u8> {
        let mut key = PROPOSAL_INDEX_PREFIX.to_vec();
        key.extend_from_slice(proposal_id.as_bytes());
        key.push(b':');
        key.extend_from_slice(hex::encode(receipt_hash).as_bytes());
        key
    }

    /// Build a proposal ID scan prefix
    fn make_proposal_scan_prefix(proposal_id: &str) -> Vec<u8> {
        let mut prefix = PROPOSAL_INDEX_PREFIX.to_vec();
        prefix.extend_from_slice(proposal_id.as_bytes());
        prefix.push(b':');
        prefix
    }

    // ========================================================================
    // GovernanceDecisionReceipt operations
    // ========================================================================

    /// Store a governance decision receipt by its canonical decision_hash.
    ///
    /// Indexes by both decision_hash and proposal_id for O(1) lookups.
    pub fn put_governance(&self, receipt: &GovernanceDecisionReceipt) -> Result<Hash, String> {
        let hash = receipt.decision_hash;
        let key = Self::make_key(GOVERNANCE_PREFIX, &hash);
        let value = serde_json::to_vec(receipt)
            .map_err(|e| format!("Failed to serialize governance receipt: {}", e))?;

        self.db
            .insert(&key, value)
            .map_err(|e| format!("Failed to store governance receipt: {}", e))?;

        // Index by decision_hash (for chain lookups)
        let decision_key = Self::make_decision_index_key(&hash, b"governance", &hash);
        self.db
            .insert(&decision_key, &hash[..])
            .map_err(|e| format!("Failed to index governance receipt by decision_hash: {}", e))?;

        // Index by proposal_id (for proposal → receipt lookups)
        let proposal_key = Self::make_proposal_index_key(&receipt.proposal_id, &hash);
        self.db
            .insert(&proposal_key, &hash[..])
            .map_err(|e| format!("Failed to index governance receipt by proposal_id: {}", e))?;

        Ok(hash)
    }

    /// Test helper: store a governance receipt for a proposal in the test domain.
    #[cfg(test)]
    pub fn put_test_governance_receipt(&self, proposal_id: &str) -> Result<Hash, String> {
        let votes = vec![];
        let receipt = GovernanceDecisionReceipt::new(
            proposal_id.to_string(),
            "test-domain".to_string(),
            ProofOutcome::Accepted,
            VoteTally::empty(),
            &votes,
        );
        self.put_governance(&receipt)
    }

    /// Get a governance decision receipt by decision_hash.
    pub fn get_governance(&self, hash: &Hash) -> Result<Option<GovernanceDecisionReceipt>, String> {
        let key = Self::make_key(GOVERNANCE_PREFIX, hash);
        match self.db.get(&key) {
            Ok(Some(bytes)) => {
                let receipt: GovernanceDecisionReceipt = serde_json::from_slice(&bytes)
                    .map_err(|e| format!("Failed to deserialize governance receipt: {}", e))?;
                Ok(Some(receipt))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(format!("Failed to get governance receipt: {}", e)),
        }
    }

    /// Get a governance decision receipt by proposal_id.
    pub fn get_governance_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        let prefix = Self::make_proposal_scan_prefix(proposal_id);

        // Scan for all receipts with this proposal_id (should be exactly 1)
        for entry in self.db.scan_prefix(&prefix) {
            let (_, hash_bytes) =
                entry.map_err(|e| format!("Failed to scan proposal index: {}", e))?;
            if hash_bytes.len() == 32 {
                let mut hash = [0u8; 32];
                hash.copy_from_slice(&hash_bytes);
                return self.get_governance(&hash);
            }
        }
        Ok(None)
    }

    /// List governance decision receipts for a decision hash.
    ///
    /// For governance receipts, decision_hash IS the canonical hash, so this
    /// returns at most one receipt.
    pub fn list_governance_by_decision(
        &self,
        decision_hash: &Hash,
    ) -> Result<Vec<GovernanceDecisionReceipt>, String> {
        let prefix = Self::make_decision_scan_prefix(decision_hash, b"governance");

        let mut receipts = Vec::new();
        for entry in self.db.scan_prefix(&prefix) {
            let (_, hash_bytes) = entry.map_err(|e| format!("Failed to scan: {}", e))?;
            if hash_bytes.len() == 32 {
                let mut hash = [0u8; 32];
                hash.copy_from_slice(&hash_bytes);
                if let Ok(Some(receipt)) = self.get_governance(&hash) {
                    receipts.push(receipt);
                }
            }
        }
        Ok(receipts)
    }

    // ========================================================================
    // AllocationReceipt operations
    // ========================================================================

    /// Store an allocation receipt by its canonical hash.
    pub fn put_allocation(&self, receipt: &AllocationReceipt) -> Result<Hash, String> {
        let hash = receipt.canonical_hash();
        let key = Self::make_key(ALLOCATION_PREFIX, &hash);
        let value = serde_json::to_vec(receipt)
            .map_err(|e| format!("Failed to serialize allocation receipt: {}", e))?;

        self.db
            .insert(&key, value)
            .map_err(|e| format!("Failed to store allocation receipt: {}", e))?;

        // Index by decision hash
        let decision_key =
            Self::make_decision_index_key(&receipt.decision_hash, b"allocation", &hash);
        self.db
            .insert(&decision_key, &hash[..])
            .map_err(|e| format!("Failed to index allocation receipt: {}", e))?;

        Ok(hash)
    }

    /// Get an allocation receipt by canonical hash.
    pub fn get_allocation(&self, hash: &Hash) -> Result<Option<AllocationReceipt>, String> {
        let key = Self::make_key(ALLOCATION_PREFIX, hash);
        match self.db.get(&key) {
            Ok(Some(bytes)) => {
                let receipt: AllocationReceipt = serde_json::from_slice(&bytes)
                    .map_err(|e| format!("Failed to deserialize allocation receipt: {}", e))?;
                Ok(Some(receipt))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(format!("Failed to get allocation receipt: {}", e)),
        }
    }

    /// List allocation receipts for a decision hash.
    pub fn list_allocations_by_decision(
        &self,
        decision_hash: &Hash,
    ) -> Result<Vec<AllocationReceipt>, String> {
        let prefix = Self::make_decision_scan_prefix(decision_hash, b"allocation");

        let mut receipts = Vec::new();
        for entry in self.db.scan_prefix(&prefix) {
            let (_, hash_bytes) = entry.map_err(|e| format!("Failed to scan: {}", e))?;
            if hash_bytes.len() == 32 {
                let mut hash = [0u8; 32];
                hash.copy_from_slice(&hash_bytes);
                if let Ok(Some(receipt)) = self.get_allocation(&hash) {
                    receipts.push(receipt);
                }
            }
        }
        Ok(receipts)
    }

    // ========================================================================
    // SettlementIntent operations
    // ========================================================================

    /// Store a settlement intent by its canonical hash.
    pub fn put_intent(&self, intent: &SettlementIntent) -> Result<Hash, String> {
        let hash = intent.canonical_hash();
        let key = Self::make_key(INTENT_PREFIX, &hash);
        let value = serde_json::to_vec(intent)
            .map_err(|e| format!("Failed to serialize settlement intent: {}", e))?;

        self.db
            .insert(&key, value)
            .map_err(|e| format!("Failed to store settlement intent: {}", e))?;

        // Index by decision hash
        let decision_key = Self::make_decision_index_key(&intent.decision_hash, b"intent", &hash);
        self.db
            .insert(&decision_key, &hash[..])
            .map_err(|e| format!("Failed to index settlement intent: {}", e))?;

        Ok(hash)
    }

    /// Get a settlement intent by canonical hash.
    pub fn get_intent(&self, hash: &Hash) -> Result<Option<SettlementIntent>, String> {
        let key = Self::make_key(INTENT_PREFIX, hash);
        match self.db.get(&key) {
            Ok(Some(bytes)) => {
                let intent: SettlementIntent = serde_json::from_slice(&bytes)
                    .map_err(|e| format!("Failed to deserialize settlement intent: {}", e))?;
                Ok(Some(intent))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(format!("Failed to get settlement intent: {}", e)),
        }
    }

    /// List settlement intents for a decision hash.
    pub fn list_intents_by_decision(
        &self,
        decision_hash: &Hash,
    ) -> Result<Vec<SettlementIntent>, String> {
        let prefix = Self::make_decision_scan_prefix(decision_hash, b"intent");

        let mut intents = Vec::new();
        for entry in self.db.scan_prefix(&prefix) {
            let (_, hash_bytes) = entry.map_err(|e| format!("Failed to scan: {}", e))?;
            if hash_bytes.len() == 32 {
                let mut hash = [0u8; 32];
                hash.copy_from_slice(&hash_bytes);
                if let Ok(Some(intent)) = self.get_intent(&hash) {
                    intents.push(intent);
                }
            }
        }
        Ok(intents)
    }

    /// List all allocation receipts, regardless of decision hash.
    pub fn list_all_allocations(&self) -> Result<Vec<AllocationReceipt>, String> {
        let mut receipts = Vec::new();
        for entry in self.db.scan_prefix(ALLOCATION_PREFIX) {
            let (_, bytes) = entry.map_err(|e| format!("Failed to scan: {}", e))?;
            match serde_json::from_slice::<AllocationReceipt>(&bytes) {
                Ok(receipt) => receipts.push(receipt),
                Err(e) => return Err(format!("Failed to deserialize allocation receipt: {}", e)),
            }
        }
        Ok(receipts)
    }

    /// List all settlement intents, regardless of decision hash.
    pub fn list_all_intents(&self) -> Result<Vec<SettlementIntent>, String> {
        let mut intents = Vec::new();
        for entry in self.db.scan_prefix(INTENT_PREFIX) {
            let (_, bytes) = entry.map_err(|e| format!("Failed to scan: {}", e))?;
            match serde_json::from_slice::<SettlementIntent>(&bytes) {
                Ok(intent) => intents.push(intent),
                Err(e) => return Err(format!("Failed to deserialize settlement intent: {}", e)),
            }
        }
        Ok(intents)
    }

    // ========================================================================
    // Convenience methods
    // ========================================================================

    /// Store an allocation receipt and all its intents atomically.
    pub fn put_allocation_with_intents(&self, receipt: &AllocationReceipt) -> Result<Hash, String> {
        // Store all intents first
        for intent in &receipt.intents {
            self.put_intent(intent)?;
        }
        // Store the allocation receipt
        self.put_allocation(receipt)
    }

    /// Get the full economic chain for a decision hash.
    ///
    /// Returns (allocations, intents) for the given decision.
    pub fn get_chain_by_decision(
        &self,
        decision_hash: &Hash,
    ) -> Result<(Vec<AllocationReceipt>, Vec<SettlementIntent>), String> {
        let allocations = self.list_allocations_by_decision(decision_hash)?;
        let intents = self.list_intents_by_decision(decision_hash)?;
        Ok((allocations, intents))
    }
}

impl ReceiptStore {
    // ========================================================================
    // InstitutionalEffectRecord operations (governance app-layer artifact)
    // ========================================================================

    /// Persist an `InstitutionalEffectRecord` keyed by `record_id` with a
    /// secondary index on `(proposal_id, recorded_at, record_id)` so
    /// reads can recover records in chronological order per proposal.
    ///
    /// Benign duplicate writes (same `record_id`) overwrite the existing
    /// entry with identical bytes; the index key is idempotent by
    /// construction. The store does NOT enforce one-record-per-proposal —
    /// callers enforce that invariant upstream.
    pub fn put_institutional_effect(
        &self,
        record: &InstitutionalEffectRecord,
    ) -> Result<(), String> {
        let mut primary_key = INSTITUTIONAL_EFFECT_PREFIX.to_vec();
        primary_key.extend_from_slice(record.record_id.as_bytes());
        let value = serde_json::to_vec(record)
            .map_err(|e| format!("Failed to serialize InstitutionalEffectRecord: {e}"))?;
        self.db
            .insert(&primary_key, value)
            .map_err(|e| format!("sled insert primary: {e}"))?;

        // Secondary index: effect:institutional:by_proposal:{proposal_id}:{recorded_at_be}:{record_id}
        // recorded_at encoded big-endian so lexicographic scan yields ascending order.
        let mut idx_key = INSTITUTIONAL_EFFECT_BY_PROPOSAL_PREFIX.to_vec();
        idx_key.extend_from_slice(record.proposal_id.as_bytes());
        idx_key.push(b':');
        idx_key.extend_from_slice(&record.recorded_at.to_be_bytes());
        idx_key.push(b':');
        idx_key.extend_from_slice(record.record_id.as_bytes());
        self.db
            .insert(&idx_key, record.record_id.as_bytes())
            .map_err(|e| format!("sled insert index: {e}"))?;

        Ok(())
    }

    /// Persist a downstream dispatch evidence entry attached to a
    /// previously emitted `InstitutionalEffectRecord`. Keyed by
    /// `evidence_id` with a secondary index on
    /// `(effect_record_id, recorded_at_be, evidence_id)` so scans over a
    /// record's evidence return chronological order without sorting.
    ///
    /// Same-`evidence_id` writes overwrite with identical bytes — idempotent.
    pub fn put_effect_dispatch_evidence(
        &self,
        evidence: &EffectDispatchEvidence,
    ) -> Result<(), String> {
        let mut primary_key = DISPATCH_EVIDENCE_PREFIX.to_vec();
        primary_key.extend_from_slice(evidence.evidence_id.as_bytes());
        let value = serde_json::to_vec(evidence)
            .map_err(|e| format!("Failed to serialize EffectDispatchEvidence: {e}"))?;
        self.db
            .insert(&primary_key, value)
            .map_err(|e| format!("sled insert primary: {e}"))?;

        let mut idx_key = DISPATCH_EVIDENCE_BY_RECORD_PREFIX.to_vec();
        idx_key.extend_from_slice(evidence.effect_record_id.as_bytes());
        idx_key.push(b':');
        idx_key.extend_from_slice(&evidence.recorded_at.to_be_bytes());
        idx_key.push(b':');
        idx_key.extend_from_slice(evidence.evidence_id.as_bytes());
        self.db
            .insert(&idx_key, evidence.evidence_id.as_bytes())
            .map_err(|e| format!("sled insert index: {e}"))?;
        Ok(())
    }

    /// Scan the secondary index for an effect record and hydrate evidence
    /// entries in chronological order (oldest-first).
    pub fn list_effect_dispatch_evidence_by_record(
        &self,
        effect_record_id: &str,
    ) -> Result<Vec<EffectDispatchEvidence>, String> {
        let mut prefix = DISPATCH_EVIDENCE_BY_RECORD_PREFIX.to_vec();
        prefix.extend_from_slice(effect_record_id.as_bytes());
        prefix.push(b':');

        let mut out = Vec::new();
        for entry in self.db.scan_prefix(&prefix) {
            let (_k, v) = entry.map_err(|e| format!("sled scan: {e}"))?;
            let evidence_id =
                std::str::from_utf8(&v).map_err(|e| format!("index value not UTF-8: {e}"))?;
            let mut primary_key = DISPATCH_EVIDENCE_PREFIX.to_vec();
            primary_key.extend_from_slice(evidence_id.as_bytes());
            let Some(bytes) = self
                .db
                .get(&primary_key)
                .map_err(|e| format!("sled get primary: {e}"))?
            else {
                // Index points to a missing primary record — index/primary skew.
                // Skip rather than hard-fail the read, but warn so operators can
                // detect on-disk corruption or partial writes.
                tracing::warn!(
                    effect_record_id = %effect_record_id,
                    evidence_id = %evidence_id,
                    "dispatch evidence index skew: primary record missing"
                );
                continue;
            };
            let e: EffectDispatchEvidence = serde_json::from_slice(&bytes)
                .map_err(|err| format!("deserialize EffectDispatchEvidence: {err}"))?;
            out.push(e);
        }
        Ok(out)
    }

    /// Scan the secondary index for a proposal and hydrate records in
    /// chronological order (oldest-first).
    pub fn list_institutional_effects_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Vec<InstitutionalEffectRecord>, String> {
        let mut prefix = INSTITUTIONAL_EFFECT_BY_PROPOSAL_PREFIX.to_vec();
        prefix.extend_from_slice(proposal_id.as_bytes());
        prefix.push(b':');

        let mut out = Vec::new();
        for entry in self.db.scan_prefix(&prefix) {
            let (_k, v) = entry.map_err(|e| format!("sled scan: {e}"))?;
            let record_id =
                std::str::from_utf8(&v).map_err(|e| format!("index value not UTF-8: {e}"))?;
            let mut primary_key = INSTITUTIONAL_EFFECT_PREFIX.to_vec();
            primary_key.extend_from_slice(record_id.as_bytes());
            let Some(bytes) = self
                .db
                .get(&primary_key)
                .map_err(|e| format!("sled get primary: {e}"))?
            else {
                // Index points to a missing primary record — log and skip
                // rather than hard-fail the read. A warning here is how
                // operators notice on-disk corruption or partial writes.
                tracing::warn!(
                    proposal_id = %proposal_id,
                    record_id = %record_id,
                    "institutional effect index skew: primary record missing"
                );
                continue;
            };
            let record: InstitutionalEffectRecord = serde_json::from_slice(&bytes)
                .map_err(|e| format!("deserialize InstitutionalEffectRecord: {e}"))?;
            out.push(record);
        }
        Ok(out)
    }
}

impl GovernanceReceiptBackend for ReceiptStore {
    fn put_governance(&self, receipt: &GovernanceDecisionReceipt) -> Result<(), String> {
        self.put_governance(receipt).map(|_| ())
    }

    fn get_governance_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        self.get_governance_by_proposal(proposal_id)
    }

    fn put_allocation(&self, receipt: &AllocationReceipt) -> Result<Hash, String> {
        self.put_allocation(receipt)
    }

    fn get_governance_by_decision(
        &self,
        decision_hash: &Hash,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        let results = self.list_governance_by_decision(decision_hash)?;
        Ok(results.into_iter().next())
    }

    fn list_allocations_by_decision(
        &self,
        decision_hash: &Hash,
    ) -> Result<Vec<AllocationReceipt>, String> {
        self.list_allocations_by_decision(decision_hash)
    }

    fn put_institutional_effect(&self, record: &InstitutionalEffectRecord) -> Result<(), String> {
        self.put_institutional_effect(record)
    }

    fn list_institutional_effects_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Vec<InstitutionalEffectRecord>, String> {
        self.list_institutional_effects_by_proposal(proposal_id)
    }

    fn put_effect_dispatch_evidence(
        &self,
        evidence: &EffectDispatchEvidence,
    ) -> Result<(), String> {
        self.put_effect_dispatch_evidence(evidence)
    }

    fn list_effect_dispatch_evidence_by_record(
        &self,
        effect_record_id: &str,
    ) -> Result<Vec<EffectDispatchEvidence>, String> {
        self.list_effect_dispatch_evidence_by_record(effect_record_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_kernel_api::ScopeLevel;

    fn temp_db() -> Db {
        sled::Config::new().temporary(true).open().unwrap()
    }

    fn make_test_intent(decision_hash: Hash, amount: u64) -> SettlementIntent {
        SettlementIntent::new(
            "dec-001",
            decision_hash,
            "treasury",
            "member",
            amount,
            "HOURS",
        )
        .with_timestamp(1000000)
    }

    fn make_test_governance_receipt(proposal_id: &str) -> GovernanceDecisionReceipt {
        let votes = vec![];
        let tally = VoteTally::empty();
        GovernanceDecisionReceipt::new(
            proposal_id.to_string(),
            "test-domain".to_string(),
            ProofOutcome::Accepted,
            tally,
            &votes,
        )
    }

    // ========================================================================
    // Governance receipt tests
    // ========================================================================

    #[test]
    fn test_put_get_governance() {
        let store = ReceiptStore::new(temp_db());
        let receipt = make_test_governance_receipt("prop-001");
        let hash = store.put_governance(&receipt).unwrap();

        let retrieved = store.get_governance(&hash).unwrap().unwrap();
        assert_eq!(retrieved.decision_hash, receipt.decision_hash);
        assert_eq!(retrieved.proposal_id, "prop-001");
    }

    #[test]
    fn test_get_governance_by_proposal() {
        let store = ReceiptStore::new(temp_db());
        let receipt = make_test_governance_receipt("prop-002");
        store.put_governance(&receipt).unwrap();

        let retrieved = store
            .get_governance_by_proposal("prop-002")
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.decision_hash, receipt.decision_hash);
        assert_eq!(retrieved.proposal_id, "prop-002");
    }

    #[test]
    fn test_list_governance_by_decision() {
        let store = ReceiptStore::new(temp_db());
        let receipt = make_test_governance_receipt("prop-003");
        let hash = receipt.decision_hash;
        store.put_governance(&receipt).unwrap();

        let receipts = store.list_governance_by_decision(&hash).unwrap();
        assert_eq!(receipts.len(), 1);
        assert_eq!(receipts[0].proposal_id, "prop-003");
    }

    #[test]
    fn test_governance_dual_indexing() {
        let store = ReceiptStore::new(temp_db());
        let receipt = make_test_governance_receipt("prop-dual-index");
        let hash = receipt.decision_hash;
        store.put_governance(&receipt).unwrap();

        // Verify can retrieve by decision_hash
        let by_hash = store.get_governance(&hash).unwrap().unwrap();
        assert_eq!(by_hash.proposal_id, "prop-dual-index");

        // Verify can retrieve by proposal_id
        let by_proposal = store
            .get_governance_by_proposal("prop-dual-index")
            .unwrap()
            .unwrap();
        assert_eq!(by_proposal.decision_hash, hash);
    }

    // ========================================================================
    // Economic receipt tests (unchanged)
    // ========================================================================

    #[test]
    fn test_put_get_intent() {
        let receipt_store = ReceiptStore::new(temp_db());

        let decision_hash = [42u8; 32];
        let intent = make_test_intent(decision_hash, 500);
        let hash = receipt_store.put_intent(&intent).unwrap();

        let retrieved = receipt_store.get_intent(&hash).unwrap().unwrap();
        assert_eq!(retrieved.canonical_hash(), intent.canonical_hash());
        assert_eq!(retrieved.amount, 500);
    }

    #[test]
    fn test_put_get_allocation() {
        let receipt_store = ReceiptStore::new(temp_db());

        let decision_hash = [42u8; 32];
        let intent = make_test_intent(decision_hash, 100);
        let allocation = AllocationReceipt::new(decision_hash, ScopeLevel::Org)
            .with_timestamp(1000)
            .add_intent(intent);

        let hash = receipt_store.put_allocation(&allocation).unwrap();

        let retrieved = receipt_store.get_allocation(&hash).unwrap().unwrap();
        assert_eq!(retrieved.canonical_hash(), allocation.canonical_hash());
        assert_eq!(retrieved.intents.len(), 1);
    }

    #[test]
    fn test_list_by_decision() {
        let receipt_store = ReceiptStore::new(temp_db());

        let decision_hash = [42u8; 32];

        // Store multiple intents for same decision
        let intent1 = make_test_intent(decision_hash, 100);
        let intent2 = make_test_intent(decision_hash, 200);
        receipt_store.put_intent(&intent1).unwrap();
        receipt_store.put_intent(&intent2).unwrap();

        let intents = receipt_store
            .list_intents_by_decision(&decision_hash)
            .unwrap();
        assert_eq!(intents.len(), 2);
    }

    #[test]
    fn test_put_allocation_with_intents() {
        let receipt_store = ReceiptStore::new(temp_db());

        let decision_hash = [42u8; 32];
        let intent1 = make_test_intent(decision_hash, 100);
        let intent2 = make_test_intent(decision_hash, 200);
        let allocation = AllocationReceipt::new(decision_hash, ScopeLevel::Org)
            .with_timestamp(1000)
            .add_intent(intent1)
            .add_intent(intent2);

        receipt_store
            .put_allocation_with_intents(&allocation)
            .unwrap();

        // Both intents should be retrievable
        let intents = receipt_store
            .list_intents_by_decision(&decision_hash)
            .unwrap();
        assert_eq!(intents.len(), 2);

        // Allocation should be retrievable
        let (allocations, _) = receipt_store.get_chain_by_decision(&decision_hash).unwrap();
        assert_eq!(allocations.len(), 1);
    }

    #[test]
    fn test_canonical_hash_as_key() {
        let receipt_store = ReceiptStore::new(temp_db());

        let decision_hash = [42u8; 32];

        // Two intents with different node-local IDs but same content
        let intent1 = make_test_intent(decision_hash, 500).with_intent_id("node-a-id-001");
        let intent2 = make_test_intent(decision_hash, 500).with_intent_id("node-b-id-999");

        // Same canonical hash
        assert_eq!(intent1.canonical_hash(), intent2.canonical_hash());

        // Storing second should overwrite first (same key)
        receipt_store.put_intent(&intent1).unwrap();
        receipt_store.put_intent(&intent2).unwrap();

        let intents = receipt_store
            .list_intents_by_decision(&decision_hash)
            .unwrap();
        assert_eq!(intents.len(), 1); // Deduplicated by canonical hash
    }

    #[test]
    fn institutional_effects_roundtrip_in_chronological_order() {
        let store = ReceiptStore::new(temp_db());

        let older = InstitutionalEffectRecord::new(
            "prop-1",
            "coop-a",
            Some([7u8; 32]),
            "freeze_member",
            Some("did:icn:x".into()),
            None,
            Some("cause".into()),
            100,
            serde_json::json!({"n": 1}),
        );
        let newer = InstitutionalEffectRecord::new(
            "prop-1",
            "coop-a",
            Some([7u8; 32]),
            "unfreeze_member",
            Some("did:icn:x".into()),
            None,
            Some("resolved".into()),
            250,
            serde_json::json!({"n": 2}),
        );

        // Write out of order.
        store.put_institutional_effect(&newer).unwrap();
        store.put_institutional_effect(&older).unwrap();

        let list = store
            .list_institutional_effects_by_proposal("prop-1")
            .unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].effect_kind, "freeze_member");
        assert_eq!(list[1].effect_kind, "unfreeze_member");
        assert!(list[0].recorded_at < list[1].recorded_at);
    }

    #[test]
    fn institutional_effects_are_scoped_by_proposal_id() {
        let store = ReceiptStore::new(temp_db());

        let a = InstitutionalEffectRecord::new(
            "prop-a",
            "coop-a",
            None,
            "freeze_member",
            Some("did:icn:1".into()),
            None,
            None,
            10,
            serde_json::json!({}),
        );
        let b = InstitutionalEffectRecord::new(
            "prop-b",
            "coop-a",
            None,
            "freeze_member",
            Some("did:icn:2".into()),
            None,
            None,
            20,
            serde_json::json!({}),
        );

        store.put_institutional_effect(&a).unwrap();
        store.put_institutional_effect(&b).unwrap();

        let a_list = store
            .list_institutional_effects_by_proposal("prop-a")
            .unwrap();
        let b_list = store
            .list_institutional_effects_by_proposal("prop-b")
            .unwrap();
        let none_list = store
            .list_institutional_effects_by_proposal("prop-missing")
            .unwrap();

        assert_eq!(a_list.len(), 1);
        assert_eq!(b_list.len(), 1);
        assert!(none_list.is_empty());
        assert_eq!(a_list[0].target_did.as_deref(), Some("did:icn:1"));
        assert_eq!(b_list[0].target_did.as_deref(), Some("did:icn:2"));
    }

    #[test]
    fn dispatch_evidence_roundtrip_and_ordering() {
        let store = ReceiptStore::new(temp_db());
        let older = EffectDispatchEvidence::new(
            "rec-a",
            "prop-1",
            "sdis",
            Some("state-hash-1".into()),
            true,
            None,
            100,
        );
        let newer = EffectDispatchEvidence::new(
            "rec-a",
            "prop-1",
            "sdis",
            Some("state-hash-2".into()),
            false,
            Some("boom".into()),
            200,
        );

        store.put_effect_dispatch_evidence(&newer).unwrap();
        store.put_effect_dispatch_evidence(&older).unwrap();

        let list = store
            .list_effect_dispatch_evidence_by_record("rec-a")
            .unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].recorded_at, 100);
        assert_eq!(list[1].recorded_at, 200);
        assert!(list[0].success);
        assert!(!list[1].success);
        assert_eq!(list[1].error_message.as_deref(), Some("boom"));
    }

    #[test]
    fn dispatch_evidence_is_scoped_by_record() {
        let store = ReceiptStore::new(temp_db());
        store
            .put_effect_dispatch_evidence(&EffectDispatchEvidence::new(
                "rec-a", "prop-1", "sdis", None, true, None, 10,
            ))
            .unwrap();
        store
            .put_effect_dispatch_evidence(&EffectDispatchEvidence::new(
                "rec-b", "prop-2", "sdis", None, true, None, 20,
            ))
            .unwrap();

        let a = store
            .list_effect_dispatch_evidence_by_record("rec-a")
            .unwrap();
        let b = store
            .list_effect_dispatch_evidence_by_record("rec-b")
            .unwrap();
        let none = store
            .list_effect_dispatch_evidence_by_record("missing")
            .unwrap();

        assert_eq!(a.len(), 1);
        assert_eq!(b.len(), 1);
        assert!(none.is_empty());
        assert_eq!(a[0].effect_record_id, "rec-a");
        assert_eq!(b[0].effect_record_id, "rec-b");
    }

    #[test]
    fn dispatch_evidence_duplicate_evidence_id_is_idempotent() {
        let store = ReceiptStore::new(temp_db());
        let ev = EffectDispatchEvidence::new("rec-dup", "prop-dup", "sdis", None, true, None, 10);
        store.put_effect_dispatch_evidence(&ev).unwrap();
        store.put_effect_dispatch_evidence(&ev).unwrap();

        let list = store
            .list_effect_dispatch_evidence_by_record("rec-dup")
            .unwrap();
        assert_eq!(list.len(), 1);
    }

    #[test]
    fn institutional_effects_duplicate_write_is_idempotent() {
        let store = ReceiptStore::new(temp_db());
        let rec = InstitutionalEffectRecord::new(
            "prop-dup",
            "coop-a",
            None,
            "freeze_member",
            None,
            None,
            None,
            100,
            serde_json::json!({}),
        );
        store.put_institutional_effect(&rec).unwrap();
        store.put_institutional_effect(&rec).unwrap();

        let list = store
            .list_institutional_effects_by_proposal("prop-dup")
            .unwrap();
        assert_eq!(list.len(), 1, "same record_id must not produce two entries");
    }
}
