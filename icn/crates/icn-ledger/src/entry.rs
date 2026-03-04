//! Journal entry creation and validation

use crate::types::{AccountDelta, ContentHash, JournalEntry, ProvenanceRef};
use anyhow::{bail, Result};
use icn_identity::Did;
use std::collections::HashMap;

/// Builder for creating valid journal entries
pub struct JournalEntryBuilder {
    author: Did,
    contract_ref: Option<ContentHash>,
    accounts: Vec<AccountDelta>,
    parents: Vec<ContentHash>,
    nonce: Option<[u8; 32]>,
    provenance: Option<ProvenanceRef>,
}

impl JournalEntryBuilder {
    /// Create a new builder with the given author
    pub fn new(author: Did) -> Self {
        JournalEntryBuilder {
            author,
            contract_ref: None,
            accounts: Vec::new(),
            parents: Vec::new(),
            nonce: None,
            provenance: None,
        }
    }

    /// Set the contract reference
    pub fn contract_ref(mut self, contract_ref: ContentHash) -> Self {
        self.contract_ref = Some(contract_ref);
        self
    }

    /// Add a debit to an account
    pub fn debit(mut self, account_id: Did, currency: String, amount: i64) -> Self {
        self.accounts
            .push(AccountDelta::debit(account_id, currency, amount));
        self
    }

    /// Add a credit to an account
    pub fn credit(mut self, account_id: Did, currency: String, amount: i64) -> Self {
        self.accounts
            .push(AccountDelta::credit(account_id, currency, amount));
        self
    }

    /// Add an account delta
    pub fn add_delta(mut self, delta: AccountDelta) -> Self {
        self.accounts.push(delta);
        self
    }

    /// Add a parent entry (for Merkle-DAG)
    pub fn add_parent(mut self, parent: ContentHash) -> Self {
        self.parents.push(parent);
        self
    }

    /// Set a nonce for replay protection.
    ///
    /// When set, the nonce is included in the content hash, ensuring
    /// entries with identical content produce distinct hashes. Use this
    /// for commons credit entries where the nonce is the `receipt_id`.
    pub fn nonce(mut self, nonce: [u8; 32]) -> Self {
        self.nonce = Some(nonce);
        self
    }

    /// Set governance decision provenance.
    ///
    /// Use when this entry was authorized by a governance proposal/vote.
    ///
    /// # Arguments
    /// * `receipt_id` - Node-local decision receipt ID
    /// * `decision_hash` - Canonical cross-node hash anchor
    pub fn with_governance_provenance(
        mut self,
        receipt_id: impl Into<String>,
        decision_hash: impl Into<String>,
    ) -> Self {
        self.provenance = Some(ProvenanceRef::Governance {
            receipt_id: receipt_id.into(),
            decision_hash: decision_hash.into(),
        });
        self
    }

    /// Set governance decision provenance (legacy name, same as `with_governance_provenance`).
    pub fn with_decision_provenance(
        self,
        receipt_id: impl Into<String>,
        decision_hash: impl Into<String>,
    ) -> Self {
        self.with_governance_provenance(receipt_id, decision_hash)
    }

    /// Set system-generated provenance.
    ///
    /// Use for internal operations: FX transfers, DID migration, mint/burn.
    pub fn with_system_provenance(mut self, reason: impl Into<String>) -> Self {
        self.provenance = Some(ProvenanceRef::SystemGenerated {
            reason: reason.into(),
        });
        self
    }

    /// Build and validate the journal entry.
    ///
    /// Returns `Err` if:
    /// - Double-entry invariant is violated (Σ debits ≠ Σ credits per currency)
    /// - Any amount is negative
    /// - No provenance was set (use `with_governance_provenance` or `with_system_provenance`)
    /// - The system clock is unavailable
    pub fn build(self) -> Result<JournalEntry> {
        // Validate double-entry invariant: Σ debits == Σ credits per currency
        validate_double_entry(&self.accounts)?;

        // Validate that amounts are positive
        validate_positive_amounts(&self.accounts)?;

        // Require provenance — every entry must be traceable
        let provenance = self
            .provenance
            .ok_or_else(|| anyhow::anyhow!("JournalEntry requires provenance; call with_governance_provenance() or with_system_provenance()"))?;

        // Get current timestamp in milliseconds (security-critical: reject if clock is invalid)
        let timestamp = icn_time::try_current_timestamp_millis()
            .map_err(|e| anyhow::anyhow!("Cannot create journal entry: {e}"))?;

        let mut entry = JournalEntry {
            id: None,
            timestamp,
            author: self.author,
            contract_ref: self.contract_ref,
            accounts: self.accounts,
            parents: self.parents,
            signature: None, // Will be set by caller
            nonce: self.nonce,
            provenance,
        };

        // Compute the content hash
        entry.compute_hash()?;

        Ok(entry)
    }
}

/// Validate double-entry bookkeeping invariant
/// For each currency, the sum of debits must equal the sum of credits
fn validate_double_entry(accounts: &[AccountDelta]) -> Result<()> {
    // Group by currency and sum debits/credits
    let mut currency_totals: HashMap<String, (i64, i64)> = HashMap::new();

    for delta in accounts {
        let (total_debits, total_credits) = currency_totals
            .entry(delta.currency.clone())
            .or_insert((0, 0));

        *total_debits += delta.debit.unwrap_or(0);
        *total_credits += delta.credit.unwrap_or(0);
    }

    // Check that debits == credits for each currency
    for (currency, (debits, credits)) in currency_totals.iter() {
        if debits != credits {
            bail!(
                "Double-entry invariant violated for currency '{currency}': debits={debits}, credits={credits}"
            );
        }
    }

    Ok(())
}

/// Validate that all amounts are positive
fn validate_positive_amounts(accounts: &[AccountDelta]) -> Result<()> {
    for delta in accounts {
        if let Some(debit) = delta.debit {
            if debit < 0 {
                bail!(
                    "Negative debit amount not allowed: {} for account {}",
                    debit,
                    delta.account_id
                );
            }
        }

        if let Some(credit) = delta.credit {
            if credit < 0 {
                bail!(
                    "Negative credit amount not allowed: {} for account {}",
                    credit,
                    delta.account_id
                );
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;

    #[test]
    fn test_valid_entry_creation() {
        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .with_system_provenance("test")
            .build();

        assert!(entry.is_ok(), "Valid entry should build successfully");

        let entry = entry.unwrap();
        assert!(entry.id.is_some(), "Entry should have a computed hash");
        assert_eq!(entry.accounts.len(), 2);
    }

    #[test]
    fn test_unbalanced_entry_fails() {
        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 5) // Unbalanced!
            .with_system_provenance("test")
            .build();

        assert!(entry.is_err(), "Unbalanced entry should fail validation");
        assert!(entry
            .unwrap_err()
            .to_string()
            .contains("Double-entry invariant violated"));
    }

    #[test]
    fn test_negative_amount_fails() {
        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), -10) // Negative!
            .credit(bob.clone(), "hours".to_string(), -10) // Negative!
            .with_system_provenance("test")
            .build();

        assert!(entry.is_err(), "Negative amounts should fail validation");
        assert!(entry
            .unwrap_err()
            .to_string()
            .contains("Negative debit amount"));
    }

    #[test]
    fn test_multi_currency_entry() {
        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .debit(alice.clone(), "USD".to_string(), 100)
            .credit(bob.clone(), "USD".to_string(), 100)
            .with_system_provenance("test")
            .build();

        assert!(
            entry.is_ok(),
            "Multi-currency entry should build successfully"
        );

        let entry = entry.unwrap();
        assert_eq!(entry.accounts.len(), 4);
    }

    #[test]
    fn test_multi_currency_unbalanced_fails() {
        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .debit(alice.clone(), "USD".to_string(), 100)
            .credit(bob.clone(), "USD".to_string(), 50) // Unbalanced USD!
            .with_system_provenance("test")
            .build();

        assert!(
            entry.is_err(),
            "Unbalanced multi-currency entry should fail"
        );
        assert!(entry.unwrap_err().to_string().contains("USD"));
    }

    /// Golden-path test: JournalEntry with governance provenance
    ///
    /// Verifies:
    /// - `ProvenanceRef::Governance` is preserved through build
    /// - Provenance round-trips through JSON serialization
    #[test]
    fn test_entry_with_governance_provenance() {
        let keypair = KeyPair::generate().unwrap();
        let treasury = keypair.did().clone();
        let recipient = KeyPair::generate().unwrap().did().clone();

        let receipt_id = "gov:proposal:2024-001:receipt:abc123";
        let decision_hash = "sha256:def456789...";

        let entry = JournalEntryBuilder::new(treasury.clone())
            .debit(treasury.clone(), "HOURS".to_string(), 2500)
            .credit(recipient.clone(), "HOURS".to_string(), 2500)
            .with_governance_provenance(receipt_id, decision_hash)
            .build()
            .expect("Entry with governance provenance should build successfully");

        // Verify provenance variant
        match &entry.provenance {
            ProvenanceRef::Governance {
                receipt_id: r,
                decision_hash: h,
            } => {
                assert_eq!(r, receipt_id);
                assert_eq!(h, decision_hash);
            }
            other => panic!("Expected Governance provenance, got {other:?}"),
        }

        assert!(entry.id.is_some(), "Entry should have computed hash");

        // Round-trip through JSON
        let json = serde_json::to_string(&entry).expect("should serialize");
        assert!(json.contains(receipt_id), "JSON should contain receipt_id");
        assert!(
            json.contains(decision_hash),
            "JSON should contain decision_hash"
        );
        let deserialized: JournalEntry = serde_json::from_str(&json).expect("should deserialize");
        assert_eq!(deserialized.provenance, entry.provenance);
    }

    /// Test that build() fails when no provenance is set.
    #[test]
    fn test_missing_provenance_fails() {
        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let result = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 5)
            .credit(bob.clone(), "hours".to_string(), 5)
            .build();

        assert!(result.is_err(), "build() without provenance must fail");
        assert!(
            result.unwrap_err().to_string().contains("provenance"),
            "error message should mention provenance"
        );
    }

    /// Test that SystemGenerated provenance round-trips correctly.
    #[test]
    fn test_system_provenance_roundtrip() {
        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 5)
            .credit(bob.clone(), "hours".to_string(), 5)
            .with_system_provenance("fx-transfer")
            .build()
            .expect("system provenance entry should build");

        match &entry.provenance {
            ProvenanceRef::SystemGenerated { reason } => assert_eq!(reason, "fx-transfer"),
            other => panic!("Expected SystemGenerated, got {other:?}"),
        }

        let json = serde_json::to_string(&entry).unwrap();
        let de: JournalEntry = serde_json::from_str(&json).unwrap();
        assert_eq!(de.provenance, entry.provenance);
    }
}
