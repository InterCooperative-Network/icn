use crate::error::{LifecycleError, LifecycleResult};
use crate::types::PartitionMap;
use icn_economics::Ledger;
use icn_identity::Did;
use std::collections::HashMap;
use tracing::{debug, info, warn};

/// Union two ledgers from different federations into a single ledger
/// 
/// This function is marked with #[cfg(test)] to make it directly testable
/// but remains available for internal use.
#[cfg(test)]
pub fn union_ledgers(ledger_a: &Ledger, ledger_b: &Ledger) -> LifecycleResult<Ledger> {
    // Create a new ledger
    let mut result_ledger = Ledger::new();
    
    // Process ledger_a
    for (account, balance) in ledger_a.accounts() {
        // Add account to result ledger
        result_ledger.create_account(account).map_err(|e| {
            LifecycleError::LedgerOperationFailed(format!("Failed to create account from ledger A: {}", e))
        })?;
        
        // Set balance
        if *balance > 0 {
            result_ledger.credit(account, *balance).map_err(|e| {
                LifecycleError::LedgerOperationFailed(format!("Failed to credit account from ledger A: {}", e))
            })?;
        }
    }
    
    // Process ledger_b, handling overlapping accounts
    for (account, balance) in ledger_b.accounts() {
        if result_ledger.has_account(account) {
            // Account exists in both ledgers, add the balance
            if *balance > 0 {
                result_ledger.credit(account, *balance).map_err(|e| {
                    LifecycleError::LedgerOperationFailed(format!("Failed to credit account from ledger B: {}", e))
                })?;
            }
            
            // Log warning about duplicate account
            warn!("Account {} exists in both ledgers, balances have been combined", account);
        } else {
            // New account, add it
            result_ledger.create_account(account).map_err(|e| {
                LifecycleError::LedgerOperationFailed(format!("Failed to create account from ledger B: {}", e))
            })?;
            
            // Set balance
            if *balance > 0 {
                result_ledger.credit(account, *balance).map_err(|e| {
                    LifecycleError::LedgerOperationFailed(format!("Failed to credit account from ledger B: {}", e))
                })?;
            }
        }
    }
    
    debug!("Ledger union created with {} accounts", result_ledger.accounts().len());
    Ok(result_ledger)
}

/// Core functionality for unioning ledgers, not marked with #[cfg(test)]
/// so it can be used in production code
pub fn union_ledgers_impl(ledger_a: &Ledger, ledger_b: &Ledger) -> LifecycleResult<Ledger> {
    union_ledgers(ledger_a, ledger_b)
}

/// Shard a ledger into two based on a partition map
/// 
/// This function is marked with #[cfg(test)] to make it directly testable
/// but remains available for internal use.
#[cfg(test)]
pub fn shard_ledger(parent_ledger: &Ledger, partition_map: &PartitionMap) -> LifecycleResult<(Ledger, Ledger)> {
    // Create two new ledgers
    let mut ledger_a = Ledger::new();
    let mut ledger_b = Ledger::new();
    
    // Process accounts based on the partition map
    for (account, balance) in parent_ledger.accounts() {
        let account_did = account.clone();
        
        // Determine which partition this account belongs to
        if partition_map.members_a.contains(&account_did) {
            // Add to ledger A
            ledger_a.create_account(&account_did).map_err(|e| {
                LifecycleError::LedgerOperationFailed(format!("Failed to create account in ledger A: {}", e))
            })?;
            
            // Check if there's a specific balance in the partition map
            let mapped_balance = partition_map.ledger_a.get(&account_did).unwrap_or(balance);
            
            // Set balance
            if *mapped_balance > 0 {
                ledger_a.credit(&account_did, *mapped_balance).map_err(|e| {
                    LifecycleError::LedgerOperationFailed(format!("Failed to credit account in ledger A: {}", e))
                })?;
            }
        } else if partition_map.members_b.contains(&account_did) {
            // Add to ledger B
            ledger_b.create_account(&account_did).map_err(|e| {
                LifecycleError::LedgerOperationFailed(format!("Failed to create account in ledger B: {}", e))
            })?;
            
            // Check if there's a specific balance in the partition map
            let mapped_balance = partition_map.ledger_b.get(&account_did).unwrap_or(balance);
            
            // Set balance
            if *mapped_balance > 0 {
                ledger_b.credit(&account_did, *mapped_balance).map_err(|e| {
                    LifecycleError::LedgerOperationFailed(format!("Failed to credit account in ledger B: {}", e))
                })?;
            }
        } else {
            // Account not in either partition, log warning
            warn!("Account {} not found in partition map, balance will be lost", account);
        }
    }
    
    // Verify economic consistency - total balance must be preserved
    verify_economic_balance(parent_ledger, &ledger_a, &ledger_b)?;
    
    debug!("Ledger sharded into {} and {} accounts", 
           ledger_a.accounts().len(), ledger_b.accounts().len());
    
    Ok((ledger_a, ledger_b))
}

/// Core functionality for sharding ledgers, not marked with #[cfg(test)]
/// so it can be used in production code
pub fn shard_ledger_impl(parent_ledger: &Ledger, partition_map: &PartitionMap) -> LifecycleResult<(Ledger, Ledger)> {
    shard_ledger(parent_ledger, partition_map)
}

/// Verify that economic balance is preserved during operations
fn verify_economic_balance(
    parent_ledger: &Ledger,
    ledger_a: &Ledger,
    ledger_b: &Ledger,
) -> LifecycleResult<()> {
    // Calculate total balance in parent ledger
    let mut parent_total: u64 = parent_ledger.accounts().values().sum();
    
    // Calculate total balance in child ledgers
    let child_a_total: u64 = ledger_a.accounts().values().sum();
    let child_b_total: u64 = ledger_b.accounts().values().sum();
    let child_total = child_a_total + child_b_total;
    
    // Verify total balance is preserved
    if parent_total != child_total {
        return Err(LifecycleError::EconomicInconsistency(format!(
            "Balance mismatch: parent total {} != child total {}", parent_total, child_total
        )));
    }
    
    Ok(())
}

/// Create a transfer plan for migrating tokens between federations
pub fn create_transfer_plan(
    source_ledger: &Ledger,
    partition_map: &PartitionMap,
) -> LifecycleResult<HashMap<Did, HashMap<Did, u64>>> {
    let mut transfer_plan = HashMap::new();
    
    // Process each account in the source ledger
    for (account, balance) in source_ledger.accounts() {
        let account_did = account.clone();
        
        // Skip accounts with zero balance
        if *balance == 0 {
            continue;
        }
        
        // Determine which partition this account belongs to
        if partition_map.members_a.contains(&account_did) {
            // Add transfer plan for federation A
            let federation_a_transfers = transfer_plan
                .entry(partition_map.members_a[0].clone()) // Use first member as federation ID
                .or_insert_with(HashMap::new);
                
            federation_a_transfers.insert(account_did, *balance);
        } else if partition_map.members_b.contains(&account_did) {
            // Add transfer plan for federation B
            let federation_b_transfers = transfer_plan
                .entry(partition_map.members_b[0].clone()) // Use first member as federation ID
                .or_insert_with(HashMap::new);
                
            federation_b_transfers.insert(account_did, *balance);
        }
    }
    
    Ok(transfer_plan)
}

#[cfg(test)]
mod tests {
    use super::*;
    
    // Helper function to create a test ledger
    fn create_test_ledger() -> Ledger {
        let mut ledger = Ledger::new();
        
        // Create some test accounts
        let accounts = [
            "did:icn:test:1",
            "did:icn:test:2",
            "did:icn:test:3",
            "did:icn:test:4",
        ];
        
        for account in accounts.iter() {
            ledger.create_account(account).unwrap();
        }
        
        // Set some balances
        ledger.credit("did:icn:test:1", 100).unwrap();
        ledger.credit("did:icn:test:2", 200).unwrap();
        ledger.credit("did:icn:test:3", 300).unwrap();
        ledger.credit("did:icn:test:4", 400).unwrap();
        
        ledger
    }
    
    // Helper function to create a second test ledger
    fn create_test_ledger_2() -> Ledger {
        let mut ledger = Ledger::new();
        
        // Create some test accounts
        let accounts = [
            "did:icn:test:3", // Overlapping with first ledger
            "did:icn:test:4", // Overlapping with first ledger
            "did:icn:test:5",
            "did:icn:test:6",
        ];
        
        for account in accounts.iter() {
            ledger.create_account(account).unwrap();
        }
        
        // Set some balances
        ledger.credit("did:icn:test:3", 30).unwrap();
        ledger.credit("did:icn:test:4", 40).unwrap();
        ledger.credit("did:icn:test:5", 500).unwrap();
        ledger.credit("did:icn:test:6", 600).unwrap();
        
        ledger
    }
    
    // Helper function to create a test partition map
    fn create_test_partition_map() -> PartitionMap {
        PartitionMap {
            members_a: vec![
                "did:icn:test:1".to_string(),
                "did:icn:test:2".to_string(),
            ],
            members_b: vec![
                "did:icn:test:3".to_string(),
                "did:icn:test:4".to_string(),
            ],
            resources_a: HashMap::new(),
            resources_b: HashMap::new(),
            ledger_a: HashMap::new(),
            ledger_b: HashMap::new(),
        }
    }
    
    #[test]
    fn test_union_ledgers() {
        let ledger_a = create_test_ledger();
        let ledger_b = create_test_ledger_2();
        
        let result = union_ledgers(&ledger_a, &ledger_b).unwrap();
        
        // Verify accounts
        assert!(result.has_account("did:icn:test:1"));
        assert!(result.has_account("did:icn:test:2"));
        assert!(result.has_account("did:icn:test:3"));
        assert!(result.has_account("did:icn:test:4"));
        assert!(result.has_account("did:icn:test:5"));
        assert!(result.has_account("did:icn:test:6"));
        
        // Verify balances
        assert_eq!(result.balance("did:icn:test:1").unwrap(), 100);
        assert_eq!(result.balance("did:icn:test:2").unwrap(), 200);
        assert_eq!(result.balance("did:icn:test:3").unwrap(), 330); // 300 + 30
        assert_eq!(result.balance("did:icn:test:4").unwrap(), 440); // 400 + 40
        assert_eq!(result.balance("did:icn:test:5").unwrap(), 500);
        assert_eq!(result.balance("did:icn:test:6").unwrap(), 600);
        
        // Verify total balance
        let total_a: u64 = ledger_a.accounts().values().sum();
        let total_b: u64 = ledger_b.accounts().values().sum();
        let total_result: u64 = result.accounts().values().sum();
        
        assert_eq!(total_result, total_a + total_b - 30 - 40); // Subtract overlap
    }
    
    #[test]
    fn test_shard_ledger() {
        let parent_ledger = create_test_ledger();
        let partition_map = create_test_partition_map();
        
        let (ledger_a, ledger_b) = shard_ledger(&parent_ledger, &partition_map).unwrap();
        
        // Verify accounts in ledger A
        assert!(ledger_a.has_account("did:icn:test:1"));
        assert!(ledger_a.has_account("did:icn:test:2"));
        assert!(!ledger_a.has_account("did:icn:test:3"));
        assert!(!ledger_a.has_account("did:icn:test:4"));
        
        // Verify accounts in ledger B
        assert!(!ledger_b.has_account("did:icn:test:1"));
        assert!(!ledger_b.has_account("did:icn:test:2"));
        assert!(ledger_b.has_account("did:icn:test:3"));
        assert!(ledger_b.has_account("did:icn:test:4"));
        
        // Verify balances in ledger A
        assert_eq!(ledger_a.balance("did:icn:test:1").unwrap(), 100);
        assert_eq!(ledger_a.balance("did:icn:test:2").unwrap(), 200);
        
        // Verify balances in ledger B
        assert_eq!(ledger_b.balance("did:icn:test:3").unwrap(), 300);
        assert_eq!(ledger_b.balance("did:icn:test:4").unwrap(), 400);
        
        // Verify total balance preservation
        let total_parent: u64 = parent_ledger.accounts().values().sum();
        let total_a: u64 = ledger_a.accounts().values().sum();
        let total_b: u64 = ledger_b.accounts().values().sum();
        
        assert_eq!(total_parent, total_a + total_b);
    }
} 