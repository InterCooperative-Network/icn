//! Obligation lifecycle — peer-to-peer financial obligations with state tracking.
//!
//! An [`Obligation`] represents a commitment by one party (debtor) to transfer
//! value to another (creditor) in a specific unit of account, authorized by
//! a governance or member receipt.  Obligations are stored in the ledger KV
//! store and indexed by both creditor and debtor for efficient lookup.
//!
//! # Lifecycle
//!
//! ```text
//! Pending ──► Active ──► Fulfilled
//!    │    \       │
//!    │     ▼      ▼
//!    │   Expired  Expired
//!    │
//!    ▼
//! Disputed ──► Fulfilled
//!          └──► Expired
//! ```
//!
//! Terminal states (`Fulfilled`, `Expired`) accept no further transitions.
//!
//! # Example
//!
//! ```rust,ignore
//! use icn_ledger::obligation::{Obligation, ObligationRegistry, ObligationState};
//! use icn_store::SledStore;
//! use std::sync::Arc;
//!
//! let store = Arc::new(SledStore::temporary()?);
//! let registry = ObligationRegistry::new(store);
//!
//! let ob = Obligation {
//!     id: "ob-001".to_string(),
//!     creditor: alice_did.clone(),
//!     debtor: bob_did.clone(),
//!     unit: "hours".to_string(),
//!     amount: 10,
//!     authorizing_receipt: "gov:receipt:abc123".to_string(),
//!     due_by: Some(1_800_000_000),
//!     state: ObligationState::Pending,
//!     created_at: 1_700_000_000,
//! };
//!
//! let id = registry.create(ob)?;
//! registry.transition_state(&id, ObligationState::Active)?;
//! registry.transition_state(&id, ObligationState::Fulfilled)?;
//! ```

use crate::error::{LedgerError, Result};
use icn_identity::Did;
use icn_store::Store;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Key prefix for obligation records in the KV store.
const OBLIGATION_PREFIX: &str = "obligation:";

/// Key prefix for the creditor-to-obligation index.
const CREDITOR_INDEX_PREFIX: &str = "obligation_creditor:";

/// Key prefix for the debtor-to-obligation index.
const DEBTOR_INDEX_PREFIX: &str = "obligation_debtor:";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Unique identifier for an obligation.
pub type ObligationId = String;

/// Lifecycle state of an obligation.
///
/// Permitted transitions:
/// - `Pending → Active | Expired | Disputed`
/// - `Active → Fulfilled | Expired | Disputed`
/// - `Disputed → Fulfilled | Expired`
/// - `Fulfilled`, `Expired` are terminal (no outbound transitions).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObligationState {
    /// Obligation created but not yet effective.
    Pending,
    /// Obligation is active and awaiting fulfillment.
    Active,
    /// Obligation has been fully satisfied by the debtor.
    Fulfilled,
    /// Obligation lapsed: deadline passed or cancelled by mutual agreement.
    Expired,
    /// Obligation is under dispute; settlement requires explicit resolution.
    Disputed,
}

impl ObligationState {
    /// Returns `true` if no further transitions are possible.
    pub fn is_terminal(self) -> bool {
        matches!(self, ObligationState::Fulfilled | ObligationState::Expired)
    }

    /// Returns `true` if transitioning to `next` is a permitted move.
    pub fn can_transition_to(self, next: ObligationState) -> bool {
        use ObligationState::*;
        matches!(
            (self, next),
            (Pending, Active)
                | (Pending, Expired)
                | (Pending, Disputed)
                | (Active, Fulfilled)
                | (Active, Expired)
                | (Active, Disputed)
                | (Disputed, Fulfilled)
                | (Disputed, Expired)
        )
    }
}

/// A peer-to-peer financial obligation within the cooperative network.
///
/// Records that `debtor` owes `creditor` an `amount` denominated in `unit`,
/// authorized by a governance or member receipt (`authorizing_receipt`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Obligation {
    /// Unique identifier (caller-assigned).
    pub id: ObligationId,
    /// Party to whom the value is owed.
    pub creditor: Did,
    /// Party who owes the value.
    pub debtor: Did,
    /// Unit of account (e.g., `"hours"`, `"USD"`, `"COMM"`).
    pub unit: String,
    /// Amount owed (must be positive).
    pub amount: i64,
    /// Receipt ID that authorized creation of this obligation.
    ///
    /// Typically a governance decision receipt or a member-direct authorization
    /// token, linking the obligation back to the authorizing event.
    pub authorizing_receipt: String,
    /// Optional deadline as a Unix timestamp (seconds).
    ///
    /// `None` means the obligation has no fixed expiry.
    pub due_by: Option<u64>,
    /// Current lifecycle state.
    pub state: ObligationState,
    /// Creation timestamp (Unix seconds).
    pub created_at: u64,
}

// ---------------------------------------------------------------------------
// ObligationRegistry
// ---------------------------------------------------------------------------

/// Registry for tracking obligations backed by a KV store.
///
/// Obligations are persisted under well-known key prefixes and indexed by
/// both creditor and debtor for efficient lookup by party.
pub struct ObligationRegistry {
    store: Arc<dyn Store>,
}

impl ObligationRegistry {
    /// Create a new registry backed by the given store.
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self { store }
    }

    // -- Creation ------------------------------------------------------------

    /// Record a new obligation. Returns the obligation ID on success.
    ///
    /// # Errors
    ///
    /// - `LedgerError::InvalidEntry` if `amount` is not positive.
    /// - `LedgerError::DuplicateEntry` if an obligation with the same ID already exists.
    pub fn create(&self, obligation: Obligation) -> Result<ObligationId> {
        if obligation.amount <= 0 {
            return Err(LedgerError::InvalidEntry(format!(
                "obligation amount must be positive, got {}",
                obligation.amount
            )));
        }

        let key = obligation_key(&obligation.id);
        if self.store.get(key.as_bytes()).map_err(store_err)?.is_some() {
            return Err(LedgerError::DuplicateEntry(format!(
                "obligation already exists: {}",
                obligation.id
            )));
        }

        let id = obligation.id.clone();
        let value = serde_json::to_vec(&obligation)?;
        self.store.put(key.as_bytes(), &value).map_err(store_err)?;

        self.add_creditor_index(&obligation.creditor, &id)?;
        self.add_debtor_index(&obligation.debtor, &id)?;

        Ok(id)
    }

    // -- Queries -------------------------------------------------------------

    /// Retrieve an obligation by ID, or `None` if not found.
    pub fn get(&self, id: &str) -> Result<Option<Obligation>> {
        let key = obligation_key(id);
        match self.store.get(key.as_bytes()).map_err(store_err)? {
            Some(bytes) => Ok(Some(serde_json::from_slice(&bytes)?)),
            None => Ok(None),
        }
    }

    /// List all obligations where `creditor` is the party owed value.
    pub fn list_by_creditor(&self, creditor: &Did) -> Result<Vec<Obligation>> {
        self.fetch_indexed(creditor_index_key(creditor))
    }

    /// List all obligations where `debtor` is the party owing value.
    pub fn list_by_debtor(&self, debtor: &Did) -> Result<Vec<Obligation>> {
        self.fetch_indexed(debtor_index_key(debtor))
    }

    // -- Lifecycle -----------------------------------------------------------

    /// Transition an obligation to a new state.
    ///
    /// # Errors
    ///
    /// - `LedgerError::EntryNotFound` if the obligation does not exist.
    /// - `LedgerError::InvalidEntry` if the transition is not permitted by the
    ///   lifecycle rules (see [`ObligationState::can_transition_to`]).
    pub fn transition_state(&self, id: &str, next_state: ObligationState) -> Result<()> {
        let mut obligation = self
            .get(id)?
            .ok_or_else(|| LedgerError::EntryNotFound(format!("obligation not found: {id}")))?;

        if !obligation.state.can_transition_to(next_state) {
            return Err(LedgerError::InvalidEntry(format!(
                "invalid obligation state transition: {:?} → {:?}",
                obligation.state, next_state
            )));
        }

        obligation.state = next_state;
        let value = serde_json::to_vec(&obligation)?;
        self.store
            .put(obligation_key(id).as_bytes(), &value)
            .map_err(store_err)?;

        Ok(())
    }

    // -- Internal helpers ----------------------------------------------------

    fn fetch_indexed(&self, index_prefix: String) -> Result<Vec<Obligation>> {
        let entries = self
            .store
            .scan(index_prefix.as_bytes())
            .map_err(store_err)?;

        let mut obligations = Vec::with_capacity(entries.len());
        for (key_bytes, _) in entries {
            let key_str = String::from_utf8_lossy(&key_bytes);
            if let Some(obligation_id) = key_str.strip_prefix(&index_prefix) {
                if let Some(ob) = self.get(obligation_id)? {
                    obligations.push(ob);
                }
            }
        }
        Ok(obligations)
    }

    fn add_creditor_index(&self, creditor: &Did, id: &str) -> Result<()> {
        let key = format!("{}{}", creditor_index_key(creditor), id);
        self.store.put(key.as_bytes(), b"1").map_err(store_err)?;
        Ok(())
    }

    fn add_debtor_index(&self, debtor: &Did, id: &str) -> Result<()> {
        let key = format!("{}{}", debtor_index_key(debtor), id);
        self.store.put(key.as_bytes(), b"1").map_err(store_err)?;
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Key helpers
// ---------------------------------------------------------------------------

fn obligation_key(id: &str) -> String {
    format!("{}{}", OBLIGATION_PREFIX, id)
}

fn creditor_index_key(did: &Did) -> String {
    format!("{}{}:", CREDITOR_INDEX_PREFIX, did)
}

fn debtor_index_key(did: &Did) -> String {
    format!("{}{}:", DEBTOR_INDEX_PREFIX, did)
}

fn store_err(e: anyhow::Error) -> LedgerError {
    LedgerError::Storage(e.to_string())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use icn_store::SledStore;
    use icn_time::current_timestamp_secs;

    fn test_registry() -> (ObligationRegistry, Did, Did) {
        let store = Arc::new(SledStore::temporary().unwrap());
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();
        (ObligationRegistry::new(store), alice, bob)
    }

    fn sample_obligation(creditor: &Did, debtor: &Did) -> Obligation {
        Obligation {
            id: "ob-001".to_string(),
            creditor: creditor.clone(),
            debtor: debtor.clone(),
            unit: "hours".to_string(),
            amount: 10,
            authorizing_receipt: "gov:receipt:abc123".to_string(),
            due_by: Some(1_800_000_000),
            state: ObligationState::Pending,
            created_at: current_timestamp_secs(),
        }
    }

    #[test]
    fn test_create_and_get_roundtrip() {
        let (registry, alice, bob) = test_registry();
        let ob = sample_obligation(&alice, &bob);

        let id = registry.create(ob.clone()).unwrap();
        assert_eq!(id, "ob-001");

        let fetched = registry.get(&id).unwrap().expect("should exist");
        assert_eq!(fetched.creditor, alice);
        assert_eq!(fetched.debtor, bob);
        assert_eq!(fetched.unit, "hours");
        assert_eq!(fetched.amount, 10);
        assert_eq!(fetched.state, ObligationState::Pending);
        assert_eq!(fetched.authorizing_receipt, "gov:receipt:abc123");
    }

    #[test]
    fn test_get_nonexistent_returns_none() {
        let (registry, _alice, _bob) = test_registry();
        let result = registry.get("no-such-obligation").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_duplicate_creation_fails() {
        let (registry, alice, bob) = test_registry();
        let ob = sample_obligation(&alice, &bob);

        registry.create(ob.clone()).unwrap();
        let err = registry.create(ob).unwrap_err();
        assert!(
            err.to_string().contains("already exists"),
            "expected duplicate error, got: {err}"
        );
    }

    #[test]
    fn test_non_positive_amount_fails() {
        let (registry, alice, bob) = test_registry();

        let zero_ob = Obligation {
            amount: 0,
            ..sample_obligation(&alice, &bob)
        };
        let err = registry.create(zero_ob).unwrap_err();
        assert!(
            err.to_string().contains("must be positive"),
            "expected positive-amount error, got: {err}"
        );

        let neg_ob = Obligation {
            id: "ob-neg".to_string(),
            amount: -5,
            ..sample_obligation(&alice, &bob)
        };
        let err = registry.create(neg_ob).unwrap_err();
        assert!(
            err.to_string().contains("must be positive"),
            "expected positive-amount error for negative, got: {err}"
        );
    }

    #[test]
    fn test_state_transitions_valid() {
        let (registry, alice, bob) = test_registry();
        let ob = sample_obligation(&alice, &bob);
        let id = registry.create(ob).unwrap();

        // Pending → Active
        registry
            .transition_state(&id, ObligationState::Active)
            .unwrap();
        assert_eq!(
            registry.get(&id).unwrap().unwrap().state,
            ObligationState::Active
        );

        // Active → Fulfilled
        registry
            .transition_state(&id, ObligationState::Fulfilled)
            .unwrap();
        assert_eq!(
            registry.get(&id).unwrap().unwrap().state,
            ObligationState::Fulfilled
        );
    }

    #[test]
    fn test_terminal_state_blocks_transition() {
        let (registry, alice, bob) = test_registry();
        let ob = sample_obligation(&alice, &bob);
        let id = registry.create(ob).unwrap();

        registry
            .transition_state(&id, ObligationState::Active)
            .unwrap();
        registry
            .transition_state(&id, ObligationState::Fulfilled)
            .unwrap();

        // Fulfilled is terminal — any further transition must fail
        let err = registry
            .transition_state(&id, ObligationState::Expired)
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("invalid obligation state transition"),
            "expected invalid-transition error, got: {err}"
        );
    }

    #[test]
    fn test_pending_to_expired_valid() {
        let (registry, alice, bob) = test_registry();
        let ob = sample_obligation(&alice, &bob);
        let id = registry.create(ob).unwrap();

        registry
            .transition_state(&id, ObligationState::Expired)
            .unwrap();
        assert_eq!(
            registry.get(&id).unwrap().unwrap().state,
            ObligationState::Expired
        );
    }

    #[test]
    fn test_dispute_and_resolve() {
        let (registry, alice, bob) = test_registry();
        let ob = sample_obligation(&alice, &bob);
        let id = registry.create(ob).unwrap();

        // Pending → Active → Disputed → Fulfilled
        registry
            .transition_state(&id, ObligationState::Active)
            .unwrap();
        registry
            .transition_state(&id, ObligationState::Disputed)
            .unwrap();
        registry
            .transition_state(&id, ObligationState::Fulfilled)
            .unwrap();

        assert_eq!(
            registry.get(&id).unwrap().unwrap().state,
            ObligationState::Fulfilled
        );
    }

    #[test]
    fn test_invalid_transition_fails() {
        let (registry, alice, bob) = test_registry();
        let ob = sample_obligation(&alice, &bob);
        let id = registry.create(ob).unwrap();

        // Pending → Fulfilled is not permitted directly
        let err = registry
            .transition_state(&id, ObligationState::Fulfilled)
            .unwrap_err();
        assert!(
            err.to_string()
                .contains("invalid obligation state transition"),
            "expected invalid-transition error, got: {err}"
        );
    }

    #[test]
    fn test_transition_nonexistent_fails() {
        let (registry, _alice, _bob) = test_registry();
        let err = registry
            .transition_state("no-such", ObligationState::Active)
            .unwrap_err();
        assert!(
            err.to_string().contains("not found"),
            "expected not-found error, got: {err}"
        );
    }

    #[test]
    fn test_list_by_creditor_and_debtor() {
        let (registry, alice, bob) = test_registry();

        // Alice is creditor for ob-001 and ob-002; Bob is creditor for ob-003
        let ob1 = sample_obligation(&alice, &bob);
        let ob2 = Obligation {
            id: "ob-002".to_string(),
            ..sample_obligation(&alice, &bob)
        };
        let ob3 = Obligation {
            id: "ob-003".to_string(),
            ..sample_obligation(&bob, &alice)
        };

        registry.create(ob1).unwrap();
        registry.create(ob2).unwrap();
        registry.create(ob3).unwrap();

        let alice_as_creditor = registry.list_by_creditor(&alice).unwrap();
        assert_eq!(alice_as_creditor.len(), 2);
        assert!(alice_as_creditor.iter().all(|o| o.creditor == alice));

        let bob_as_creditor = registry.list_by_creditor(&bob).unwrap();
        assert_eq!(bob_as_creditor.len(), 1);
        assert_eq!(bob_as_creditor[0].id, "ob-003");

        let bob_as_debtor = registry.list_by_debtor(&bob).unwrap();
        assert_eq!(bob_as_debtor.len(), 2);
        assert!(bob_as_debtor.iter().all(|o| o.debtor == bob));
    }

    #[test]
    fn test_serde_roundtrip() {
        let alice = KeyPair::generate().unwrap().did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();
        let ob = sample_obligation(&alice, &bob);

        let json = serde_json::to_string(&ob).unwrap();
        let de: Obligation = serde_json::from_str(&json).unwrap();

        assert_eq!(de.id, ob.id);
        assert_eq!(de.creditor, ob.creditor);
        assert_eq!(de.debtor, ob.debtor);
        assert_eq!(de.unit, ob.unit);
        assert_eq!(de.amount, ob.amount);
        assert_eq!(de.authorizing_receipt, ob.authorizing_receipt);
        assert_eq!(de.due_by, ob.due_by);
        assert_eq!(de.state, ob.state);
    }

    #[test]
    fn test_obligation_without_due_by() {
        let (registry, alice, bob) = test_registry();
        let ob = Obligation {
            due_by: None,
            ..sample_obligation(&alice, &bob)
        };

        let id = registry.create(ob).unwrap();
        let fetched = registry.get(&id).unwrap().unwrap();
        assert!(fetched.due_by.is_none());
    }

    #[test]
    fn test_is_terminal() {
        assert!(!ObligationState::Pending.is_terminal());
        assert!(!ObligationState::Active.is_terminal());
        assert!(!ObligationState::Disputed.is_terminal());
        assert!(ObligationState::Fulfilled.is_terminal());
        assert!(ObligationState::Expired.is_terminal());
    }
}
