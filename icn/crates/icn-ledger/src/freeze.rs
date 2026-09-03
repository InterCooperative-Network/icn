//! Member freeze functionality for emergency governance actions
//!
//! This module provides the ability to freeze member accounts, blocking all
//! ledger transactions involving them until unfrozen. This is an emergency
//! action requiring super-majority approval via governance.
//!
//! ## Use Cases
//! - Compromised account detected
//! - Suspected fraud or abuse
//! - Pending investigation
//! - Legal compliance requirements
//!
//! ## Example
//!
//! ```rust,no_run
//! use icn_ledger::freeze::{FreezeManager, FrozenMember};
//! use icn_identity::KeyPair;
//!
//! let mut freeze_manager = FreezeManager::new();
//!
//! // Create a test DID
//! let keypair = KeyPair::generate().unwrap();
//! let did = keypair.did().clone();
//!
//! // Freeze a member for 24 hours
//! freeze_manager.freeze(did.clone(), "Suspected fraud".to_string(), Some(86400));
//!
//! // Check if frozen
//! assert!(freeze_manager.is_frozen(&did));
//!
//! // Unfreeze
//! freeze_manager.unfreeze(&did, "Investigation complete".to_string());
//! assert!(!freeze_manager.is_frozen(&did));
//! ```

use crate::principal_rows::{
    refuse_unless_one_spelling_per_principal, PrincipalRowsRefusal, FROZEN_KEYSPACE,
};
use icn_identity::Did;
use icn_store::Store;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{info, warn};

/// Storage key prefix for frozen members
const FREEZE_PREFIX: &str = "ledger:frozen:";

/// A frozen member record
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FrozenMember {
    /// The frozen member's DID
    pub did: Did,

    /// Reason for freezing
    pub reason: String,

    /// When the member was frozen (Unix timestamp)
    pub frozen_at: u64,

    /// When the freeze expires (Unix timestamp), None = indefinite
    pub expires_at: Option<u64>,

    /// ID of the governance proposal that authorized the freeze
    pub proposal_id: Option<String>,

    /// Who initiated the freeze (governance actor DID or system)
    pub frozen_by: Option<Did>,
}

impl FrozenMember {
    /// Create a new frozen member record
    pub fn new(did: Did, reason: String, duration_seconds: Option<u64>) -> Self {
        let now = icn_time::current_timestamp_secs();

        Self {
            did,
            reason,
            frozen_at: now,
            expires_at: duration_seconds.map(|d| now + d),
            proposal_id: None,
            frozen_by: None,
        }
    }

    /// Create with proposal reference
    pub fn with_proposal(mut self, proposal_id: String) -> Self {
        self.proposal_id = Some(proposal_id);
        self
    }

    /// Create with actor reference
    pub fn with_actor(mut self, actor_did: Did) -> Self {
        self.frozen_by = Some(actor_did);
        self
    }

    /// Check if the freeze has expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            icn_time::current_timestamp_secs() >= expires_at
        } else {
            false // No expiration = frozen indefinitely
        }
    }

    /// Get remaining duration in seconds (0 if expired, None if indefinite)
    pub fn remaining_seconds(&self) -> Option<u64> {
        if let Some(expires_at) = self.expires_at {
            let now = icn_time::current_timestamp_secs();
            if now >= expires_at {
                Some(0)
            } else {
                Some(expires_at - now)
            }
        } else {
            None // Indefinite
        }
    }
}

/// Unfreeze event record (for audit trail)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnfreezeEvent {
    /// The unfrozen member's DID
    pub did: Did,

    /// Reason for unfreezing
    pub reason: String,

    /// When unfrozen (Unix timestamp)
    pub unfrozen_at: u64,

    /// ID of the governance proposal that authorized unfreezing
    pub proposal_id: Option<String>,

    /// Who initiated the unfreeze
    pub unfrozen_by: Option<Did>,
}

/// Manager for frozen members with optional persistence
pub struct FreezeManager {
    /// In-memory cache of frozen members
    frozen: HashMap<Did, FrozenMember>,

    /// Unfreeze history (audit trail)
    unfreeze_history: Vec<UnfreezeEvent>,

    /// Optional persistent storage
    store: Option<Arc<dyn Store>>,
}

impl FreezeManager {
    /// Create a new freeze manager (in-memory only)
    pub fn new() -> Self {
        Self {
            frozen: HashMap::new(),
            unfreeze_history: Vec::new(),
            store: None,
        }
    }

    /// Create a freeze manager with persistent storage
    pub fn with_store(store: Arc<dyn Store>) -> anyhow::Result<Self> {
        let mut manager = Self {
            frozen: HashMap::new(),
            unfreeze_history: Vec::new(),
            store: Some(store.clone()),
        };

        // Load existing frozen members from store
        manager.load_from_store()?;

        Ok(manager)
    }

    /// Freeze a member
    pub fn freeze(&mut self, did: Did, reason: String, duration_seconds: Option<u64>) {
        // Re-freezing a principal already on file must address the row the
        // store has. A second spelling would write a second row and leave the
        // first live, which is the collision `load_from_store` refuses.
        let did = self.stored_spelling(&did);
        let record = FrozenMember::new(did.clone(), reason.clone(), duration_seconds);

        info!(
            did = %did,
            reason = %reason,
            duration = ?duration_seconds,
            "Freezing member"
        );

        self.frozen.insert(did.clone(), record.clone());

        // Persist if store available
        if let Some(ref store) = self.store {
            if let Err(e) = self.persist_frozen(&did, &record, store) {
                warn!(did = %did, error = %e, "Failed to persist frozen member");
            }
        }
    }

    /// Freeze a member with additional metadata
    pub fn freeze_with_metadata(
        &mut self,
        did: Did,
        reason: String,
        duration_seconds: Option<u64>,
        proposal_id: Option<String>,
        frozen_by: Option<Did>,
    ) {
        // Same row-identity rule as `freeze`.
        let did = self.stored_spelling(&did);
        let mut record = FrozenMember::new(did.clone(), reason.clone(), duration_seconds);
        record.proposal_id = proposal_id;
        record.frozen_by = frozen_by;

        info!(
            did = %did,
            reason = %reason,
            duration = ?duration_seconds,
            proposal = ?record.proposal_id,
            "Freezing member with metadata"
        );

        self.frozen.insert(did.clone(), record.clone());

        if let Some(ref store) = self.store {
            if let Err(e) = self.persist_frozen(&did, &record, store) {
                warn!(did = %did, error = %e, "Failed to persist frozen member");
            }
        }
    }

    /// Unfreeze a member
    pub fn unfreeze(&mut self, did: &Did, reason: String) -> Option<FrozenMember> {
        let removed = self.forget_frozen(did);

        if let Some(ref record) = removed {
            info!(
                did = %record.did,
                reason = %reason,
                "Unfreezing member"
            );

            // The audit trail names the spelling that was actually stored and
            // deleted, not the one the caller happened to type. Under I7 those
            // can differ, and a history entry naming a DID string that was
            // never a persisted key cannot be reconciled with the store.
            let event = UnfreezeEvent {
                did: record.did.clone(),
                reason,
                unfrozen_at: icn_time::current_timestamp_secs(),
                proposal_id: None,
                unfrozen_by: None,
            };
            self.unfreeze_history.push(event);
        }

        removed
    }

    /// Unfreeze a member with additional metadata
    pub fn unfreeze_with_metadata(
        &mut self,
        did: &Did,
        reason: String,
        proposal_id: Option<String>,
        unfrozen_by: Option<Did>,
    ) -> Option<FrozenMember> {
        let removed = self.forget_frozen(did);

        if let Some(ref record) = removed {
            info!(
                did = %record.did,
                reason = %reason,
                proposal = ?proposal_id,
                "Unfreezing member with metadata"
            );

            // Same rule as `unfreeze`: the stored spelling is the audit fact.
            let event = UnfreezeEvent {
                did: record.did.clone(),
                reason,
                unfrozen_at: icn_time::current_timestamp_secs(),
                proposal_id,
                unfrozen_by,
            };
            self.unfreeze_history.push(event);
        }

        removed
    }

    /// Check if a member is frozen (including expiration check)
    pub fn is_frozen(&mut self, did: &Did) -> bool {
        if let Some(record) = self.frozen.get(did) {
            if record.is_expired() {
                // Auto-unfreeze expired records
                info!(did = %did, "Auto-unfreezing expired member");
                self.forget_frozen(did);

                return false;
            }
            true
        } else {
            false
        }
    }

    /// Get frozen member record if exists and not expired
    pub fn get_frozen(&mut self, did: &Did) -> Option<&FrozenMember> {
        // First check and auto-expire if needed
        if !self.is_frozen(did) {
            return None;
        }
        self.frozen.get(did)
    }

    /// Get all currently frozen members (excluding expired)
    pub fn list_frozen(&mut self) -> Vec<&FrozenMember> {
        // Clean up expired entries first
        let expired: Vec<Did> = self
            .frozen
            .iter()
            .filter(|(_, record)| record.is_expired())
            .map(|(did, _)| did.clone())
            .collect();

        for did in expired {
            self.forget_frozen(&did);
        }

        self.frozen.values().collect()
    }

    /// Get the count of frozen members
    pub fn frozen_count(&mut self) -> usize {
        self.list_frozen().len()
    }

    /// Get unfreeze history
    pub fn unfreeze_history(&self) -> &[UnfreezeEvent] {
        &self.unfreeze_history
    }

    /// Clean up all expired freezes
    pub fn cleanup_expired(&mut self) -> usize {
        let expired: Vec<Did> = self
            .frozen
            .iter()
            .filter(|(_, record)| record.is_expired())
            .map(|(did, _)| did.clone())
            .collect();

        let count = expired.len();

        for did in expired {
            info!(did = %did, "Cleaning up expired freeze");
            self.forget_frozen(&did);
        }

        count
    }

    // === Private persistence methods ===

    fn load_from_store(&mut self) -> anyhow::Result<()> {
        if let Some(ref store) = self.store {
            let pairs = store.scan(FREEZE_PREFIX.as_bytes())?;

            // Read and filter before adopting anything. Expired records are
            // dropped first because an expired freeze is not state: two rows
            // for one principal are only ambiguous when both still bind, and
            // refusing on a lapsed alias row would block a start for nothing.
            //
            // The stored **key** is what the guard groups by, not the spelling
            // inside the record. The key is the row's identity — it is what a
            // later `persist_frozen` or `remove_frozen` addresses — so a guard
            // reading only the body would see two keyed rows whose bodies reuse
            // one spelling, find no collision, and adopt them under a single
            // map entry. The loser would then survive every unfreeze and
            // re-freeze the member on the next start.
            // `pairs` is consumed so each raw row is dropped once parsed; the
            // guard holds the parsed rows only.
            let mut live = Vec::with_capacity(pairs.len());
            let mut unreadable = 0usize;
            for (key, value) in pairs {
                // The key is decoded strictly, never lossily. `persist_frozen`
                // writes `FREEZE_PREFIX` followed by a `Did`'s `Display`, which
                // is always UTF-8, so a key that is not is one the writer never
                // produced. Normalizing it would be worse than refusing it: an
                // invalid byte can normalize into a spelling the guard accepts
                // and a body can match, while the row on disk keeps the raw
                // key — and the later unfreeze would delete the normalized key
                // and leave the raw row to freeze the member again.
                let Ok(key_str) = std::str::from_utf8(&key) else {
                    unreadable += 1;
                    continue;
                };
                let spelling = key_str
                    .strip_prefix(FREEZE_PREFIX)
                    .unwrap_or(key_str)
                    .to_string();
                let record: FrozenMember = serde_json::from_slice(&value)?;
                if !record.is_expired() {
                    live.push((spelling, record));
                }
            }

            // Reported before the guard runs: a key the writer could not have
            // produced is evidence, and a classification computed without it
            // proves nothing.
            if unreadable > 0 {
                return Err(PrincipalRowsRefusal::UnreadableKey {
                    keyspace: FROZEN_KEYSPACE,
                    rows: unreadable,
                }
                .into());
            }

            // Two live freezes for one principal disagree about reason and
            // expiry, and `icn-ledger/frozen` carries
            // `RuleBasis::AwaitingDomainSignOff` — the union rule written down
            // in the migration gate is asserted, not authorized. Refuse.
            refuse_unless_one_spelling_per_principal(
                FROZEN_KEYSPACE,
                live.iter().map(|(spelling, _)| (spelling.as_str(), "")),
            )?;

            // A record whose body spells the member differently from its own
            // key cannot be addressed by the key the map would derive from it.
            let strayed = live
                .iter()
                .filter(|(spelling, record)| record.did.as_str() != spelling)
                .count();
            if strayed > 0 {
                return Err(PrincipalRowsRefusal::KeyValueSpellingMismatch {
                    keyspace: FROZEN_KEYSPACE,
                    rows: strayed,
                }
                .into());
            }

            for (_spelling, record) in live {
                self.frozen.insert(record.did.clone(), record);
            }
        }
        Ok(())
    }

    /// The spelling this manager already holds for `did`, if any.
    ///
    /// Persisted freeze keys are built from a spelling, so writing or deleting
    /// under a second spelling of the same principal would leave the first row
    /// live — a freeze that outlives its unfreeze. Every persistence call
    /// therefore addresses the row identity the map already has.
    fn stored_spelling(&self, did: &Did) -> Did {
        self.frozen
            .get_key_value(did)
            .map(|(stored, _)| stored.clone())
            .unwrap_or_else(|| did.clone())
    }

    /// Drop a principal's freeze from memory and from the store together.
    ///
    /// `HashMap::remove_entry` hands back the key the map actually held, which
    /// is the spelling the persisted row uses. Deleting under the caller's
    /// spelling instead would silently leave that row behind, and the next
    /// `load_from_store` would re-freeze a member who had been released.
    fn forget_frozen(&mut self, did: &Did) -> Option<FrozenMember> {
        let (stored_did, record) = self.frozen.remove_entry(did)?;

        if let Some(ref store) = self.store {
            if let Err(e) = self.remove_frozen(&stored_did, store) {
                warn!(did = %stored_did, error = %e, "Failed to remove frozen member from store");
            }
        }

        Some(record)
    }

    fn persist_frozen(
        &self,
        did: &Did,
        record: &FrozenMember,
        store: &Arc<dyn Store>,
    ) -> anyhow::Result<()> {
        let key = format!("{FREEZE_PREFIX}{did}");
        let value = serde_json::to_vec(record)?;
        store.put(key.as_bytes(), &value)?;
        Ok(())
    }

    fn remove_frozen(&self, did: &Did, store: &Arc<dyn Store>) -> anyhow::Result<()> {
        let key = format!("{FREEZE_PREFIX}{did}");
        store.delete(key.as_bytes())?;
        Ok(())
    }
}

impl Default for FreezeManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    fn test_did(name: &str) -> Did {
        // Generate a deterministic DID based on the name
        // For tests, we just use a generated keypair
        let _ = name; // Silence unused warning
        KeyPair::generate().unwrap().did().clone()
    }

    #[test]
    fn test_freeze_and_unfreeze() {
        let mut manager = FreezeManager::new();
        let alice = test_did("alice");

        // Initially not frozen
        assert!(!manager.is_frozen(&alice));

        // Freeze indefinitely
        manager.freeze(alice.clone(), "Test freeze".to_string(), None);
        assert!(manager.is_frozen(&alice));

        // Get frozen record
        let record = manager.get_frozen(&alice).unwrap();
        assert_eq!(record.reason, "Test freeze");
        assert!(record.expires_at.is_none());

        // Unfreeze
        let removed = manager.unfreeze(&alice, "Test complete".to_string());
        assert!(removed.is_some());
        assert!(!manager.is_frozen(&alice));
    }

    #[test]
    fn test_freeze_with_expiration() {
        let mut manager = FreezeManager::new();
        let bob = test_did("bob");

        // Freeze for 1 hour
        manager.freeze(bob.clone(), "Temporary freeze".to_string(), Some(3600));
        assert!(manager.is_frozen(&bob));

        let record = manager.get_frozen(&bob).unwrap();
        assert!(record.expires_at.is_some());
        assert!(record.remaining_seconds().unwrap() > 0);
        assert!(record.remaining_seconds().unwrap() <= 3600);
    }

    #[test]
    fn test_freeze_expiration_check() {
        let mut manager = FreezeManager::new();
        let charlie = test_did("charlie");

        // Create an already-expired freeze (0 seconds)
        let mut record = FrozenMember::new(charlie.clone(), "Expired".to_string(), Some(0));
        // Manually set expires_at to the past
        record.expires_at = Some(0); // Unix epoch = long expired

        manager.frozen.insert(charlie.clone(), record);

        // Should auto-unfreeze
        assert!(!manager.is_frozen(&charlie));
    }

    #[test]
    fn test_list_frozen() {
        let mut manager = FreezeManager::new();
        let alice = test_did("alice");
        let bob = test_did("bob");

        manager.freeze(alice.clone(), "Reason A".to_string(), None);
        manager.freeze(bob.clone(), "Reason B".to_string(), Some(3600));

        let frozen_list = manager.list_frozen();
        assert_eq!(frozen_list.len(), 2);
    }

    #[test]
    fn test_freeze_with_metadata() {
        let mut manager = FreezeManager::new();
        let alice = test_did("alice");
        let admin = test_did("admin");

        manager.freeze_with_metadata(
            alice.clone(),
            "Fraud investigation".to_string(),
            Some(86400), // 24 hours
            Some("proposal-123".to_string()),
            Some(admin.clone()),
        );

        let record = manager.get_frozen(&alice).unwrap();
        assert_eq!(record.proposal_id, Some("proposal-123".to_string()));
        assert_eq!(record.frozen_by, Some(admin));
    }

    #[test]
    fn test_unfreeze_history() {
        let mut manager = FreezeManager::new();
        let alice = test_did("alice");

        manager.freeze(alice.clone(), "Test".to_string(), None);
        manager.unfreeze(&alice, "Cleared".to_string());

        let history = manager.unfreeze_history();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].did, alice);
        assert_eq!(history[0].reason, "Cleared");
    }

    #[test]
    fn test_frozen_member_remaining_seconds() {
        let record = FrozenMember::new(test_did("test"), "Test".to_string(), Some(3600));

        // Should have some remaining time
        let remaining = record.remaining_seconds();
        assert!(remaining.is_some());
        assert!(remaining.unwrap() > 0);

        // Indefinite freeze has no remaining time
        let indefinite = FrozenMember::new(test_did("test2"), "Indefinite".to_string(), None);
        assert!(indefinite.remaining_seconds().is_none());
    }

    /// A second, equally valid textual encoding of the principal `did` names.
    fn alternate_spelling(did: &Did) -> Did {
        let bytes = did.identifier_bytes().unwrap();
        let alias = Did::from_str(&format!("did:icn:f{}", hex::encode(bytes))).unwrap();
        assert_ne!(did.as_str(), alias.as_str(), "the spellings must differ");
        assert_eq!(&alias, did, "and name one principal");
        alias
    }

    #[test]
    fn unfreeze_audit_names_the_stored_row_not_the_callers_spelling() {
        // Persistence action and audit fact must refer to the same stored row
        // identity. A release named under an alias deletes the stored spelling's
        // row, so the history entry must name that spelling too — an event
        // naming a DID string that was never a persisted key cannot be
        // reconciled with the store.
        let store: Arc<dyn Store> = Arc::new(icn_store::SledStore::temporary().unwrap());
        let stored = test_did("stored");
        let alias = alternate_spelling(&stored);
        let mut manager = FreezeManager::with_store(store.clone()).unwrap();

        manager.freeze(stored.clone(), "hold".to_string(), None);
        let removed = manager
            .unfreeze(&alias, "released".to_string())
            .expect("the principal was frozen under another spelling");
        assert_eq!(removed.did.as_str(), stored.as_str());
        let event = manager.unfreeze_history().last().unwrap();
        assert_eq!(
            event.did.as_str(),
            stored.as_str(),
            "the audit names the row that was deleted, not the alias the caller typed"
        );
        assert_ne!(event.did.as_str(), alias.as_str());
        assert!(store.scan(FREEZE_PREFIX.as_bytes()).unwrap().is_empty());

        // The metadata path follows the same rule.
        manager.freeze_with_metadata(
            stored.clone(),
            "hold".to_string(),
            None,
            Some("prop-1".to_string()),
            None,
        );
        let removed = manager
            .unfreeze_with_metadata(
                &alias,
                "released".to_string(),
                Some("prop-2".to_string()),
                Some(alias.clone()),
            )
            .expect("the principal was frozen under another spelling");
        assert_eq!(removed.did.as_str(), stored.as_str());
        let event = manager.unfreeze_history().last().unwrap();
        assert_eq!(event.did.as_str(), stored.as_str());
        assert_eq!(event.proposal_id.as_deref(), Some("prop-2"));
        assert!(store.scan(FREEZE_PREFIX.as_bytes()).unwrap().is_empty());
    }
}
