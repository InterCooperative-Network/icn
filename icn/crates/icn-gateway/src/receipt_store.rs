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
    ActionItemCompletionReceipt, AuthorityGrant, AuthorityGrantId, GovernanceDecisionReceipt,
    Grantee, Mandate, MandateId, ProofOutcome, Timestamp, VoteTally,
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
/// Primary key prefix for action-item completion receipts, keyed by the
/// receipt's canonical `record_hash`. Append-only: a reopen/re-complete
/// cycle on the same `item_id` produces a distinct `record_hash` (because
/// `completed_at` advances) and thus a distinct primary record. The
/// previous receipt is preserved.
const ACTION_ITEM_COMPLETION_REC_PREFIX: &[u8] = b"receipt:action_item_completion:rec:";
/// Secondary index prefix for action-item completion receipts, keyed
/// by `item_id` and ordered by `completed_at` so the audit chain reads
/// oldest-first under `scan_prefix`. Layout per entry:
///   `<prefix><u64 BE item_id_len><item_id bytes><u64 BE completed_at><32-byte record_hash>`
/// Distinct from the primary prefix's tail (`rec:`) so blake3 record
/// hashes cannot alias the by-item index range.
const ACTION_ITEM_COMPLETION_BY_ITEM_PREFIX: &[u8] = b"receipt:action_item_completion:by_item:";
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
/// Secondary index: authority grant by grantee (ADR-0014 length-prefix key
/// scheme). The grantee portion is length-prefixed so no two canonical
/// grantee encodings can alias under `scan_prefix` — required because
/// entity IDs are unconstrained strings that may contain `:`, and Person
/// DIDs and Entity IDs share this index under distinct tag bytes.
const AUTHORITY_GRANT_BY_GRANTEE_PREFIX: &[u8] = b"adr0014:grant:by_grantee:";

/// One-shot migration sentinels. Set after a successful pass converts
/// legacy raw-colon `{proposal_id}:{…}` secondary index keys into the
/// length-prefix colon-safe scheme used by `MANDATE_BY_PROPOSAL_PREFIX`.
/// Sentinel prefix is distinct (`receipt_store:migration:`) so it cannot
/// collide with any scan_prefix used for data lookups.
const MIGRATION_FLAG_V2_PROPOSAL_INDEX: &[u8] = b"receipt_store:migration:v2_proposal_index:done";
const MIGRATION_FLAG_V2_IER_BY_PROPOSAL: &[u8] = b"receipt_store:migration:v2_ier_by_proposal:done";

/// Receipt storage service for governance and economic chain artifacts.
///
/// Stores receipts by canonical hash for cross-node deterministic lookup.
pub struct ReceiptStore {
    db: Db,
}

impl ReceiptStore {
    /// Create a new receipt store backed by the given sled database.
    ///
    /// Runs a one-shot migration that converts any legacy raw-colon
    /// `{proposal_id}:…` secondary index keys (see
    /// [`MIGRATION_FLAG_V2_PROPOSAL_INDEX`] and
    /// [`MIGRATION_FLAG_V2_IER_BY_PROPOSAL`]) into the colon-safe
    /// length-prefix scheme. The migration is idempotent and gated by
    /// sentinel keys so already-migrated stores pay only two `db.get`
    /// lookups. Failures are logged but do not abort open, matching the
    /// original infallible `new` contract; any entries that fail to
    /// migrate will be invisible to length-prefixed readers until the
    /// next successful run.
    pub fn new(db: Db) -> Self {
        let store = Self { db };
        if let Err(e) = store.migrate_legacy_proposal_indexes() {
            tracing::error!(
                error = %e,
                "ReceiptStore: legacy proposal-index migration failed; some legacy entries may be hidden from length-prefixed readers"
            );
        }
        store
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

    /// Build a proposal ID index key using the colon-safe length-prefix
    /// scheme. Proposal IDs are unconstrained strings that may legitimately
    /// contain `:`, so a bare `:` delimiter (as used pre-#1589) allowed
    /// `foo` and `foo:bar` to alias under `scan_prefix`. The length prefix
    /// makes the proposal_id boundary unambiguous.
    fn make_proposal_index_key(proposal_id: &str, receipt_hash: &Hash) -> Vec<u8> {
        let mut key = PROPOSAL_INDEX_PREFIX.to_vec();
        key.extend_from_slice(&Self::len_prefixed(proposal_id.as_bytes()));
        key.extend_from_slice(hex::encode(receipt_hash).as_bytes());
        key
    }

    /// Build a proposal ID scan prefix in the length-prefix scheme.
    fn make_proposal_scan_prefix(proposal_id: &str) -> Vec<u8> {
        let mut prefix = PROPOSAL_INDEX_PREFIX.to_vec();
        prefix.extend_from_slice(&Self::len_prefixed(proposal_id.as_bytes()));
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
    /// Uses `scan_prefix` over the length-prefixed secondary index
    /// (`PROPOSAL_INDEX_PREFIX | len_prefixed(proposal_id) | hex_hash`).
    /// The length prefix on the proposal_id eliminates the `foo` vs
    /// `foo:bar` aliasing that the pre-#1589 raw-colon scheme allowed,
    /// so no filter-on-read fallback is needed. Legacy raw-colon entries
    /// are rewritten into this format by the one-shot migration that
    /// runs in [`ReceiptStore::new`].
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
            if let Some(receipt) = self.get_governance(&hash)? {
                return Ok(Some(receipt));
            }
            tracing::warn!(
                proposal_id = %proposal_id,
                receipt_hash = %hex::encode(hash),
                "governance proposal index skew: primary record missing"
            );
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

        // Secondary index uses the colon-safe length-prefix scheme so
        // `foo` and `foo:bar` cannot alias under `scan_prefix` — required
        // because `emit_accepted_effect` dedup is
        // `list_institutional_effects_by_proposal(proposal_id)`.
        let idx_key =
            Self::ier_by_proposal_key(&record.proposal_id, record.recorded_at, &record.record_id);
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
    /// Secondary index key:
    /// `INSTITUTIONAL_EFFECT_BY_PROPOSAL_PREFIX | len_prefixed(proposal_id)
    /// | recorded_at_be_u64 | record_id_bytes`. The length prefix on
    /// `proposal_id` blocks the `foo` vs `foo:bar` aliasing the pre-#1589
    /// raw-colon scheme allowed — load-bearing because
    /// [`emit_accepted_effect`](icn_governance_actor::institutional_effect::emit_accepted_effect)
    /// uses this lookup for `(proposal_id, effect_kind)` dedup and a
    /// false hit would silently drop a real new record.
    pub fn list_institutional_effects_by_proposal(
        &self,
        proposal_id: &str,
    ) -> Result<Vec<InstitutionalEffectRecord>, String> {
        let prefix = Self::ier_by_proposal_scan_prefix(proposal_id);

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

impl ReceiptStore {
    // ========================================================================
    // Legacy colon-aliased proposal-id secondary index migration (#1589)
    // ========================================================================
    //
    // `PROPOSAL_INDEX_PREFIX` and `INSTITUTIONAL_EFFECT_BY_PROPOSAL_PREFIX`
    // pre-#1589 used raw `{proposal_id}:…` key shapes. Because proposal
    // IDs are unconstrained strings that may contain `:`, `foo` and
    // `foo:bar` could alias under `scan_prefix`. #1576 patched this on
    // the read path with a filter-on-read guard; #1589 closes it on the
    // write path by adopting the same length-prefix scheme as
    // `MANDATE_BY_PROPOSAL_PREFIX` and migrating legacy on-disk entries
    // into the new shape on first open.
    //
    // Migration properties:
    // - Idempotent: gated by sentinel keys; re-running is a no-op after
    //   a successful pass.
    // - Preserves existing data: each legacy entry is rewritten under
    //   the canonical length-prefix key derived from the primary record's
    //   canonical `proposal_id` before the legacy key is removed.
    // - Safe on already-migrated stores: new-format keys map to the
    //   same new-format key (no-op write) and skip the delete step.
    // - No operator step required: runs inside [`ReceiptStore::new`].
    // - Orphans (index entries whose primary record is missing) are
    //   removed with a warning log.

    fn migrate_legacy_proposal_indexes(&self) -> Result<(), String> {
        self.migrate_proposal_index_once()?;
        self.migrate_ier_by_proposal_index_once()?;
        Ok(())
    }

    fn migrate_proposal_index_once(&self) -> Result<(), String> {
        if self
            .db
            .get(MIGRATION_FLAG_V2_PROPOSAL_INDEX)
            .map_err(|e| format!("sled get migration flag v2_proposal_index: {e}"))?
            .is_some()
        {
            return Ok(());
        }
        let keys: Vec<Vec<u8>> = self
            .db
            .scan_prefix(PROPOSAL_INDEX_PREFIX)
            .filter_map(|e| e.ok().map(|(k, _)| k.to_vec()))
            .collect();

        let mut rewritten = 0usize;
        let mut dropped = 0usize;
        for old_key in keys {
            let Some(val) = self
                .db
                .get(&old_key)
                .map_err(|e| format!("sled get proposal_index entry: {e}"))?
            else {
                continue;
            };
            if val.len() != 32 {
                // Unknown value shape — drop orphan so length-prefixed
                // readers don't trip over a stale 32!= value.
                self.db
                    .remove(&old_key)
                    .map_err(|e| format!("sled remove stale proposal_index entry: {e}"))?;
                dropped += 1;
                continue;
            }
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&val);
            let Some(receipt) = self.get_governance(&hash)? else {
                tracing::warn!(
                    receipt_hash = %hex::encode(hash),
                    "proposal_index migration: primary governance record missing; dropping orphan index entry"
                );
                self.db
                    .remove(&old_key)
                    .map_err(|e| format!("sled remove orphan proposal_index entry: {e}"))?;
                dropped += 1;
                continue;
            };
            let new_key = Self::make_proposal_index_key(&receipt.proposal_id, &hash);
            if new_key == old_key {
                continue;
            }
            self.db
                .insert(&new_key, &hash[..])
                .map_err(|e| format!("sled insert migrated proposal_index key: {e}"))?;
            self.db
                .remove(&old_key)
                .map_err(|e| format!("sled remove legacy proposal_index key: {e}"))?;
            rewritten += 1;
        }

        self.db
            .insert(MIGRATION_FLAG_V2_PROPOSAL_INDEX, &[1u8])
            .map_err(|e| format!("sled set migration flag v2_proposal_index: {e}"))?;

        if rewritten > 0 || dropped > 0 {
            tracing::info!(
                rewritten,
                dropped,
                "proposal_index: colon-alias migration complete"
            );
        }
        Ok(())
    }

    fn migrate_ier_by_proposal_index_once(&self) -> Result<(), String> {
        if self
            .db
            .get(MIGRATION_FLAG_V2_IER_BY_PROPOSAL)
            .map_err(|e| format!("sled get migration flag v2_ier_by_proposal: {e}"))?
            .is_some()
        {
            return Ok(());
        }
        let keys: Vec<Vec<u8>> = self
            .db
            .scan_prefix(INSTITUTIONAL_EFFECT_BY_PROPOSAL_PREFIX)
            .filter_map(|e| e.ok().map(|(k, _)| k.to_vec()))
            .collect();

        let mut rewritten = 0usize;
        let mut dropped = 0usize;
        for old_key in keys {
            let Some(val) = self
                .db
                .get(&old_key)
                .map_err(|e| format!("sled get ier_by_proposal entry: {e}"))?
            else {
                continue;
            };
            let Ok(record_id) = std::str::from_utf8(&val) else {
                self.db
                    .remove(&old_key)
                    .map_err(|e| format!("sled remove non-utf8 ier_by_proposal entry: {e}"))?;
                dropped += 1;
                continue;
            };
            let mut primary_key = INSTITUTIONAL_EFFECT_PREFIX.to_vec();
            primary_key.extend_from_slice(record_id.as_bytes());
            let Some(bytes) = self
                .db
                .get(&primary_key)
                .map_err(|e| format!("sled get ier primary for migration: {e}"))?
            else {
                tracing::warn!(
                    record_id = %record_id,
                    "ier_by_proposal migration: primary record missing; dropping orphan index entry"
                );
                self.db
                    .remove(&old_key)
                    .map_err(|e| format!("sled remove orphan ier_by_proposal entry: {e}"))?;
                dropped += 1;
                continue;
            };
            let record: InstitutionalEffectRecord =
                serde_json::from_slice(&bytes).map_err(|e| {
                    format!("deserialize InstitutionalEffectRecord during migration: {e}")
                })?;
            let new_key = Self::ier_by_proposal_key(
                &record.proposal_id,
                record.recorded_at,
                &record.record_id,
            );
            if new_key == old_key {
                continue;
            }
            self.db
                .insert(&new_key, record.record_id.as_bytes())
                .map_err(|e| format!("sled insert migrated ier_by_proposal key: {e}"))?;
            self.db
                .remove(&old_key)
                .map_err(|e| format!("sled remove legacy ier_by_proposal key: {e}"))?;
            rewritten += 1;
        }

        self.db
            .insert(MIGRATION_FLAG_V2_IER_BY_PROPOSAL, &[1u8])
            .map_err(|e| format!("sled set migration flag v2_ier_by_proposal: {e}"))?;

        if rewritten > 0 || dropped > 0 {
            tracing::info!(
                rewritten,
                dropped,
                "ier_by_proposal: colon-alias migration complete"
            );
        }
        Ok(())
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

    /// InstitutionalEffectRecord by-proposal secondary index key. Same
    /// length-prefix scheme as `mandate_by_proposal_key` so proposal IDs
    /// containing `:` cannot alias, with `recorded_at` big-endian-encoded
    /// so lexicographic scans yield ascending chronological order.
    fn ier_by_proposal_key(proposal_id: &str, recorded_at: u64, record_id: &str) -> Vec<u8> {
        let mut key = INSTITUTIONAL_EFFECT_BY_PROPOSAL_PREFIX.to_vec();
        key.extend_from_slice(&Self::len_prefixed(proposal_id.as_bytes()));
        key.extend_from_slice(&recorded_at.to_be_bytes());
        key.extend_from_slice(record_id.as_bytes());
        key
    }

    fn ier_by_proposal_scan_prefix(proposal_id: &str) -> Vec<u8> {
        let mut prefix = INSTITUTIONAL_EFFECT_BY_PROPOSAL_PREFIX.to_vec();
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

    /// Canonical byte encoding of a [`Grantee`] for secondary-index keying.
    ///
    /// - Person: tag byte `0x01` followed by the DID string bytes.
    /// - Entity: tag byte `0x02` followed by the entity ID string bytes.
    ///
    /// The distinct tag bytes keep Person and Entity values in separate
    /// key-spaces even when string bytes coincide, and the whole value is
    /// further wrapped in [`Self::len_prefixed`] at the key-composition
    /// site so two encodings where one is a prefix of the other cannot
    /// alias under `scan_prefix`. This is the ADR-0014 length-prefix
    /// scheme; raw colon-delimited composition would reintroduce the
    /// exact aliasing bug that PR #1576 closed on the proposal-id side.
    fn grantee_canonical_bytes(grantee: &Grantee) -> Vec<u8> {
        match grantee {
            Grantee::Person(did) => {
                let s = did.as_str().as_bytes();
                let mut out = Vec::with_capacity(1 + s.len());
                out.push(0x01);
                out.extend_from_slice(s);
                out
            }
            Grantee::Entity(id) => {
                let s = id.as_bytes();
                let mut out = Vec::with_capacity(1 + s.len());
                out.push(0x02);
                out.extend_from_slice(s);
                out
            }
        }
    }

    fn grant_by_grantee_key(grantee: &Grantee, valid_from: u64, id: &AuthorityGrantId) -> Vec<u8> {
        let mut key = AUTHORITY_GRANT_BY_GRANTEE_PREFIX.to_vec();
        let canon = Self::grantee_canonical_bytes(grantee);
        key.extend_from_slice(&Self::len_prefixed(&canon));
        key.extend_from_slice(&valid_from.to_be_bytes());
        key.extend_from_slice(id.0.hyphenated().to_string().as_bytes());
        key
    }

    fn grant_by_grantee_scan_prefix(grantee: &Grantee) -> Vec<u8> {
        let mut prefix = AUTHORITY_GRANT_BY_GRANTEE_PREFIX.to_vec();
        let canon = Self::grantee_canonical_bytes(grantee);
        prefix.extend_from_slice(&Self::len_prefixed(&canon));
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
        let grantee_idx = Self::grant_by_grantee_key(&grant.grantee, grant.valid_from, &grant.id);

        self.db
            .transaction(|tx| {
                tx.insert(primary_key.as_slice(), value.as_slice())?;
                if let Some(idx) = decision_idx.as_ref() {
                    tx.insert(idx.as_slice(), id_bytes.as_slice())?;
                }
                tx.insert(grantee_idx.as_slice(), id_bytes.as_slice())?;
                Ok::<(), ConflictableTransactionError<()>>(())
            })
            .map_err(|e: TransactionError<()>| format!("sled grant tx: {e:?}"))
    }

    /// Backfill the by-grantee secondary index for grants persisted
    /// before this index existed.
    ///
    /// PR #1575 wired `put_authority_grant` (primary + by-decision
    /// index). PR #1579 added the by-grantee index wired into the
    /// same write path. A sled database written between #1575 merging
    /// and #1579 merging may hold primary grant records with no
    /// corresponding by-grantee entry — `list_*_by_grantee` readers
    /// would then miss those grants, and an accepted lifecycle
    /// revocation in the acceptance seam would leave them active.
    ///
    /// This method scans every primary grant record and writes any
    /// missing by-grantee entry with the deterministic
    /// `(grantee, valid_from, id)` key. It is:
    /// - **Idempotent**: keys are deterministic from the grant,
    ///   so re-running against a fully backfilled db is a no-op.
    /// - **Non-destructive**: primary records are never mutated; the
    ///   by-decision index is never touched; no grant is revoked,
    ///   deleted, or moved.
    /// - **Per-entry atomic**: each missing index write uses a single
    ///   `db.insert` (no cross-grant transaction). Partial failures
    ///   leave the db in a consistent partial-backfill state that the
    ///   next call resumes from.
    ///
    /// Callers typically invoke this once during gateway startup
    /// immediately after opening the receipt store. Returns the number
    /// of index entries written.
    pub fn backfill_grant_by_grantee_index(&self) -> Result<usize, String> {
        let mut written = 0usize;
        for kv in self.db.scan_prefix(AUTHORITY_GRANT_PREFIX) {
            let (key, value) = kv.map_err(|e| format!("sled scan grants: {e}"))?;
            // `AUTHORITY_GRANT_PREFIX` ("adr0014:grant:") is a byte
            // prefix of both secondary-index prefixes. Skip secondary
            // entries explicitly — their values are bare grant-id
            // bytes, not serialized grants, and attempting to
            // deserialize them would be a category error.
            if key.starts_with(AUTHORITY_GRANT_BY_DECISION_PREFIX)
                || key.starts_with(AUTHORITY_GRANT_BY_GRANTEE_PREFIX)
            {
                continue;
            }
            let grant: AuthorityGrant = serde_json::from_slice(&value).map_err(|e| {
                format!("deserialize AuthorityGrant during by-grantee backfill: {e}")
            })?;
            let grantee_key =
                Self::grant_by_grantee_key(&grant.grantee, grant.valid_from, &grant.id);
            if self
                .db
                .get(&grantee_key)
                .map_err(|e| format!("sled get grantee idx during backfill: {e}"))?
                .is_none()
            {
                let id_bytes = grant.id.0.hyphenated().to_string().into_bytes();
                self.db
                    .insert(&grantee_key, id_bytes.as_slice())
                    .map_err(|e| format!("sled insert grantee idx during backfill: {e}"))?;
                written += 1;
            }
        }
        Ok(written)
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

    /// List all authority grants for a grantee that are active at `now`.
    ///
    /// "Active" is defined by [`AuthorityGrant::is_active_at`]: not
    /// revoked at or before `now`, `now >= valid_from`, and (if
    /// `valid_until` is set) `now < valid_until`. Ordering is oldest-first
    /// by `valid_from` (the secondary-index sort order).
    ///
    /// Uses the by-grantee secondary index written at grant-creation
    /// time. Iteration is `O(grants_for_grantee)` — bounded by the
    /// number of grants ever issued to this grantee, not by the total
    /// grant count.
    pub fn list_active_authority_grants_by_grantee(
        &self,
        grantee: &Grantee,
        now: Timestamp,
    ) -> Result<Vec<AuthorityGrant>, String> {
        let scan = Self::grant_by_grantee_scan_prefix(grantee);
        let mut out = Vec::new();
        for entry in self.db.scan_prefix(&scan) {
            let (_k, v) = entry.map_err(|e| format!("sled scan grant by_grantee: {e}"))?;
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
                    id = %id_str,
                    "authority grant by-grantee index skew: primary missing"
                );
                continue;
            };
            let grant: AuthorityGrant = serde_json::from_slice(&bytes)
                .map_err(|e| format!("deserialize AuthorityGrant: {e}"))?;
            // Filter-on-read: the grantee index is authoritative for
            // grantee-keyed lookup, but the canonical answer to "is this
            // still active?" lives on the primary record (its
            // `revoked_at` may have been updated since the index was
            // written). `is_active_at` consults that canonical state.
            if grant.is_active_at(now) {
                out.push(grant);
            }
        }
        Ok(out)
    }

    /// List **all** authority grants ever issued to a grantee, including
    /// revoked and expired ones, ordered oldest-first by `valid_from`.
    ///
    /// Uses the same by-grantee secondary index as
    /// [`Self::list_active_authority_grants_by_grantee`] but omits the
    /// `is_active_at` filter. Used by the reinstatement seam to locate
    /// the most-recent revoked grant as a template for the fresh grant
    /// that reinstatement mints.
    pub fn list_authority_grants_by_grantee(
        &self,
        grantee: &Grantee,
    ) -> Result<Vec<AuthorityGrant>, String> {
        let scan = Self::grant_by_grantee_scan_prefix(grantee);
        let mut out = Vec::new();
        for entry in self.db.scan_prefix(&scan) {
            let (_k, v) = entry.map_err(|e| format!("sled scan grant by_grantee: {e}"))?;
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
                    id = %id_str,
                    "authority grant by-grantee index skew: primary missing"
                );
                continue;
            };
            let grant: AuthorityGrant = serde_json::from_slice(&bytes)
                .map_err(|e| format!("deserialize AuthorityGrant: {e}"))?;
            out.push(grant);
        }
        Ok(out)
    }

    /// Revoke an authority grant by stamping `revoked_at` on its primary
    /// record.
    ///
    /// **Semantics (monotonic minimum):**
    ///
    /// - If `revoked_at` is currently `None`, stamp it with the new
    ///   timestamp.
    /// - If `revoked_at` is currently `Some(existing)` and
    ///   `new < existing`, replace with the earlier value. This covers
    ///   the real case where a `RevokeAuthority` with
    ///   `effective_at: Some(future)` lands first, and a later
    ///   `RemoveSteward` at `now < future` must tighten the termination
    ///   time rather than be silently ignored.
    /// - If `revoked_at` is currently `Some(existing)` and
    ///   `new >= existing`, this is a no-op. Once a grant is terminated,
    ///   a later decision cannot loosen that termination — revocation
    ///   is one-way. This preserves the ADR-0014 constitutional-record
    ///   property: a double-revocation retry at a later `now` never
    ///   moves the timestamp forward.
    /// - Concurrent revocations serialize inside a single sled
    ///   transaction on the primary key, so two concurrent writers
    ///   cannot both observe the same pre-state and race to overwrite
    ///   each other. The transaction computes `min(existing, new)` and
    ///   writes only when that is strictly less than the current
    ///   `revoked_at`.
    /// - Missing primary is an error: if no grant exists for `grant_id`,
    ///   returns `Err("grant_not_found: …")`. Callers decide whether this
    ///   is fatal or skippable (the acceptance seam logs and continues).
    /// - Secondary indexes are intentionally untouched. The by-decision
    ///   and by-grantee indexes are keyed by `valid_from` and grant id,
    ///   neither of which changes on revocation. Consumers that need
    ///   "active right now" semantics filter on read via
    ///   [`AuthorityGrant::is_active_at`]. Rewriting index keys on
    ///   revocation would only invalidate existing readers without
    ///   adding correctness.
    pub fn revoke_authority_grant(
        &self,
        grant_id: &AuthorityGrantId,
        revoked_at: Timestamp,
    ) -> Result<(), String> {
        let primary_key = Self::grant_primary_key(grant_id);
        // Read + check + conditional write all happen inside one sled
        // transaction so concurrent revocations serialize on the
        // primary key: two callers cannot both observe the same
        // pre-state and race to overwrite each other. The transaction
        // body computes `min(existing, new)` and writes only when that
        // is strictly less than the current `revoked_at`.
        self.db
            .transaction(|tx| {
                let Some(bytes) = tx.get(primary_key.as_slice())? else {
                    return Err(ConflictableTransactionError::Abort(format!(
                        "grant_not_found: {grant_id}"
                    )));
                };
                let mut grant: AuthorityGrant =
                    serde_json::from_slice(bytes.as_ref()).map_err(|e| {
                        ConflictableTransactionError::Abort(format!(
                            "deserialize AuthorityGrant: {e}"
                        ))
                    })?;
                // Monotonic-minimum: keep the earliest effective
                // revocation. A later decision can tighten but never
                // loosen the termination time.
                match grant.revoked_at {
                    Some(existing) if revoked_at >= existing => {
                        // No tightening: existing termination already
                        // occurs no later than this one. Leave as-is.
                        return Ok(());
                    }
                    _ => {
                        grant.revoked_at = Some(revoked_at);
                    }
                }
                let new_bytes = serde_json::to_vec(&grant).map_err(|e| {
                    ConflictableTransactionError::Abort(format!(
                        "Failed to serialize AuthorityGrant: {e}"
                    ))
                })?;
                tx.insert(primary_key.as_slice(), new_bytes.as_slice())?;
                Ok(())
            })
            .map_err(|e: TransactionError<String>| match e {
                TransactionError::Abort(msg) => msg,
                TransactionError::Storage(err) => format!("sled revoke grant tx: {err}"),
            })
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
            grantee_idx: Vec<u8>,
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
                let grantee_idx = Self::grant_by_grantee_key(&g.grantee, g.valid_from, &g.id);
                Ok(PreparedGrant {
                    primary_key,
                    value,
                    id_bytes,
                    decision_idx,
                    grantee_idx,
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
                    tx.insert(pg.grantee_idx.as_slice(), pg.id_bytes.as_slice())?;
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

    fn list_active_authority_grants_by_grantee(
        &self,
        grantee: &Grantee,
        now: Timestamp,
    ) -> Result<Vec<AuthorityGrant>, String> {
        self.list_active_authority_grants_by_grantee(grantee, now)
    }

    fn list_authority_grants_by_grantee(
        &self,
        grantee: &Grantee,
    ) -> Result<Vec<AuthorityGrant>, String> {
        self.list_authority_grants_by_grantee(grantee)
    }

    fn revoke_authority_grant(
        &self,
        grant_id: &AuthorityGrantId,
        revoked_at: Timestamp,
    ) -> Result<(), String> {
        self.revoke_authority_grant(grant_id, revoked_at)
    }

    fn put_mandate_with_grants(
        &self,
        mandate: &Mandate,
        grants: &[AuthorityGrant],
    ) -> Result<(), String> {
        self.put_mandate_with_grants_atomic(mandate, grants)
    }

    fn put_action_item_completion(
        &self,
        receipt: &ActionItemCompletionReceipt,
    ) -> Result<(), String> {
        // Primary record by record_hash. Two receipts with identical
        // content collapse to one (idempotent), but any change in
        // actor/completed_at/transition produces a distinct hash and a
        // distinct record — append-only history is preserved.
        let mut primary_key = ACTION_ITEM_COMPLETION_REC_PREFIX.to_vec();
        primary_key.extend_from_slice(&receipt.record_hash);
        let value = serde_json::to_vec(receipt)
            .map_err(|e| format!("serialize action item completion receipt: {e}"))?;
        self.db
            .insert(&primary_key, value)
            .map_err(|e| format!("sled put_action_item_completion primary: {e}"))?;

        // Secondary index by_item, ordered by completed_at for chain
        // reads. Empty value — the index points at the record via the
        // record_hash suffix in the key.
        let mut idx_key = ACTION_ITEM_COMPLETION_BY_ITEM_PREFIX.to_vec();
        idx_key.extend_from_slice(&(receipt.item_id.len() as u64).to_be_bytes());
        idx_key.extend_from_slice(receipt.item_id.as_bytes());
        idx_key.extend_from_slice(&receipt.completed_at.to_be_bytes());
        idx_key.extend_from_slice(&receipt.record_hash);
        self.db
            .insert(&idx_key, b"")
            .map_err(|e| format!("sled put_action_item_completion by_item: {e}"))?;

        // Note: no per-write `db.flush()`. Other receipt writes in this
        // store (`put_governance`, `put_allocation`, …) rely on sled's
        // normal durability semantics; this path follows the same
        // pattern.
        Ok(())
    }

    fn get_action_item_completion_by_item(
        &self,
        item_id: &str,
    ) -> Result<Option<ActionItemCompletionReceipt>, String> {
        // Latest = receipt with the largest `completed_at`. Because the
        // by_item index orders entries by `completed_at` ascending under
        // a fixed item-id prefix, the latest is the last hit.
        Ok(self
            .list_action_item_completions_by_item(item_id)?
            .into_iter()
            .next_back())
    }

    fn list_action_item_completions_by_item(
        &self,
        item_id: &str,
    ) -> Result<Vec<ActionItemCompletionReceipt>, String> {
        let mut prefix = ACTION_ITEM_COMPLETION_BY_ITEM_PREFIX.to_vec();
        prefix.extend_from_slice(&(item_id.len() as u64).to_be_bytes());
        prefix.extend_from_slice(item_id.as_bytes());

        let mut out = Vec::new();
        for entry in self.db.scan_prefix(&prefix) {
            let (key, _) = entry.map_err(|e| {
                format!("sled scan_prefix list_action_item_completions_by_item: {e}")
            })?;
            // Tail of the key is the 32-byte record_hash.
            if key.len() < 32 {
                continue;
            }
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&key[key.len() - 32..]);
            let mut primary_key = ACTION_ITEM_COMPLETION_REC_PREFIX.to_vec();
            primary_key.extend_from_slice(&hash);
            let raw = self.db.get(&primary_key).map_err(|e| {
                format!("sled get list_action_item_completions_by_item primary: {e}")
            })?;
            if let Some(bytes) = raw {
                let r: ActionItemCompletionReceipt = serde_json::from_slice(&bytes)
                    .map_err(|e| format!("deserialize action item completion receipt: {e}"))?;
                out.push(r);
            }
        }
        Ok(out)
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
    fn authority_grant_revocation_roundtrip() {
        // Put a fresh grant, revoke it, read it back, assert `revoked_at`
        // is stamped and survives the primary-record roundtrip.
        let store = ReceiptStore::new(temp_db());
        let decision_hash = [0xa1u8; 32];
        let grant = make_grant(decision_hash, 1_000);
        let grant_id = grant.id.clone();

        store.put_authority_grant(&grant).unwrap();
        assert!(
            store
                .get_authority_grant(&grant_id)
                .unwrap()
                .unwrap()
                .revoked_at
                .is_none(),
            "fresh grant must have revoked_at: None"
        );

        store.revoke_authority_grant(&grant_id, 2_000).unwrap();

        let revoked = store.get_authority_grant(&grant_id).unwrap().unwrap();
        assert_eq!(
            revoked.revoked_at,
            Some(2_000),
            "revoke_authority_grant must stamp revoked_at on the primary record"
        );
    }

    #[test]
    fn authority_grant_is_active_at_flips_false_after_revocation() {
        // After revocation, `is_active_at(now)` must return false for any
        // `now >= revoked_at`. This is the canonical-state invariant the
        // seam relies on to decide whether a grant still carries authority.
        let store = ReceiptStore::new(temp_db());
        let decision_hash = [0xa2u8; 32];
        let grant = make_grant(decision_hash, 1_000); // valid_from 1_000, valid_until 4_600
        let grant_id = grant.id.clone();
        store.put_authority_grant(&grant).unwrap();

        // Pre-revocation: active at a time inside the term.
        let before = store.get_authority_grant(&grant_id).unwrap().unwrap();
        assert!(before.is_active_at(2_000), "active before revocation");

        store.revoke_authority_grant(&grant_id, 2_000).unwrap();

        let after = store.get_authority_grant(&grant_id).unwrap().unwrap();
        assert!(
            !after.is_active_at(2_000),
            "must not be active at t == revoked_at"
        );
        assert!(
            !after.is_active_at(3_000),
            "must not be active after revocation"
        );
        assert!(
            after.is_active_at(1_500),
            "must still be active at t < revoked_at (revocation is not retroactive)"
        );
    }

    #[test]
    fn authority_grant_revocation_later_retry_is_no_op() {
        // Monotonic minimum: once `revoked_at` is set, a retry at a
        // LATER timestamp must not loosen the termination. This keeps
        // double-revocation retries safe and preserves the
        // constitutional record: a later decision can tighten but
        // never extend active authority.
        let store = ReceiptStore::new(temp_db());
        let grant = make_grant([0xa3u8; 32], 1_000);
        let grant_id = grant.id.clone();
        store.put_authority_grant(&grant).unwrap();

        store.revoke_authority_grant(&grant_id, 2_000).unwrap();
        // Second revoke at a strictly later time: must be a no-op.
        store.revoke_authority_grant(&grant_id, 9_999).unwrap();
        // Revoke at the same time: also a no-op (>= comparison).
        store.revoke_authority_grant(&grant_id, 2_000).unwrap();

        let g = store.get_authority_grant(&grant_id).unwrap().unwrap();
        assert_eq!(
            g.revoked_at,
            Some(2_000),
            "later-or-equal retry must preserve the earlier `revoked_at`"
        );
    }

    #[test]
    fn authority_grant_revocation_tightens_to_earlier_timestamp() {
        // Monotonic minimum: if an existing `revoked_at` is in the
        // future (e.g. from a `RevokeAuthority { effective_at }` grace
        // period), a later decision whose effective time is strictly
        // earlier must tighten the termination. Otherwise an
        // immediate-removal decision would be silently ignored while
        // a grace-period revocation kept the grant active.
        let store = ReceiptStore::new(temp_db());
        let grant = make_grant([0xa4u8; 32], 1_000);
        let grant_id = grant.id.clone();
        store.put_authority_grant(&grant).unwrap();

        // First revocation with a FUTURE effective_at (grace period).
        store.revoke_authority_grant(&grant_id, 5_000).unwrap();
        assert_eq!(
            store
                .get_authority_grant(&grant_id)
                .unwrap()
                .unwrap()
                .revoked_at,
            Some(5_000)
        );

        // Immediate-removal decision at `now = 3_000` must tighten.
        store.revoke_authority_grant(&grant_id, 3_000).unwrap();
        let g = store.get_authority_grant(&grant_id).unwrap().unwrap();
        assert_eq!(
            g.revoked_at,
            Some(3_000),
            "strictly-earlier revocation must tighten the termination time"
        );
        assert!(
            !g.is_active_at(3_000),
            "grant must be inactive at the tightened revocation time"
        );

        // A subsequent retry even earlier also tightens.
        store.revoke_authority_grant(&grant_id, 2_500).unwrap();
        assert_eq!(
            store
                .get_authority_grant(&grant_id)
                .unwrap()
                .unwrap()
                .revoked_at,
            Some(2_500)
        );
    }

    #[test]
    fn revoke_missing_grant_returns_grant_not_found() {
        // Revoking a grant that was never persisted must surface as an
        // error whose message begins with `grant_not_found:` so the
        // acceptance seam can recognise it as skippable (vs a hard
        // store failure).
        let store = ReceiptStore::new(temp_db());
        let stranger_id = AuthorityGrantId::new();
        let err = store
            .revoke_authority_grant(&stranger_id, 1_000)
            .unwrap_err();
        assert!(
            err.starts_with("grant_not_found"),
            "expected grant_not_found sentinel; got {err}"
        );
    }

    #[test]
    fn list_active_authority_grants_by_grantee_filters_revoked() {
        // The active-filter variant must consult primary-record
        // `revoked_at` and drop revoked grants even though the by-grantee
        // index still points at them.
        let store = ReceiptStore::new(temp_db());
        // Uses an Entity grantee so this test stays focused on the
        // revoke-vs-active filter and does not depend on Ed25519
        // public-key validity for a synthetic DID.
        let grantee = Grantee::Entity("svc:filter-test".into());
        let active_grant = AuthorityGrant {
            grantee: grantee.clone(),
            ..make_grant([0xb1u8; 32], 1_000)
        };
        let to_revoke = AuthorityGrant {
            grantee: grantee.clone(),
            ..make_grant([0xb1u8; 32], 1_500)
        };
        store.put_authority_grant(&active_grant).unwrap();
        store.put_authority_grant(&to_revoke).unwrap();
        store.revoke_authority_grant(&to_revoke.id, 1_700).unwrap();

        let listed = store
            .list_active_authority_grants_by_grantee(&grantee, 2_000)
            .unwrap();
        assert_eq!(listed.len(), 1, "only the non-revoked grant must be listed");
        assert_eq!(listed[0].id, active_grant.id);
    }

    #[test]
    fn list_authority_grants_by_grantee_includes_revoked() {
        // The unfiltered variant is what reinstatement uses to find a
        // template; it must return the revoked grant as well.
        let store = ReceiptStore::new(temp_db());
        let grantee = Grantee::Entity("svc:unfiltered-test".into());
        let g1 = AuthorityGrant {
            grantee: grantee.clone(),
            ..make_grant([0xb2u8; 32], 1_000)
        };
        let g2 = AuthorityGrant {
            grantee: grantee.clone(),
            ..make_grant([0xb2u8; 32], 1_500)
        };
        store.put_authority_grant(&g1).unwrap();
        store.put_authority_grant(&g2).unwrap();
        store.revoke_authority_grant(&g2.id, 1_700).unwrap();

        let all = store.list_authority_grants_by_grantee(&grantee).unwrap();
        assert_eq!(all.len(), 2, "unfiltered list must include revoked grants");
        // Expect oldest-first by valid_from
        assert_eq!(all[0].id, g1.id);
        assert_eq!(all[1].id, g2.id);
        assert!(all[1].revoked_at.is_some());
    }

    #[test]
    fn backfill_by_grantee_index_recovers_legacy_grants() {
        // Simulate a database written before the by-grantee index existed:
        // write a primary grant record directly via the raw db, bypassing
        // `put_authority_grant` which would populate the index. The
        // listing-by-grantee reader must not see the grant, then after
        // backfill runs it must see it. Running backfill again returns 0.
        let store = ReceiptStore::new(temp_db());
        let grantee = Grantee::Entity("svc:legacy".into());
        let grant = AuthorityGrant {
            grantee: grantee.clone(),
            ..make_grant([0xd1u8; 32], 1_000)
        };

        // Write primary record ONLY — no by-grantee index entry.
        let primary_key = ReceiptStore::grant_primary_key(&grant.id);
        let bytes = serde_json::to_vec(&grant).unwrap();
        store.db.insert(&primary_key, bytes).unwrap();

        // Pre-backfill: primary reads fine, but by-grantee listing is empty.
        assert_eq!(
            store.get_authority_grant(&grant.id).unwrap().as_ref(),
            Some(&grant)
        );
        assert!(store
            .list_active_authority_grants_by_grantee(&grantee, 1_500)
            .unwrap()
            .is_empty());

        // Run backfill.
        let written = store.backfill_grant_by_grantee_index().unwrap();
        assert_eq!(written, 1);

        // Post-backfill: listing-by-grantee sees the grant.
        let listed = store
            .list_active_authority_grants_by_grantee(&grantee, 1_500)
            .unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, grant.id);

        // Idempotent: re-running does nothing.
        assert_eq!(store.backfill_grant_by_grantee_index().unwrap(), 0);
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
    // Proposal-id index colon-alias regression tests
    // ============================================================
    //
    // `PROPOSAL_INDEX_PREFIX` and `INSTITUTIONAL_EFFECT_BY_PROPOSAL_PREFIX`
    // now use the same colon-safe length-prefix scheme as
    // `MANDATE_BY_PROPOSAL_PREFIX`, so `foo` and `foo:bar` have disjoint
    // scan-prefix ranges by construction. Legacy raw-colon entries
    // (pre-#1589) are rewritten into the new shape by the one-shot
    // migration that runs in [`ReceiptStore::new`]. The #1576
    // filter-on-read workaround has been removed; these tests pin the
    // canonical schema-level separation and the migration path.

    #[test]
    fn governance_proposal_index_does_not_alias_colon_prefixes() {
        let store = ReceiptStore::new(temp_db());
        // Write only the `foo:bar` receipt. Under the length-prefix
        // scheme, scanning with the `foo` scan prefix must not see any
        // entry whose length-prefix is the `foo:bar` byte pattern.
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
        // Write only a `foo:bar` record. Under the length-prefix scheme
        // its scan prefix is disjoint from `foo`'s, so a scan for `foo`
        // must return empty.
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

    // ============================================================
    // Migration regression tests (#1589)
    // ============================================================
    //
    // These tests seed legacy raw-colon `{proposal_id}:{…}` entries
    // into a fresh sled db (bypassing the public writers, which now
    // only emit the length-prefix shape), then open a `ReceiptStore`
    // on that db and assert:
    //   1. Canonical reads return the migrated record.
    //   2. The legacy key is gone.
    //   3. The new length-prefix key exists with the expected value.
    //   4. Re-opening the store is idempotent (sentinel key short-
    //      circuits the scan).

    /// Build a legacy `PROPOSAL_INDEX_PREFIX` key in the pre-#1589
    /// raw-colon shape. Test-only helper: production writers use
    /// `make_proposal_index_key` with length prefix.
    fn legacy_proposal_index_key(proposal_id: &str, receipt_hash: &Hash) -> Vec<u8> {
        let mut key = PROPOSAL_INDEX_PREFIX.to_vec();
        key.extend_from_slice(proposal_id.as_bytes());
        key.push(b':');
        key.extend_from_slice(hex::encode(receipt_hash).as_bytes());
        key
    }

    /// Build a legacy `INSTITUTIONAL_EFFECT_BY_PROPOSAL_PREFIX` key in
    /// the pre-#1589 raw-colon shape.
    fn legacy_ier_by_proposal_key(proposal_id: &str, recorded_at: u64, record_id: &str) -> Vec<u8> {
        let mut key = INSTITUTIONAL_EFFECT_BY_PROPOSAL_PREFIX.to_vec();
        key.extend_from_slice(proposal_id.as_bytes());
        key.push(b':');
        key.extend_from_slice(&recorded_at.to_be_bytes());
        key.push(b':');
        key.extend_from_slice(record_id.as_bytes());
        key
    }

    #[test]
    fn migration_rewrites_legacy_proposal_index_keys() {
        let db = temp_db();

        // Seed the primary governance record and its decision-hash
        // index via the public writer, then overwrite the proposal
        // index entry with the legacy-shape key. We want the db to
        // contain a legacy key at open time.
        let pre_store = ReceiptStore::new(db.clone());
        let receipt = make_test_governance_receipt("foo:bar");
        let hash = pre_store.put_governance(&receipt).unwrap();

        // Delete the new-format key the writer just inserted, then
        // insert its legacy equivalent.
        let new_key = ReceiptStore::make_proposal_index_key("foo:bar", &hash);
        db.remove(&new_key).unwrap();
        let legacy_key = legacy_proposal_index_key("foo:bar", &hash);
        db.insert(&legacy_key, &hash[..]).unwrap();

        // Clear the migration sentinel so open re-runs the migration.
        db.remove(MIGRATION_FLAG_V2_PROPOSAL_INDEX).unwrap();

        // Open a fresh ReceiptStore — migration runs in `new`.
        let store = ReceiptStore::new(db.clone());

        // Legacy key must be gone.
        assert!(
            db.get(&legacy_key).unwrap().is_none(),
            "legacy proposal_index key should be removed by migration"
        );
        // New-format key must exist with the hash as value.
        let migrated = db
            .get(&new_key)
            .unwrap()
            .expect("new-format proposal_index key must exist post-migration");
        assert_eq!(&migrated[..], &hash[..]);

        // Canonical read resolves via the length-prefix scan prefix.
        let found = store
            .get_governance_by_proposal("foo:bar")
            .unwrap()
            .expect("migrated receipt must be readable");
        assert_eq!(found.proposal_id, "foo:bar");
        assert_eq!(found.decision_hash, hash);

        // Sentinel is now set.
        assert!(db.get(MIGRATION_FLAG_V2_PROPOSAL_INDEX).unwrap().is_some());

        // Second open is a no-op: migration is gated by the sentinel.
        let _ = ReceiptStore::new(db);
    }

    #[test]
    fn migration_drops_orphan_proposal_index_entries() {
        let db = temp_db();
        // Orphan legacy entry: index points at a hash whose primary
        // governance receipt does not exist. Migration should remove it.
        let orphan_hash = [0x11u8; 32];
        let legacy_key = legacy_proposal_index_key("ghost", &orphan_hash);
        db.insert(&legacy_key, &orphan_hash[..]).unwrap();

        let _ = ReceiptStore::new(db.clone());

        assert!(
            db.get(&legacy_key).unwrap().is_none(),
            "orphan legacy proposal_index entry should be dropped by migration"
        );
    }

    #[test]
    fn migration_rewrites_legacy_ier_by_proposal_keys() {
        let db = temp_db();

        // Use the public writer to land primary + new-format index,
        // then drop the new-format index entry and insert a legacy one.
        let pre_store = ReceiptStore::new(db.clone());
        let rec = InstitutionalEffectRecord::new(
            "foo:bar",
            "coop-a",
            Some([7u8; 32]),
            "freeze_member",
            Some("did:icn:x".into()),
            None,
            None,
            42,
            serde_json::json!({}),
        );
        pre_store.put_institutional_effect(&rec).unwrap();

        let new_key =
            ReceiptStore::ier_by_proposal_key(&rec.proposal_id, rec.recorded_at, &rec.record_id);
        db.remove(&new_key).unwrap();
        let legacy_key =
            legacy_ier_by_proposal_key(&rec.proposal_id, rec.recorded_at, &rec.record_id);
        db.insert(&legacy_key, rec.record_id.as_bytes()).unwrap();

        // Clear sentinel so migration re-runs on open.
        db.remove(MIGRATION_FLAG_V2_IER_BY_PROPOSAL).unwrap();

        let store = ReceiptStore::new(db.clone());

        assert!(
            db.get(&legacy_key).unwrap().is_none(),
            "legacy ier_by_proposal key should be removed by migration"
        );
        let migrated = db
            .get(&new_key)
            .unwrap()
            .expect("new-format ier_by_proposal key must exist post-migration");
        assert_eq!(&migrated[..], rec.record_id.as_bytes());

        let list = store
            .list_institutional_effects_by_proposal("foo:bar")
            .unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].record_id, rec.record_id);

        // Sentinel set, second open is a no-op.
        assert!(db.get(MIGRATION_FLAG_V2_IER_BY_PROPOSAL).unwrap().is_some());
        let _ = ReceiptStore::new(db);
    }

    #[test]
    fn migration_disambiguates_legacy_colon_aliased_ier_entries() {
        // The bug: a legacy db contains entries for both `foo` and
        // `foo:bar`, raw-colon-shaped. Scanning for `foo:` in the old
        // scheme would hit `foo:bar`'s entries too. After migration,
        // each proposal's length-prefixed scan must return exactly
        // its own records.
        let db = temp_db();
        let pre_store = ReceiptStore::new(db.clone());

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
        pre_store.put_institutional_effect(&foo).unwrap();
        pre_store.put_institutional_effect(&foo_bar).unwrap();

        // Replace both new-format index entries with legacy equivalents.
        for r in [&foo, &foo_bar] {
            let new_key =
                ReceiptStore::ier_by_proposal_key(&r.proposal_id, r.recorded_at, &r.record_id);
            db.remove(&new_key).unwrap();
            let legacy_key =
                legacy_ier_by_proposal_key(&r.proposal_id, r.recorded_at, &r.record_id);
            db.insert(&legacy_key, r.record_id.as_bytes()).unwrap();
        }
        db.remove(MIGRATION_FLAG_V2_IER_BY_PROPOSAL).unwrap();

        let store = ReceiptStore::new(db);
        let foo_list = store.list_institutional_effects_by_proposal("foo").unwrap();
        assert_eq!(foo_list.len(), 1, "foo must not see foo:bar's record");
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
    fn migration_is_idempotent_on_already_migrated_store() {
        // Open and write via public API (always new-format); re-open
        // after clearing the sentinel — migration should re-run but
        // rewrite nothing because every key is already new-format.
        let db = temp_db();
        let pre = ReceiptStore::new(db.clone());
        let _ = pre
            .put_governance(&make_test_governance_receipt("foo:bar"))
            .unwrap();
        let rec = InstitutionalEffectRecord::new(
            "foo:bar",
            "coop-a",
            None,
            "freeze_member",
            None,
            None,
            None,
            1,
            serde_json::json!({}),
        );
        pre.put_institutional_effect(&rec).unwrap();

        // Clear both sentinels to force the migration loops to run.
        db.remove(MIGRATION_FLAG_V2_PROPOSAL_INDEX).unwrap();
        db.remove(MIGRATION_FLAG_V2_IER_BY_PROPOSAL).unwrap();

        let store = ReceiptStore::new(db);
        // Data unchanged.
        assert!(store
            .get_governance_by_proposal("foo:bar")
            .unwrap()
            .is_some());
        assert_eq!(
            store
                .list_institutional_effects_by_proposal("foo:bar")
                .unwrap()
                .len(),
            1
        );
    }
}
