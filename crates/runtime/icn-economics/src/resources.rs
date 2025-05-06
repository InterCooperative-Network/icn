/*!
# ICN Economic Resources Module

This module defines the resources that can be managed by the economics system,
including Mana, computation tokens, and other economic resources.
*/

use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

/// Types of regenerative resources (Mana-like resources)
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RegenerativeResourceType {
    /// Primary Mana used for all core operations
    Mana,
    
    /// Computational resources
    ComputeUnits,
    
    /// Storage resources
    StorageUnits,
    
    /// Network bandwidth
    NetworkBandwidth,
    
    /// Custom resource type
    Custom(String),
}

impl std::fmt::Display for RegenerativeResourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegenerativeResourceType::Mana => write!(f, "mana"),
            RegenerativeResourceType::ComputeUnits => write!(f, "compute_units"),
            RegenerativeResourceType::StorageUnits => write!(f, "storage_units"),
            RegenerativeResourceType::NetworkBandwidth => write!(f, "network_bandwidth"),
            RegenerativeResourceType::Custom(name) => write!(f, "custom_{}", name),
        }
    }
}

/// Mana balance pool
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManaPool {
    /// Current amount of mana
    pub current: u64,
    
    /// Maximum capacity of mana
    pub max_capacity: u64,
    
    /// Regeneration rate per hour
    pub regen_rate_per_hour: u64,
    
    /// Last regeneration timestamp
    pub last_regen_time: SystemTime,
    
    /// Locked amounts (for pending transactions)
    pub locked: u64,
}

impl ManaPool {
    /// Create a new mana pool
    pub fn new(max_capacity: u64, regen_rate_per_hour: u64) -> Self {
        Self {
            current: max_capacity, // Start full
            max_capacity,
            regen_rate_per_hour,
            last_regen_time: SystemTime::now(),
            locked: 0,
        }
    }
    
    /// Update the regeneration
    pub fn update_regeneration(&mut self) {
        let now = SystemTime::now();
        if let Ok(elapsed) = now.duration_since(self.last_regen_time) {
            // Calculate how much to regenerate based on time passed
            let hours_passed = elapsed.as_secs() as f64 / 3600.0;
            let regen_amount = (hours_passed * self.regen_rate_per_hour as f64) as u64;
            
            if regen_amount > 0 {
                // Add regenerated amount, but don't exceed max capacity
                self.current = std::cmp::min(self.current + regen_amount, self.max_capacity);
                self.last_regen_time = now;
            }
        }
    }
    
    /// Check if there is enough mana for an operation
    pub fn has_sufficient(&self, amount: u64) -> bool {
        self.current >= amount + self.locked
    }
    
    /// Consume mana for an operation
    pub fn consume(&mut self, amount: u64) -> bool {
        self.update_regeneration();
        
        if self.has_sufficient(amount) {
            self.current -= amount;
            true
        } else {
            false
        }
    }
    
    /// Lock mana for a pending operation
    pub fn lock(&mut self, amount: u64) -> bool {
        self.update_regeneration();
        
        if self.has_sufficient(amount) {
            self.locked += amount;
            true
        } else {
            false
        }
    }
    
    /// Release locked mana (in case operation fails)
    pub fn release_lock(&mut self, amount: u64) {
        if amount > self.locked {
            self.locked = 0;
        } else {
            self.locked -= amount;
        }
    }
    
    /// Complete a locked operation (convert lock to consumption)
    pub fn complete_locked_operation(&mut self, amount: u64) {
        if amount > self.locked {
            // Error case, but handle gracefully
            self.current = self.current.saturating_sub(amount);
            self.locked = 0;
        } else {
            self.locked -= amount;
            self.current -= amount;
        }
    }
}

/// Resource pool manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceManager {
    /// The DID this resource pool belongs to
    pub owner_did: String,
    
    /// Mana pools for different resource types
    pub resource_pools: HashMap<RegenerativeResourceType, ManaPool>,
    
    /// Reputation score affecting max capacity
    pub reputation_score: Option<u64>,
}

impl ResourceManager {
    /// Create a new resource manager for a DID
    pub fn new(owner_did: String) -> Self {
        let mut resource_pools = HashMap::new();
        
        // Add default Mana pool
        resource_pools.insert(
            RegenerativeResourceType::Mana,
            ManaPool::new(1000, 100) // 1000 max, regenerates 100 per hour
        );
        
        Self {
            owner_did,
            resource_pools,
            reputation_score: None,
        }
    }
    
    /// Update regeneration for all resource pools
    pub fn update_all_pools(&mut self) {
        for pool in self.resource_pools.values_mut() {
            pool.update_regeneration();
        }
    }
    
    /// Add a new resource pool
    pub fn add_resource_pool(
        &mut self,
        resource_type: RegenerativeResourceType,
        max_capacity: u64,
        regen_rate_per_hour: u64
    ) {
        self.resource_pools.insert(
            resource_type,
            ManaPool::new(max_capacity, regen_rate_per_hour)
        );
    }
    
    /// Get a resource pool
    pub fn get_pool(&self, resource_type: &RegenerativeResourceType) -> Option<&ManaPool> {
        self.resource_pools.get(resource_type)
    }
    
    /// Get a mutable resource pool
    pub fn get_pool_mut(&mut self, resource_type: &RegenerativeResourceType) -> Option<&mut ManaPool> {
        self.resource_pools.get_mut(resource_type)
    }
    
    /// Consume resources of a specific type
    pub fn consume_resource(
        &mut self,
        resource_type: &RegenerativeResourceType,
        amount: u64
    ) -> bool {
        if let Some(pool) = self.get_pool_mut(resource_type) {
            pool.consume(amount)
        } else {
            false
        }
    }
    
    /// Apply reputation effects to regeneration rates and capacities
    pub fn apply_reputation_effects(&mut self, reputation_score: u64) {
        self.reputation_score = Some(reputation_score);
        
        // Apply reputation effects - higher reputation means higher capacity and regen rate
        let reputation_modifier = 1.0 + (reputation_score as f64 / 1000.0);
        
        for pool in self.resource_pools.values_mut() {
            // Adjust max capacity based on reputation
            let new_max = (pool.max_capacity as f64 * reputation_modifier) as u64;
            
            // If max capacity is increased, also increase current by the same proportion
            if new_max > pool.max_capacity {
                let ratio = new_max as f64 / pool.max_capacity as f64;
                pool.current = (pool.current as f64 * ratio) as u64;
            }
            
            pool.max_capacity = new_max;
            
            // Adjust regeneration rate based on reputation
            pool.regen_rate_per_hour = (pool.regen_rate_per_hour as f64 * reputation_modifier) as u64;
        }
    }
}

/// Resource costs for various operations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationCosts {
    /// Costs by operation type
    pub costs: HashMap<String, HashMap<RegenerativeResourceType, u64>>,
}

impl OperationCosts {
    /// Create a new set of operation costs
    pub fn new() -> Self {
        let mut costs = HashMap::new();
        
        // Add default cost for basic operations
        let mut transaction_costs = HashMap::new();
        transaction_costs.insert(RegenerativeResourceType::Mana, 10);
        costs.insert("transaction.basic".to_string(), transaction_costs);
        
        // Cost for contract execution
        let mut contract_costs = HashMap::new();
        contract_costs.insert(RegenerativeResourceType::Mana, 50);
        contract_costs.insert(RegenerativeResourceType::ComputeUnits, 100);
        costs.insert("contract.execute".to_string(), contract_costs);
        
        // Cost for storage operations
        let mut storage_costs = HashMap::new();
        storage_costs.insert(RegenerativeResourceType::Mana, 20);
        storage_costs.insert(RegenerativeResourceType::StorageUnits, 1);
        costs.insert("storage.write".to_string(), storage_costs);
        
        Self { costs }
    }
    
    /// Get the cost of an operation
    pub fn get_operation_cost(
        &self,
        operation_type: &str
    ) -> Option<&HashMap<RegenerativeResourceType, u64>> {
        self.costs.get(operation_type)
    }
    
    /// Set the cost for an operation
    pub fn set_operation_cost(
        &mut self,
        operation_type: &str,
        costs: HashMap<RegenerativeResourceType, u64>
    ) {
        self.costs.insert(operation_type.to_string(), costs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    
    #[test]
    fn test_mana_regeneration() {
        let mut pool = ManaPool::new(100, 10); // 100 max, 10 per hour
        
        // Consume some mana
        assert!(pool.consume(50));
        assert_eq!(pool.current, 50);
        
        // Manually adjust the last regen time to simulate passage of time
        pool.last_regen_time = SystemTime::now()
            .checked_sub(Duration::from_secs(360)) // 6 minutes = 0.1 hours
            .unwrap();
        
        // Update regeneration
        pool.update_regeneration();
        
        // Should have regenerated 0.1 hours * 10 per hour = 1 mana
        assert_eq!(pool.current, 51);
    }
    
    #[test]
    fn test_resource_manager() {
        let mut manager = ResourceManager::new("did:icn:test".to_string());
        
        // Add compute units resource
        manager.add_resource_pool(
            RegenerativeResourceType::ComputeUnits,
            500,
            50
        );
        
        // Consume mana
        assert!(manager.consume_resource(&RegenerativeResourceType::Mana, 200));
        
        // Check updated balance
        let mana_pool = manager.get_pool(&RegenerativeResourceType::Mana).unwrap();
        assert_eq!(mana_pool.current, 800);
        
        // Apply reputation effects
        manager.apply_reputation_effects(500); // 500 reputation score
        
        // Check updated capacities
        let mana_pool = manager.get_pool(&RegenerativeResourceType::Mana).unwrap();
        assert!(mana_pool.max_capacity > 1000); // Should have increased
        assert!(mana_pool.regen_rate_per_hour > 100); // Should have increased
    }
    
    #[test]
    fn test_operation_costs() {
        let costs = OperationCosts::new();
        
        // Check basic transaction cost
        let tx_cost = costs.get_operation_cost("transaction.basic").unwrap();
        assert_eq!(*tx_cost.get(&RegenerativeResourceType::Mana).unwrap(), 10);
        
        // Check contract execution cost
        let contract_cost = costs.get_operation_cost("contract.execute").unwrap();
        assert_eq!(*contract_cost.get(&RegenerativeResourceType::Mana).unwrap(), 50);
        assert_eq!(*contract_cost.get(&RegenerativeResourceType::ComputeUnits).unwrap(), 100);
    }
} 