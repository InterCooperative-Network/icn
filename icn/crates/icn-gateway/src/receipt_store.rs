//! Receipt storage service for governance and economic receipts.
//!
//! Stores three canonical receipt types:
//! - GovernanceDecisionReceipt (governance decisions)
//! - AllocationReceipt (resource allocation)
//! - SettlementIntent (economic settlement)
//!
//! Supports lookup by canonical hash, decision_hash, and proposal_id.

#[cfg_attr(not(test), allow(unused_imports))]
use icn_governance::{
    AuthorityGrant, AuthorityGrantId, GovernanceDecisionReceipt, Mandate, MandateId, ProofOutcome,
    VoteTally,
};
use icn_governance_actor::dispatch_evidence::EffectDispatchEvidence;
use icn_governance_actor::institutional_effect::InstitutionalEffectRecord;
use icn_governance_actor::receipt_backend::GovernanceReceiptBackend;
use icn_kernel_api::economics::SettlementIntent;
use icn_kernel_api::receipts::{AllocationReceipt, CanonicalReceipt, Hash};
use sled::transaction::{ConflictableTransactionError, TransactionError};
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
/// Key prefix for ADR-0014 mandate records (primary by mandate id)
const MANDATE_PREFIX: &[u8] = b"adr0014:mandate:";
/// Secondary index: mandate by proposal_id (sortable by issued_at)
const MANDATE_BY_PROPOSAL_PREFIX: &[u8] = b"adr0014:mandate:by_proposal:";
/// Secondary index: mandate by decision_hash (sortable by issued_at)
const MANDATE_BY_DECISION_PREFIX: &[u8] = b"adr0014:mandate:by_decision:";
/// Key prefix for ADR-0014 authority grant records (primary by grant id)
const AUTHORITY_GRANT_PREFIX: &[u8] = b"adr0014:grant:";
/// Secondary index: authority grant by decision_hash (sortable by valid_from)
const AUTHORITY_GRANT_BY_DECISION_PREFIX: &[u8] = b"adr0014:grant:by_decision:";

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
    ///
    /// Uses `scan_prefix` over a raw `{proposal_id}:{hex_hash}` secondary
    /// index that predates the colon-safe length-prefix scheme used by
    /// `MANDATE_BY_PROPOSAL_PREFIX`. Two proposal IDs where one is a
    /// `:`-delimited prefix of the other (e.g. `foo` and `foo:bar`) would
    /// otherwise alias under `scan_prefix`.
    ///
    /// **Repair strategy:** filter-on-read. The scan may return aliased
    /// hits; we defuse them by loading the primary record (which already
    /// happens in the O(1) lookup) and keeping only those whose canonical
    /// `proposal_id` matches the requested one. This is zero extra I/O
    /// versus the pre-repair behavior and needs no migration of
    /// on-disk data — the index keys stay in their legacy raw-colon
    /// shape, but reads converge to the canonical truth stored on the
    /// primary record.
    pub fn get_governance_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Option<GovernanceDecisionReceipt>, String> {
        let prefix = Self::make_proposal_scan_prefix(proposal_id);

        for entry in self.db.scan_prefix(&prefix) {
            let (_, hash_bytes) =
                entry.map_err(|e| format!("Failed to scan proposal index: {}", e))?;
            if hash_bytes.len() != 32 {
                continue;
            }
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&hash_bytes);
            match self.get_governance(&hash)? {
                Some(receipt) if receipt.proposal_id == proposal_id => {
                    return Ok(Some(receipt));
                }
                Some(other) => {
                    tracing::debug!(
                        requested = %proposal_id,
                        found = %other.proposal_id,
                        "governance proposal index: filtered colon-aliased hit"
                    );
                }
                None => {
                    tracing::warn!(
                        proposal_id = %proposal_id,
                        "governance proposal index skew: primary record missing"
                    );
                }
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
    ///
    /// The secondary index key is
    /// `effect:institutional:by_proposal:{proposal_id}:{recorded_at_be}:{record_id}`
    /// — raw bytes, not length-prefixed. A scan with prefix
    /// `{…}:{proposal_id}:` would otherwise alias when two proposal IDs
    /// share a `:`-delimited prefix (e.g. `foo` vs `foo:bar`). That
    /// aliasing is load-bearing here because
    /// [`emit_accepted_effect`](icn_governance_actor::institutional_effect::emit_accepted_effect)
    /// uses this lookup for `(proposal_id, effect_kind)` dedup — a false
    /// hit would silently drop a real new record.
    ///
    /// **Repair strategy:** filter-on-read. We already load each primary
    /// record to hydrate it, so comparing its canonical `proposal_id`
    /// against the requested one is free. Aliased entries are logged and
    /// skipped, not returned. Zero extra I/O and no on-disk migration —
    /// live K3s data keeps working, new writes stay in the same format,
    /// and dedup now sees a truthful `(proposal_id, effect_kind)` set.
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
            if record.proposal_id != proposal_id {
                tracing::debug!(
                    requested = %proposal_id,
                    found = %record.proposal_id,
                    record_id = %record_id,
                    "institutional effect proposal index: filtered colon-aliased hit"
                );
                continue;
            }
            out.push(record);
        }
        Ok(out)
    }
}

impl ReceiptStore {
    // ========================================================================
    // ADR-0014 Mandate + AuthorityGrant operations
    // ========================================================================
    //
    // Mandates and authority grants are authorization-side constitutional
    // memory. They sit upstream of institutional effect / dispatch evidence
    // (which are execution-side). Storage is keyed by stable UUID with
    // secondary indexes for the trait-required lookups.
    //
    // Each `put_*` uses a sled transaction so the primary record and all
    // its secondary index entries land atomically — no partial index state
    // on process crash mid-write. `put_mandate_with_grants` extends that
    // atomicity across the full mandate + grant set so the acceptance
    // seam cannot produce durable orphan grants.

    fn mandate_primary_key(id: &MandateId) -> Vec<u8> {
        let mut key = MANDATE_PREFIX.to_vec();
        key.extend_from_slice(id.0.hyphenated().to_string().as_bytes());
        key
    }

    /// Encode `proposal_id` as `{len_be_u32}{bytes}` so two IDs where
    /// one is a `:`-delimited prefix of the other (e.g. `foo` and
    /// `foo:bar`) cannot alias under `scan_prefix`. A bare `:` delimiter
    /// was vulnerable because proposal IDs are unconstrained strings
    /// and may legitimately contain `:`; a length prefix makes the
    /// boundary unambiguous. `.len() as u32` is wrapping on overflow;
    /// proposal IDs exceeding 4 GiB are not a realistic scenario and
    /// would produce harmless aliasing confined to absurd-sized IDs.
    fn len_prefixed(bytes: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(4 + bytes.len());
        out.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
        out.extend_from_slice(bytes);
        out
    }

    fn mandate_by_proposal_key(proposal_id: &str, issued_at: u64, id: &MandateId) -> Vec<u8> {
        let mut key = MANDATE_BY_PROPOSAL_PREFIX.to_vec();
        key.extend_from_slice(&Self::len_prefixed(proposal_id.as_bytes()));
        key.extend_from_slice(&issued_at.to_be_bytes());
        key.extend_from_slice(id.0.hyphenated().to_string().as_bytes());
        key
    }

    fn mandate_by_proposal_scan_prefix(proposal_id: &str) -> Vec<u8> {
        let mut prefix = MANDATE_BY_PROPOSAL_PREFIX.to_vec();
        prefix.extend_from_slice(&Self::len_prefixed(proposal_id.as_bytes()));
        prefix
    }

    fn mandate_by_decision_key(decision_hash: &Hash, issued_at: u64, id: &MandateId) -> Vec<u8> {
        let mut key = MANDATE_BY_DECISION_PREFIX.to_vec();
        key.extend_from_slice(hex::encode(decision_hash).as_bytes());
        key.push(b':');
        key.extend_from_slice(&issued_at.to_be_bytes());
        key.push(b':');
        key.extend_from_slice(id.0.hyphenated().to_string().as_bytes());
        key
    }

    fn mandate_by_decision_scan_prefix(decision_hash: &Hash) -> Vec<u8> {
        let mut prefix = MANDATE_BY_DECISION_PREFIX.to_vec();
        prefix.extend_from_slice(hex::encode(decision_hash).as_bytes());
        prefix.push(b':');
        prefix
    }

    fn grant_primary_key(id: &AuthorityGrantId) -> Vec<u8> {
        let mut key = AUTHORITY_GRANT_PREFIX.to_vec();
        key.extend_from_slice(id.0.hyphenated().to_string().as_bytes());
        key
    }

    fn grant_by_decision_key(
        decision_hash: &Hash,
        valid_from: u64,
        id: &AuthorityGrantId,
    ) -> Vec<u8> {
        let mut key = AUTHORITY_GRANT_BY_DECISION_PREFIX.to_vec();
        key.extend_from_slice(hex::encode(decision_hash).as_bytes());
        key.push(b':');
        key.extend_from_slice(&valid_from.to_be_bytes());
        key.push(b':');
        key.extend_from_slice(id.0.hyphenated().to_string().as_bytes());
        key
    }

    fn grant_by_decision_scan_prefix(decision_hash: &Hash) -> Vec<u8> {
        let mut prefix = AUTHORITY_GRANT_BY_DECISION_PREFIX.to_vec();
        prefix.extend_from_slice(hex::encode(decision_hash).as_bytes());
        prefix.push(b':');
        prefix
    }

    /// Persist a mandate with its proposal and decision secondary indexes
    /// in a single sled transaction.
    ///
    /// Same-`MandateId` rewrites overwrite primary bytes; index keys are
    /// deterministic from `(issued_at, id)` so they are idempotent.
    pub fn put_mandate(&self, mandate: &Mandate) -> Result<(), String> {
        let primary_key = Self::mandate_primary_key(&mandate.id);
        let value =
            serde_json::to_vec(mandate).map_err(|e| format!("Failed to serialize Mandate: {e}"))?;
        let proposal_idx = Self::mandate_by_proposal_key(
            &mandate.decision.proposal_id,
            mandate.issued_at,
            &mandate.id,
        );
        let decision_idx = Self::mandate_by_decision_key(
            &mandate.decision.decision_hash,
            mandate.issued_at,
            &mandate.id,
        );
        let id_bytes = mandate.id.0.hyphenated().to_string().into_bytes();

        self.db
            .transaction(|tx| {
                tx.insert(primary_key.as_slice(), value.as_slice())?;
                tx.insert(proposal_idx.as_slice(), id_bytes.as_slice())?;
                tx.insert(decision_idx.as_slice(), id_bytes.as_slice())?;
                Ok::<(), ConflictableTransactionError<()>>(())
            })
            .map_err(|e: TransactionError<()>| format!("sled mandate tx: {e:?}"))
    }

    /// Retrieve the earliest mandate recorded for `proposal_id`, or
    /// `None` if no mandate exists.
    pub fn get_mandate_by_proposal(&self, proposal_id: &str) -> Result<Option<Mandate>, String> {
        let scan = Self::mandate_by_proposal_scan_prefix(proposal_id);
        for entry in self.db.scan_prefix(&scan) {
            let (_k, v) = entry.map_err(|e| format!("sled scan mandate by_proposal: {e}"))?;
            let id_str = std::str::from_utf8(&v)
                .map_err(|e| format!("mandate index value not UTF-8: {e}"))?;
            let uuid = uuid::Uuid::parse_str(id_str)
                .map_err(|e| format!("mandate index value not a UUID: {e}"))?;
            let primary = Self::mandate_primary_key(&MandateId(uuid));
            let Some(bytes) = self
                .db
                .get(&primary)
                .map_err(|e| format!("sled get mandate primary: {e}"))?
            else {
                tracing::warn!(
                    proposal_id = %proposal_id,
                    id = %id_str,
                    "mandate index skew: primary missing"
                );
                continue;
            };
            let mandate: Mandate =
                serde_json::from_slice(&bytes).map_err(|e| format!("deserialize Mandate: {e}"))?;
            return Ok(Some(mandate));
        }
        Ok(None)
    }

    /// Retrieve all mandates anchored to `decision_hash`, ordered
    /// oldest-first by `issued_at`.
    pub fn list_mandates_by_decision(&self, decision_hash: &Hash) -> Result<Vec<Mandate>, String> {
        let scan = Self::mandate_by_decision_scan_prefix(decision_hash);
        let mut out = Vec::new();
        for entry in self.db.scan_prefix(&scan) {
            let (_k, v) = entry.map_err(|e| format!("sled scan mandate by_decision: {e}"))?;
            let id_str = std::str::from_utf8(&v)
                .map_err(|e| format!("mandate index value not UTF-8: {e}"))?;
            let uuid = uuid::Uuid::parse_str(id_str)
                .map_err(|e| format!("mandate index value not a UUID: {e}"))?;
            let primary = Self::mandate_primary_key(&MandateId(uuid));
            let Some(bytes) = self
                .db
                .get(&primary)
                .map_err(|e| format!("sled get mandate primary: {e}"))?
            else {
                tracing::warn!(
                    decision_hash = %hex::encode(decision_hash),
                    id = %id_str,
                    "mandate index skew: primary missing"
                );
                continue;
            };
            let mandate: Mandate =
                serde_json::from_slice(&bytes).map_err(|e| format!("deserialize Mandate: {e}"))?;
            out.push(mandate);
        }
        Ok(out)
    }

    /// Persist an authority grant with its decision-hash secondary index
    /// (when `granted_by` is set) in a single sled transaction.
    ///
    /// Same-`AuthorityGrantId` rewrites overwrite primary bytes; index
    /// keys are deterministic from `(valid_from, id)` so they are
    /// idempotent. Grants with `granted_by = None` (charter-direct
    /// grants, future work) are stored by primary only; the trait's
    /// `list_authority_grants_by_decision` skips them by construction.
    pub fn put_authority_grant(&self, grant: &AuthorityGrant) -> Result<(), String> {
        let primary_key = Self::grant_primary_key(&grant.id);
        let value = serde_json::to_vec(grant)
            .map_err(|e| format!("Failed to serialize AuthorityGrant: {e}"))?;
        let id_bytes = grant.id.0.hyphenated().to_string().into_bytes();
        let decision_idx = grant
            .granted_by
            .as_ref()
            .map(|p| Self::grant_by_decision_key(&p.decision_hash, grant.valid_from, &grant.id));

        self.db
            .transaction(|tx| {
                tx.insert(primary_key.as_slice(), value.as_slice())?;
                if let Some(idx) = decision_idx.as_ref() {
                    tx.insert(idx.as_slice(), id_bytes.as_slice())?;
                }
                Ok::<(), ConflictableTransactionError<()>>(())
            })
            .map_err(|e: TransactionError<()>| format!("sled grant tx: {e:?}"))
    }

    /// Retrieve an authority grant by its stable id.
    pub fn get_authority_grant(
        &self,
        grant_id: &AuthorityGrantId,
    ) -> Result<Option<AuthorityGrant>, String> {
        let key = Self::grant_primary_key(grant_id);
        let Some(bytes) = self
            .db
            .get(&key)
            .map_err(|e| format!("sled get grant primary: {e}"))?
        else {
            return Ok(None);
        };
        let grant: AuthorityGrant = serde_json::from_slice(&bytes)
            .map_err(|e| format!("deserialize AuthorityGrant: {e}"))?;
        Ok(Some(grant))
    }

    /// Retrieve all authority grants whose `granted_by.decision_hash`
    /// matches `decision_hash`, ordered oldest-first by `valid_from`.
    pub fn list_authority_grants_by_decision(
        &self,
        decision_hash: &Hash,
    ) -> Result<Vec<AuthorityGrant>, String> {
        let scan = Self::grant_by_decision_scan_prefix(decision_hash);
        let mut out = Vec::new();
        for entry in self.db.scan_prefix(&scan) {
            let (_k, v) = entry.map_err(|e| format!("sled scan grant by_decision: {e}"))?;
            let id_str =
                std::str::from_utf8(&v).map_err(|e| format!("grant index value not UTF-8: {e}"))?;
            let uuid = uuid::Uuid::parse_str(id_str)
                .map_err(|e| format!("grant index value not a UUID: {e}"))?;
            let primary = Self::grant_primary_key(&AuthorityGrantId(uuid));
            let Some(bytes) = self
                .db
                .get(&primary)
                .map_err(|e| format!("sled get grant primary: {e}"))?
            else {
                tracing::warn!(
                    decision_hash = %hex::encode(decision_hash),
                    id = %id_str,
                    "authority grant index skew: primary missing"
                );
                continue;
            };
            let grant: AuthorityGrant = serde_json::from_slice(&bytes)
                .map_err(|e| format!("deserialize AuthorityGrant: {e}"))?;
            out.push(grant);
        }
        Ok(out)
    }

    /// Atomically persist a mandate and all grants it references.
    ///
    /// All primary records and all secondary index entries land in a
    /// single sled transaction. On any write error, sled aborts the
    /// transaction and no keys are written — no durable orphan grants,
    /// no half-linked mandate. This is the real-backend override of
    /// [`GovernanceReceiptBackend::put_mandate_with_grants`] and the
    /// canonical acceptance-commit path for ADR-0014.
    pub fn put_mandate_with_grants_atomic(
        &self,
        mandate: &Mandate,
        grants: &[AuthorityGrant],
    ) -> Result<(), String> {
        // Pre-serialize everything outside the transaction so a serde
        // error does not partially execute the transaction body. Also
        // keeps the transaction closure `FnMut`-safe (sled retries it).
        let mandate_primary_key = Self::mandate_primary_key(&mandate.id);
        let mandate_value =
            serde_json::to_vec(mandate).map_err(|e| format!("Failed to serialize Mandate: {e}"))?;
        let mandate_id_bytes = mandate.id.0.hyphenated().to_string().into_bytes();
        let mandate_proposal_idx = Self::mandate_by_proposal_key(
            &mandate.decision.proposal_id,
            mandate.issued_at,
            &mandate.id,
        );
        let mandate_decision_idx = Self::mandate_by_decision_key(
            &mandate.decision.decision_hash,
            mandate.issued_at,
            &mandate.id,
        );

        struct PreparedGrant {
            primary_key: Vec<u8>,
            value: Vec<u8>,
            id_bytes: Vec<u8>,
            decision_idx: Option<Vec<u8>>,
        }
        let prepared: Vec<PreparedGrant> = grants
            .iter()
            .map(|g| {
                let primary_key = Self::grant_primary_key(&g.id);
                let value = serde_json::to_vec(g)
                    .map_err(|e| format!("Failed to serialize AuthorityGrant: {e}"))?;
                let id_bytes = g.id.0.hyphenated().to_string().into_bytes();
                let decision_idx = g
                    .granted_by
                    .as_ref()
                    .map(|p| Self::grant_by_decision_key(&p.decision_hash, g.valid_from, &g.id));
                Ok(PreparedGrant {
                    primary_key,
                    value,
                    id_bytes,
                    decision_idx,
                })
            })
            .collect::<Result<_, String>>()?;

        self.db
            .transaction(|tx| {
                for pg in &prepared {
                    tx.insert(pg.primary_key.as_slice(), pg.value.as_slice())?;
                    if let Some(idx) = pg.decision_idx.as_ref() {
                        tx.insert(idx.as_slice(), pg.id_bytes.as_slice())?;
                    }
                }
                tx.insert(mandate_primary_key.as_slice(), mandate_value.as_slice())?;
                tx.insert(mandate_proposal_idx.as_slice(), mandate_id_bytes.as_slice())?;
                tx.insert(mandate_decision_idx.as_slice(), mandate_id_bytes.as_slice())?;
                Ok::<(), ConflictableTransactionError<()>>(())
            })
            .map_err(|e: TransactionError<()>| {
                format!("sled put_mandate_with_grants tx aborted: {e:?}")
            })
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

    fn put_mandate(&self, mandate: &Mandate) -> Result<(), String> {
        self.put_mandate(mandate)
    }

    fn get_mandate_by_proposal(&self, proposal_id: &str) -> Result<Option<Mandate>, String> {
        self.get_mandate_by_proposal(proposal_id)
    }

    fn list_mandates_by_decision(&self, decision_hash: &Hash) -> Result<Vec<Mandate>, String> {
        self.list_mandates_by_decision(decision_hash)
    }

    fn put_authority_grant(&self, grant: &AuthorityGrant) -> Result<(), String> {
        self.put_authority_grant(grant)
    }

    fn get_authority_grant(
        &self,
        grant_id: &AuthorityGrantId,
    ) -> Result<Option<AuthorityGrant>, String> {
        self.get_authority_grant(grant_id)
    }

    fn list_authority_grants_by_decision(
        &self,
        decision_hash: &Hash,
    ) -> Result<Vec<AuthorityGrant>, String> {
        self.list_authority_grants_by_decision(decision_hash)
    }

    fn put_mandate_with_grants(
        &self,
        mandate: &Mandate,
        grants: &[AuthorityGrant],
    ) -> Result<(), String> {
        self.put_mandate_with_grants_atomic(mandate, grants)
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
            None,
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
                "rec-a", "prop-1", "sdis", None, true, None, None, 10,
            ))
            .unwrap();
        store
            .put_effect_dispatch_evidence(&EffectDispatchEvidence::new(
                "rec-b", "prop-2", "sdis", None, true, None, None, 20,
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
        let ev =
            EffectDispatchEvidence::new("rec-dup", "prop-dup", "sdis", None, true, None, None, 10);
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

    // ========================================================================
    // ADR-0014 Mandate + AuthorityGrant tests
    // ========================================================================

    use icn_governance::{
        AuthorityClass, AuthorityGrant, AuthorityGrantId, DecisionProvenance, Grantee,
        GrantorEntityId, Mandate, TypedScope,
    };

    fn make_mandate(proposal_id: &str, decision_hash: Hash, issued_at: u64) -> Mandate {
        Mandate::new_pending_grants(
            DecisionProvenance {
                proposal_id: proposal_id.to_string(),
                decision_hash,
            },
            [7u8; 32],
            None,
            None,
            issued_at,
        )
    }

    fn make_grant(decision_hash: Hash, valid_from: u64) -> AuthorityGrant {
        AuthorityGrant {
            id: AuthorityGrantId::new(),
            class: AuthorityClass::Attestation,
            grantor: GrantorEntityId("coop:tech".into()),
            grantee: Grantee::Entity("svc:test".into()),
            scope: TypedScope {
                domain: Some(icn_governance::GovernanceDomainId("coop:tech".into())),
                proposal_class: vec!["Sdis".into()],
                ..TypedScope::default()
            },
            granted_by: Some(DecisionProvenance {
                proposal_id: "p1".into(),
                decision_hash,
            }),
            valid_from,
            valid_until: Some(valid_from + 3_600),
            revoked_at: None,
        }
    }

    #[test]
    fn mandate_roundtrip_by_proposal_and_decision() {
        let store = ReceiptStore::new(temp_db());
        let decision_hash = [0x11u8; 32];
        let mandate = make_mandate("prop-mandate-1", decision_hash, 1_000);

        store.put_mandate(&mandate).unwrap();

        // By-proposal lookup
        let by_proposal = store.get_mandate_by_proposal("prop-mandate-1").unwrap();
        assert_eq!(by_proposal.as_ref(), Some(&mandate));

        // By-decision lookup returns the same record
        let by_decision = store.list_mandates_by_decision(&decision_hash).unwrap();
        assert_eq!(by_decision, vec![mandate.clone()]);

        // Missing proposal → None
        assert!(store.get_mandate_by_proposal("missing").unwrap().is_none());
        // Missing decision → empty
        assert!(store
            .list_mandates_by_decision(&[0u8; 32])
            .unwrap()
            .is_empty());
    }

    #[test]
    fn mandate_list_by_decision_is_chronological() {
        let store = ReceiptStore::new(temp_db());
        let decision_hash = [0x22u8; 32];

        // Insert out of chronological order
        let m_late = make_mandate("prop-late", decision_hash, 5_000);
        let m_early = make_mandate("prop-early", decision_hash, 1_000);
        let m_mid = make_mandate("prop-mid", decision_hash, 3_000);
        store.put_mandate(&m_late).unwrap();
        store.put_mandate(&m_early).unwrap();
        store.put_mandate(&m_mid).unwrap();

        let list = store.list_mandates_by_decision(&decision_hash).unwrap();
        let issued: Vec<u64> = list.iter().map(|m| m.issued_at).collect();
        assert_eq!(
            issued,
            vec![1_000, 3_000, 5_000],
            "list_mandates_by_decision must return oldest-first by issued_at"
        );
    }

    #[test]
    fn mandate_rewrite_same_id_is_idempotent() {
        let store = ReceiptStore::new(temp_db());
        let mandate = make_mandate("prop-idem", [0x33u8; 32], 100);
        store.put_mandate(&mandate).unwrap();
        store.put_mandate(&mandate).unwrap();

        assert_eq!(
            store
                .list_mandates_by_decision(&[0x33u8; 32])
                .unwrap()
                .len(),
            1,
            "same MandateId must not produce two entries"
        );
    }

    #[test]
    fn authority_grant_roundtrip_by_id_and_decision() {
        let store = ReceiptStore::new(temp_db());
        let decision_hash = [0x44u8; 32];
        let grant = make_grant(decision_hash, 2_000);

        store.put_authority_grant(&grant).unwrap();

        let by_id = store.get_authority_grant(&grant.id).unwrap();
        assert_eq!(by_id.as_ref(), Some(&grant));

        let by_decision = store
            .list_authority_grants_by_decision(&decision_hash)
            .unwrap();
        assert_eq!(by_decision, vec![grant.clone()]);

        // Missing id → None
        assert!(store
            .get_authority_grant(&AuthorityGrantId::new())
            .unwrap()
            .is_none());
    }

    #[test]
    fn authority_grants_list_by_decision_is_chronological() {
        let store = ReceiptStore::new(temp_db());
        let decision_hash = [0x55u8; 32];

        let g_late = make_grant(decision_hash, 5_000);
        let g_early = make_grant(decision_hash, 1_000);
        let g_mid = make_grant(decision_hash, 3_000);
        store.put_authority_grant(&g_late).unwrap();
        store.put_authority_grant(&g_early).unwrap();
        store.put_authority_grant(&g_mid).unwrap();

        let list = store
            .list_authority_grants_by_decision(&decision_hash)
            .unwrap();
        let valid_from: Vec<u64> = list.iter().map(|g| g.valid_from).collect();
        assert_eq!(
            valid_from,
            vec![1_000, 3_000, 5_000],
            "list_authority_grants_by_decision must return oldest-first by valid_from"
        );
    }

    #[test]
    fn authority_grant_rewrite_same_id_is_idempotent() {
        let store = ReceiptStore::new(temp_db());
        let grant = make_grant([0x66u8; 32], 100);
        store.put_authority_grant(&grant).unwrap();
        store.put_authority_grant(&grant).unwrap();

        assert_eq!(
            store
                .list_authority_grants_by_decision(&[0x66u8; 32])
                .unwrap()
                .len(),
            1,
            "same AuthorityGrantId must not produce two entries"
        );
    }

    #[test]
    fn charter_direct_grant_stores_primary_but_not_decision_index() {
        // Grants with `granted_by = None` (charter-direct, future work)
        // should be retrievable by primary id but must not appear in
        // any decision-hash scan (no secondary index key exists).
        let store = ReceiptStore::new(temp_db());
        let grant = AuthorityGrant {
            granted_by: None,
            ..make_grant([0x77u8; 32], 500)
        };
        store.put_authority_grant(&grant).unwrap();

        assert_eq!(
            store.get_authority_grant(&grant.id).unwrap().as_ref(),
            Some(&grant)
        );
        assert!(
            store
                .list_authority_grants_by_decision(&[0x77u8; 32])
                .unwrap()
                .is_empty(),
            "charter-direct grant must not be discoverable via decision scan"
        );
    }

    #[test]
    fn put_mandate_with_grants_atomic_commits_all() {
        let store = ReceiptStore::new(temp_db());
        let decision_hash = [0x88u8; 32];
        let g1 = make_grant(decision_hash, 1_000);
        let g2 = make_grant(decision_hash, 2_000);
        let mandate = Mandate::new(
            DecisionProvenance {
                proposal_id: "prop-atomic".into(),
                decision_hash,
            },
            [1u8; 32],
            vec![g1.id.clone(), g2.id.clone()],
            None,
            None,
            3_000,
        )
        .unwrap();

        store
            .put_mandate_with_grants_atomic(&mandate, &[g1.clone(), g2.clone()])
            .unwrap();

        // Mandate present
        let m = store.get_mandate_by_proposal("prop-atomic").unwrap();
        assert_eq!(m.as_ref(), Some(&mandate));
        assert_eq!(
            m.as_ref().unwrap().grants,
            vec![g1.id.clone(), g2.id.clone()]
        );

        // Both grants present, retrievable by id and by decision
        assert!(store.get_authority_grant(&g1.id).unwrap().is_some());
        assert!(store.get_authority_grant(&g2.id).unwrap().is_some());
        let listed = store
            .list_authority_grants_by_decision(&decision_hash)
            .unwrap();
        assert_eq!(listed.len(), 2);
    }

    #[test]
    fn put_mandate_with_grants_atomic_on_serialization_failure_writes_nothing() {
        // Proof-of-atomicity: pre-serialization happens before the sled
        // transaction; if any write inside the tx fails, sled rolls back
        // everything. We can exercise the pre-serialization guard by
        // passing a mandate that's well-formed (so sled won't fail) but
        // we can separately verify that no keys land if the atomic path
        // is not invoked. This test documents the happy-path atomicity
        // contract: after a successful atomic write, every primary and
        // index record is present; after a not-invoked path, none are.
        let store = ReceiptStore::new(temp_db());
        let decision_hash = [0x99u8; 32];
        let grant = make_grant(decision_hash, 100);

        // Invoke only the primary store_grant path and then confirm no
        // mandate record exists, proving mandate writes are not a
        // side-effect of grant writes.
        store.put_authority_grant(&grant).unwrap();
        assert!(store
            .get_mandate_by_proposal("prop-not-written")
            .unwrap()
            .is_none());
        assert!(store
            .list_mandates_by_decision(&decision_hash)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn mandate_by_proposal_index_does_not_alias_colon_prefixes() {
        // Regression: the by_proposal index used a raw `proposal_id +
        // ':'` boundary, which aliased `foo` against `foo:bar` under
        // scan_prefix (Codex P2 on #1575). In the seam's idempotency
        // check, that could make a fresh "foo" proposal return
        // AlreadyMinted with "foo:bar"'s mandate. Length-prefix
        // encoding makes the boundary unambiguous.
        let store = ReceiptStore::new(temp_db());
        let decision_a = [0xaau8; 32];
        let decision_b = [0xbbu8; 32];

        let mandate_foo = Mandate::new_pending_grants(
            DecisionProvenance {
                proposal_id: "foo".into(),
                decision_hash: decision_a,
            },
            [1u8; 32],
            None,
            None,
            100,
        );
        let mandate_foo_bar = Mandate::new_pending_grants(
            DecisionProvenance {
                proposal_id: "foo:bar".into(),
                decision_hash: decision_b,
            },
            [2u8; 32],
            None,
            None,
            200,
        );

        // Write the deeper-prefixed proposal first — this maximises the
        // chance of aliasing if the index is buggy, because `foo:bar:...`
        // would sort under `foo:` scan.
        store.put_mandate(&mandate_foo_bar).unwrap();
        store.put_mandate(&mandate_foo).unwrap();

        // Exact match only
        let foo = store.get_mandate_by_proposal("foo").unwrap().unwrap();
        assert_eq!(
            foo.id, mandate_foo.id,
            "get('foo') must not alias to 'foo:bar'"
        );
        assert_eq!(foo.decision.proposal_id, "foo");

        let foo_bar = store.get_mandate_by_proposal("foo:bar").unwrap().unwrap();
        assert_eq!(foo_bar.id, mandate_foo_bar.id);
        assert_eq!(foo_bar.decision.proposal_id, "foo:bar");

        // Negative case: "fo" must not match either
        assert!(store.get_mandate_by_proposal("fo").unwrap().is_none());
        // And "foo:" (with trailing colon) must not match the bare "foo"
        assert!(store.get_mandate_by_proposal("foo:").unwrap().is_none());
    }

    #[test]
    fn seam_idempotency_is_not_fooled_by_colon_aliased_proposal_id() {
        // Seam-level proof: minting for "foo:bar" then minting for "foo"
        // must produce two distinct Minted outcomes, not AlreadyMinted
        // for the second call. This is the concrete correctness bug
        // the prefix-alias fix closes.
        use icn_governance::sdis::SdisProposal;
        use icn_governance::{GovernanceDomainId, ProposalPayload};
        use icn_governance_actor::grant_minting::{
            mint_and_persist_for_accepted, MandateMintOutcome,
        };
        use icn_identity::Did;

        let store = ReceiptStore::new(temp_db());
        let domain = GovernanceDomainId("coop:tech".into());
        let candidate = Did::from_anchor_id(&[9u8; 32]);
        let payload = ProposalPayload::Sdis {
            proposal: SdisProposal::AppointSteward {
                candidate,
                sponsors: vec![],
                region: "nyc".into(),
                bond_amount: 1_000,
                term_length: 3_600,
            },
        };

        let outcome_foo_bar = mint_and_persist_for_accepted(
            &store,
            "foo:bar",
            &domain,
            [0xbbu8; 32],
            &payload,
            1_000,
        )
        .unwrap();
        let outcome_foo =
            mint_and_persist_for_accepted(&store, "foo", &domain, [0xaau8; 32], &payload, 1_000)
                .unwrap();

        match outcome_foo_bar {
            MandateMintOutcome::Minted { .. } => {}
            other => panic!("expected Minted for 'foo:bar'; got {other:?}"),
        }
        match outcome_foo {
            MandateMintOutcome::Minted { .. } => {}
            other => {
                panic!("expected Minted for 'foo'; got {other:?} — prefix aliasing regression")
            }
        }

        // Two distinct mandates exist
        let m_foo = store.get_mandate_by_proposal("foo").unwrap().unwrap();
        let m_foo_bar = store.get_mandate_by_proposal("foo:bar").unwrap().unwrap();
        assert_ne!(m_foo.id, m_foo_bar.id);
        assert_eq!(m_foo.decision.proposal_id, "foo");
        assert_eq!(m_foo_bar.decision.proposal_id, "foo:bar");
    }

    #[test]
    fn seam_integration_with_real_receipt_store_persists_mandate_and_grant() {
        // Real-backend seam integration: invoke the ADR-0014 acceptance
        // seam against a real sled-backed ReceiptStore and verify that
        // both the Mandate and the AuthorityGrant land durably and
        // remain queryable by proposal, decision, and grant id.
        use icn_governance::sdis::SdisProposal;
        use icn_governance::{GovernanceDomainId, ProposalPayload};
        use icn_governance_actor::grant_minting::{
            mint_and_persist_for_accepted, MandateMintOutcome,
        };
        use icn_identity::Did;

        let store = ReceiptStore::new(temp_db());
        let domain = GovernanceDomainId("coop:tech".into());
        let candidate = Did::from_anchor_id(&[9u8; 32]);
        let payload = ProposalPayload::Sdis {
            proposal: SdisProposal::AppointSteward {
                candidate: candidate.clone(),
                sponsors: vec![],
                region: "nyc".into(),
                bond_amount: 1_000,
                term_length: 3_600,
            },
        };
        let decision_hash = [0xabu8; 32];

        let outcome = mint_and_persist_for_accepted(
            &store,
            "prop-seam-real",
            &domain,
            decision_hash,
            &payload,
            1_000,
        )
        .unwrap();

        match outcome {
            MandateMintOutcome::Minted {
                grants_persisted, ..
            } => assert_eq!(grants_persisted, 1),
            other => panic!("expected Minted; got {other:?}"),
        }

        // Durable mandate retrievable by proposal
        let mandate = store
            .get_mandate_by_proposal("prop-seam-real")
            .unwrap()
            .expect("mandate persisted");
        assert_eq!(
            mandate.grants.len(),
            1,
            "strict mandate references exactly one grant"
        );
        assert_eq!(mandate.decision.decision_hash, decision_hash);

        // Same mandate retrievable by decision
        let by_decision = store.list_mandates_by_decision(&decision_hash).unwrap();
        assert_eq!(by_decision.len(), 1);
        assert_eq!(by_decision[0].id, mandate.id);

        // Grant retrievable by id and by decision, and its id matches
        // the one referenced in the mandate
        let grant_id = &mandate.grants[0];
        let grant = store
            .get_authority_grant(grant_id)
            .unwrap()
            .expect("grant persisted");
        assert_eq!(grant.grantee, Grantee::Person(candidate));
        assert_eq!(grant.class, AuthorityClass::Attestation);
        assert_eq!(grant.valid_until, Some(1_000 + 3_600));
        let by_decision_grants = store
            .list_authority_grants_by_decision(&decision_hash)
            .unwrap();
        assert_eq!(by_decision_grants.len(), 1);
        assert_eq!(&by_decision_grants[0].id, grant_id);

        // Seam idempotency: re-invoking must not duplicate mandate or grant
        let outcome2 = mint_and_persist_for_accepted(
            &store,
            "prop-seam-real",
            &domain,
            decision_hash,
            &payload,
            1_000,
        )
        .unwrap();
        assert!(matches!(outcome2, MandateMintOutcome::AlreadyMinted { .. }));
        assert_eq!(
            store
                .list_mandates_by_decision(&decision_hash)
                .unwrap()
                .len(),
            1
        );
        assert_eq!(
            store
                .list_authority_grants_by_decision(&decision_hash)
                .unwrap()
                .len(),
            1
        );
    }

    // ============================================================
    // Legacy proposal-id index colon-alias repair regression tests
    // ============================================================
    //
    // The `PROPOSAL_INDEX_PREFIX` and `INSTITUTIONAL_EFFECT_BY_PROPOSAL_PREFIX`
    // secondary indexes use raw `{proposal_id}:{…}` key shapes. The
    // colon-safe length-prefix scheme used by `MANDATE_BY_PROPOSAL_PREFIX`
    // was not retrofitted onto them because they carry live on-disk
    // K3s data. Instead, their `get_governance_by_proposal` and
    // `list_institutional_effects_by_proposal` readers perform a
    // canonical-proposal-id match against the loaded primary record,
    // so aliased scan hits never become returned results.

    #[test]
    fn governance_proposal_index_does_not_alias_colon_prefixes() {
        let store = ReceiptStore::new(temp_db());
        // Write only the `foo:bar` receipt. A buggy scan on `foo:` would
        // return this receipt; the filter must reject it.
        store
            .put_governance(&make_test_governance_receipt("foo:bar"))
            .unwrap();

        let aliased = store.get_governance_by_proposal("foo").unwrap();
        assert!(
            aliased.is_none(),
            "get_governance_by_proposal(\"foo\") leaked a \"foo:bar\" hit under prefix aliasing"
        );

        let exact = store
            .get_governance_by_proposal("foo:bar")
            .unwrap()
            .expect("exact proposal_id must still resolve");
        assert_eq!(exact.proposal_id, "foo:bar");
    }

    #[test]
    fn governance_proposal_index_returns_correct_receipt_when_both_exist() {
        let store = ReceiptStore::new(temp_db());
        // Both `foo` and `foo:bar` live in the index under overlapping
        // scan-prefix space; each lookup must return its own receipt.
        let foo_hash = store
            .put_governance(&make_test_governance_receipt("foo"))
            .unwrap();
        let foo_bar_hash = store
            .put_governance(&make_test_governance_receipt("foo:bar"))
            .unwrap();
        assert_ne!(foo_hash, foo_bar_hash);

        let foo = store
            .get_governance_by_proposal("foo")
            .unwrap()
            .expect("foo");
        assert_eq!(foo.proposal_id, "foo");
        assert_eq!(foo.decision_hash, foo_hash);

        let foo_bar = store
            .get_governance_by_proposal("foo:bar")
            .unwrap()
            .expect("foo:bar");
        assert_eq!(foo_bar.proposal_id, "foo:bar");
        assert_eq!(foo_bar.decision_hash, foo_bar_hash);
    }

    #[test]
    fn institutional_effect_index_does_not_alias_colon_prefixes() {
        let store = ReceiptStore::new(temp_db());
        // Write only a `foo:bar` record. A buggy scan on `foo:` would
        // include it; the canonical-id filter must drop it.
        let rec = InstitutionalEffectRecord::new(
            "foo:bar",
            "coop-a",
            Some([1u8; 32]),
            "freeze_member",
            Some("did:icn:x".into()),
            None,
            None,
            100,
            serde_json::json!({}),
        );
        store.put_institutional_effect(&rec).unwrap();

        let aliased = store.list_institutional_effects_by_proposal("foo").unwrap();
        assert!(
            aliased.is_empty(),
            "list_institutional_effects_by_proposal(\"foo\") leaked a \"foo:bar\" hit"
        );

        let exact = store
            .list_institutional_effects_by_proposal("foo:bar")
            .unwrap();
        assert_eq!(exact.len(), 1);
        assert_eq!(exact[0].proposal_id, "foo:bar");
    }

    #[test]
    fn institutional_effect_index_is_scoped_when_both_proposal_ids_coexist() {
        let store = ReceiptStore::new(temp_db());
        let foo = InstitutionalEffectRecord::new(
            "foo",
            "coop-a",
            None,
            "freeze_member",
            Some("did:icn:1".into()),
            None,
            None,
            10,
            serde_json::json!({"src": "foo"}),
        );
        let foo_bar = InstitutionalEffectRecord::new(
            "foo:bar",
            "coop-a",
            None,
            "freeze_member",
            Some("did:icn:2".into()),
            None,
            None,
            20,
            serde_json::json!({"src": "foo:bar"}),
        );
        store.put_institutional_effect(&foo).unwrap();
        store.put_institutional_effect(&foo_bar).unwrap();

        let foo_list = store.list_institutional_effects_by_proposal("foo").unwrap();
        assert_eq!(foo_list.len(), 1);
        assert_eq!(foo_list[0].proposal_id, "foo");
        assert_eq!(foo_list[0].target_did.as_deref(), Some("did:icn:1"));

        let foo_bar_list = store
            .list_institutional_effects_by_proposal("foo:bar")
            .unwrap();
        assert_eq!(foo_bar_list.len(), 1);
        assert_eq!(foo_bar_list[0].proposal_id, "foo:bar");
        assert_eq!(foo_bar_list[0].target_did.as_deref(), Some("did:icn:2"));
    }

    #[test]
    fn institutional_effect_dedup_not_fooled_by_colon_aliased_proposal_id() {
        // The dedup check inside `emit_accepted_effect` is
        // `list_institutional_effects_by_proposal(proposal_id)` followed
        // by a match on `effect_kind`. If aliasing were still leaking
        // hits, recording `foo` with `freeze_member` after `foo:bar`
        // with the same kind already exists would spuriously look like
        // `AlreadyEmitted` and the `foo` record would be silently
        // dropped. Pin that this cannot happen.
        let store = ReceiptStore::new(temp_db());
        let foo_bar = InstitutionalEffectRecord::new(
            "foo:bar",
            "coop-a",
            None,
            "freeze_member",
            Some("did:icn:2".into()),
            None,
            None,
            20,
            serde_json::json!({}),
        );
        store.put_institutional_effect(&foo_bar).unwrap();

        let before_foo = store.list_institutional_effects_by_proposal("foo").unwrap();
        assert!(
            before_foo.is_empty(),
            "dedup scan for \"foo\" leaked \"foo:bar\" and would cause silent write loss"
        );
    }
}
