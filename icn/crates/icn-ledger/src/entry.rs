//! Journal entry creation and validation

use crate::types::{AccountDelta, ContentHash, JournalEntry};
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
    decision_receipt_id: Option<String>,
    decision_hash: Option<String>,
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
            decision_receipt_id: None,
            decision_hash: None,
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

    /// Set the governance decision provenance.
    ///
    /// Links this ledger entry to the governance decision that authorized it.
    /// Both fields should be set together for complete provenance.
    ///
    /// # Arguments
    /// * `receipt_id` - Node-local decision receipt ID (e.g., "gov:proposal:2024-001:receipt:abc")
    /// * `hash` - Canonical decision hash (cross-node anchor, e.g., "sha256:abc123...")
    pub fn with_decision_provenance(
        mut self,
        receipt_id: impl Into<String>,
        hash: impl Into<String>,
    ) -> Self {
        self.decision_receipt_id = Some(receipt_id.into());
        self.decision_hash = Some(hash.into());
        self
    }

    /// Build and validate the journal entry
    pub fn build(self) -> Result<JournalEntry> {
        // Validate double-entry invariant: Σ debits == Σ credits per currency
        validate_double_entry(&self.accounts)?;

        // Validate that amounts are positive
        validate_positive_amounts(&self.accounts)?;

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
            decision_receipt_id: self.decision_receipt_id,
            decision_hash: self.decision_hash,
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
            .build();

        assert!(
            entry.is_err(),
            "Unbalanced multi-currency entry should fail"
        );
        assert!(entry.unwrap_err().to_string().contains("USD"));
    }

    /// Golden-path test: JournalEntry with decision provenance
    ///
    /// This test verifies the pilot invariant:
    /// - Ledger entries carry decision_receipt_id and decision_hash
    /// - These fields are preserved through serialization
    /// - The entry can be traced back to its authorizing decision
    #[test]
    fn test_entry_with_decision_provenance() {
        let keypair = KeyPair::generate().unwrap();
        let treasury = keypair.did().clone();
        let recipient = KeyPair::generate().unwrap().did().clone();

        let decision_receipt_id = "gov:proposal:2024-001:receipt:abc123";
        let decision_hash = "sha256:def456789...";

        let entry = JournalEntryBuilder::new(treasury.clone())
            .debit(treasury.clone(), "HOURS".to_string(), 2500)
            .credit(recipient.clone(), "HOURS".to_string(), 2500)
            .with_decision_provenance(decision_receipt_id, decision_hash)
            .build()
            .expect("Entry with provenance should build successfully");

        // Verify provenance fields are set
        assert_eq!(
            entry.decision_receipt_id.as_deref(),
            Some(decision_receipt_id),
            "decision_receipt_id must be preserved"
        );
        assert_eq!(
            entry.decision_hash.as_deref(),
            Some(decision_hash),
            "decision_hash must be preserved"
        );

        // Verify entry is valid
        assert!(entry.id.is_some(), "Entry should have computed hash");

        // Verify serialization preserves provenance
        let json = serde_json::to_string(&entry).expect("should serialize");
        assert!(
            json.contains(decision_receipt_id),
            "JSON should contain decision_receipt_id"
        );
        assert!(
            json.contains(decision_hash),
            "JSON should contain decision_hash"
        );

        // Verify deserialization preserves provenance
        let deserialized: JournalEntry =
            serde_json::from_str(&json).expect("should deserialize");
        assert_eq!(
            deserialized.decision_receipt_id,
            entry.decision_receipt_id,
            "decision_receipt_id must survive round-trip"
        );
        assert_eq!(
            deserialized.decision_hash,
            entry.decision_hash,
            "decision_hash must survive round-trip"
        );
    }
}
