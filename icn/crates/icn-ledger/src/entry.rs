//! Journal entry creation and validation

use crate::types::{AccountDelta, ContentHash, JournalEntry};
use anyhow::{bail, Result};
use icn_identity::Did;
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH};

/// Builder for creating valid journal entries
pub struct JournalEntryBuilder {
    author: Did,
    contract_ref: Option<ContentHash>,
    accounts: Vec<AccountDelta>,
    parents: Vec<ContentHash>,
}

impl JournalEntryBuilder {
    /// Create a new builder with the given author
    pub fn new(author: Did) -> Self {
        JournalEntryBuilder {
            author,
            contract_ref: None,
            accounts: Vec::new(),
            parents: Vec::new(),
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

    /// Build and validate the journal entry
    pub fn build(self) -> Result<JournalEntry> {
        // Validate double-entry invariant: Σ debits == Σ credits per currency
        validate_double_entry(&self.accounts)?;

        // Validate that amounts are positive
        validate_positive_amounts(&self.accounts)?;

        // Get current timestamp
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)?
            .as_millis() as u64;

        let mut entry = JournalEntry {
            id: None,
            timestamp,
            author: self.author,
            contract_ref: self.contract_ref,
            accounts: self.accounts,
            parents: self.parents,
            signature: None, // Will be set by caller
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
                "Double-entry invariant violated for currency '{}': debits={}, credits={}",
                currency,
                debits,
                credits
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

        assert!(
            entry.is_err(),
            "Unbalanced entry should fail validation"
        );
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
}
