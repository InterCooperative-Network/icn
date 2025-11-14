//! Balance computation and queries

use crate::types::{AccountBalances, JournalEntry};
use icn_identity::Did;
use std::collections::HashMap;

/// Compute balances for all accounts from a sequence of journal entries
pub fn compute_all_balances(entries: &[JournalEntry]) -> HashMap<Did, AccountBalances> {
    let mut balances: HashMap<Did, AccountBalances> = HashMap::new();

    for entry in entries {
        for delta in &entry.accounts {
            let account_balances = balances
                .entry(delta.account_id.clone())
                .or_insert_with(|| AccountBalances::new(delta.account_id.clone()));

            account_balances.apply_delta(delta);
        }
    }

    balances
}

/// Compute balance for a specific account from journal entries
pub fn compute_account_balance(account_id: &Did, entries: &[JournalEntry]) -> AccountBalances {
    let mut balance = AccountBalances::new(account_id.clone());

    for entry in entries {
        for delta in &entry.accounts {
            if &delta.account_id == account_id {
                balance.apply_delta(delta);
            }
        }
    }

    balance
}

/// Compute balance for a specific account and currency
pub fn compute_balance(
    account_id: &Did,
    currency: &str,
    entries: &[JournalEntry],
) -> i64 {
    let mut balance = 0i64;

    for entry in entries {
        for delta in &entry.accounts {
            if &delta.account_id == account_id && delta.currency == currency {
                balance += delta.net_change();
            }
        }
    }

    balance
}

/// Validate that an entry doesn't cause overdraft beyond credit limits
pub fn validate_credit_limit(
    entry: &JournalEntry,
    current_balances: &HashMap<Did, AccountBalances>,
    credit_limits: &HashMap<(Did, String), i64>,
) -> Result<(), String> {
    // Compute new balances after applying this entry
    let mut test_balances = current_balances.clone();

    for delta in &entry.accounts {
        let account_balances = test_balances
            .entry(delta.account_id.clone())
            .or_insert_with(|| AccountBalances::new(delta.account_id.clone()));

        account_balances.apply_delta(delta);
    }

    // Check credit limits
    for (account_id, balances) in test_balances.iter() {
        for (currency, balance) in balances.balances.iter() {
            if let Some(limit) = credit_limits.get(&(account_id.clone(), currency.clone())) {
                if balance < limit {
                    return Err(format!(
                        "Account {} would exceed credit limit for {}: balance={}, limit={}",
                        account_id, currency, balance, limit
                    ));
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entry::JournalEntryBuilder;
    use icn_identity::KeyPair;

    #[test]
    fn test_compute_balance() {
        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Alice receives 10 hours from Bob
        let entry1 = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .build()
            .unwrap();

        // Alice receives another 5 hours
        let entry2 = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 5)
            .credit(bob.clone(), "hours".to_string(), 5)
            .build()
            .unwrap();

        let entries = vec![entry1, entry2];

        // Alice should have +15 hours (she's owed 15)
        let alice_balance = compute_balance(&alice, "hours", &entries);
        assert_eq!(alice_balance, 15);

        // Bob should have -15 hours (he owes 15)
        let bob_balance = compute_balance(&bob, "hours", &entries);
        assert_eq!(bob_balance, -15);
    }

    #[test]
    fn test_compute_account_balance_multi_currency() {
        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .debit(alice.clone(), "USD".to_string(), 100)
            .credit(bob.clone(), "USD".to_string(), 100)
            .build()
            .unwrap();

        let entries = vec![entry];

        let alice_balances = compute_account_balance(&alice, &entries);

        assert_eq!(alice_balances.get("hours"), 10);
        assert_eq!(alice_balances.get("USD"), 100);
    }

    #[test]
    fn test_compute_all_balances() {
        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();
        let charlie = KeyPair::generate().unwrap().did().clone();

        let entry1 = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 10)
            .credit(bob.clone(), "hours".to_string(), 10)
            .build()
            .unwrap();

        let entry2 = JournalEntryBuilder::new(bob.clone())
            .debit(bob.clone(), "hours".to_string(), 5)
            .credit(charlie.clone(), "hours".to_string(), 5)
            .build()
            .unwrap();

        let entries = vec![entry1, entry2];

        let all_balances = compute_all_balances(&entries);

        assert_eq!(all_balances.len(), 3);
        assert_eq!(all_balances.get(&alice).unwrap().get("hours"), 10);
        assert_eq!(all_balances.get(&bob).unwrap().get("hours"), -5); // -10 + 5
        assert_eq!(all_balances.get(&charlie).unwrap().get("hours"), -5);
    }

    #[test]
    fn test_validate_credit_limit() {
        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        // Set up current balances
        let mut current_balances = HashMap::new();
        let mut bob_balances = AccountBalances::new(bob.clone());
        bob_balances.balances.insert("hours".to_string(), -50); // Bob already owes 50
        current_balances.insert(bob.clone(), bob_balances);

        // Set credit limit: Bob can owe at most 100 hours
        let mut credit_limits = HashMap::new();
        credit_limits.insert((bob.clone(), "hours".to_string()), -100);

        // Try to create entry that would put Bob at -110 hours (exceeds limit)
        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 60)
            .credit(bob.clone(), "hours".to_string(), 60)
            .build()
            .unwrap();

        let result = validate_credit_limit(&entry, &current_balances, &credit_limits);

        assert!(result.is_err(), "Should fail credit limit check");
        assert!(result.unwrap_err().contains("credit limit"));
    }

    #[test]
    fn test_validate_credit_limit_passes() {
        let keypair = KeyPair::generate().unwrap();
        let alice = keypair.did().clone();
        let bob = KeyPair::generate().unwrap().did().clone();

        let mut current_balances = HashMap::new();
        let bob_balances = AccountBalances::new(bob.clone());
        current_balances.insert(bob.clone(), bob_balances);

        let mut credit_limits = HashMap::new();
        credit_limits.insert((bob.clone(), "hours".to_string()), -100);

        // Bob owes 50, which is within the limit of -100
        let entry = JournalEntryBuilder::new(alice.clone())
            .debit(alice.clone(), "hours".to_string(), 50)
            .credit(bob.clone(), "hours".to_string(), 50)
            .build()
            .unwrap();

        let result = validate_credit_limit(&entry, &current_balances, &credit_limits);

        assert!(result.is_ok(), "Should pass credit limit check");
    }
}
