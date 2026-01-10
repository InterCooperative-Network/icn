use crate::error::{CommunityError, Result};
use crate::types::{Community, ResourcePool};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceAllocation {
    pub pool_name: String,
    pub member_id: String,
    pub amount: u64,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub struct ResourceManager;

impl ResourceManager {
    pub fn new() -> Self {
        Self
    }

    pub fn create_pool(
        &self,
        community: &mut Community,
        name: String,
        resource_type: String,
        capacity: u64,
        unit: String,
    ) -> Result<()> {
        let pool = ResourcePool {
            name: name.clone(),
            resource_type,
            total_capacity: capacity,
            allocated: 0,
            unit,
        };
        community.resource_pools.insert(name, pool);
        community.updated_at = chrono::Utc::now();
        Ok(())
    }

    pub fn allocate(&self, community: &mut Community, pool_name: &str, amount: u64) -> Result<()> {
        let pool = community
            .resource_pools
            .get_mut(pool_name)
            .ok_or_else(|| CommunityError::NotFound(format!("Resource pool: {pool_name}")))?;

        if !pool.can_allocate(amount) {
            return Err(CommunityError::InsufficientResources {
                required: amount,
                available: pool.available(),
            });
        }

        pool.allocated += amount;
        community.updated_at = chrono::Utc::now();
        Ok(())
    }

    pub fn deallocate(
        &self,
        community: &mut Community,
        pool_name: &str,
        amount: u64,
    ) -> Result<()> {
        let pool = community
            .resource_pools
            .get_mut(pool_name)
            .ok_or_else(|| CommunityError::NotFound(format!("Resource pool: {pool_name}")))?;

        pool.allocated = pool.allocated.saturating_sub(amount);
        community.updated_at = chrono::Utc::now();
        Ok(())
    }
}

impl Default for ResourceManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{CommunityStatus, CommunityType};
    use chrono::Utc;

    fn create_test_community() -> Community {
        Community::new(
            "test-comm".to_string(),
            "Test Community".to_string(),
            CommunityType::Ecosystem,
            "test-domain".to_string(),
        )
    }

    // === ResourceManager creation tests ===

    #[test]
    fn test_resource_manager_new() {
        let _manager = ResourceManager::new();
        // ResourceManager is a unit struct, just verify it constructs
    }

    #[test]
    fn test_resource_manager_default() {
        let _manager: ResourceManager = Default::default();
        // Verify Default trait implementation works
    }

    // === ResourceAllocation tests ===

    #[test]
    fn test_resource_allocation_creation() {
        let allocation = ResourceAllocation {
            pool_name: "compute".to_string(),
            member_id: "member-1".to_string(),
            amount: 50,
            expires_at: None,
        };

        assert_eq!(allocation.pool_name, "compute");
        assert_eq!(allocation.member_id, "member-1");
        assert_eq!(allocation.amount, 50);
        assert!(allocation.expires_at.is_none());
    }

    #[test]
    fn test_resource_allocation_with_expiration() {
        let expiry = Utc::now() + chrono::Duration::hours(24);
        let allocation = ResourceAllocation {
            pool_name: "storage".to_string(),
            member_id: "member-2".to_string(),
            amount: 100,
            expires_at: Some(expiry),
        };

        assert!(allocation.expires_at.is_some());
        assert_eq!(allocation.expires_at.unwrap(), expiry);
    }

    #[test]
    fn test_resource_allocation_serialization() {
        let allocation = ResourceAllocation {
            pool_name: "credits".to_string(),
            member_id: "did:icn:abc123".to_string(),
            amount: 1000,
            expires_at: None,
        };

        let serialized = serde_json::to_string(&allocation).unwrap();
        let deserialized: ResourceAllocation = serde_json::from_str(&serialized).unwrap();

        assert_eq!(deserialized.pool_name, allocation.pool_name);
        assert_eq!(deserialized.member_id, allocation.member_id);
        assert_eq!(deserialized.amount, allocation.amount);
    }

    // === create_pool tests ===

    #[test]
    fn test_create_pool_basic() {
        let manager = ResourceManager::new();
        let mut community = create_test_community();
        let original_updated_at = community.updated_at;

        // Small delay to ensure timestamp changes
        std::thread::sleep(std::time::Duration::from_millis(10));

        manager
            .create_pool(
                &mut community,
                "compute".to_string(),
                "cpu".to_string(),
                100,
                "cores".to_string(),
            )
            .unwrap();

        assert!(community.resource_pools.contains_key("compute"));
        let pool = community.resource_pools.get("compute").unwrap();
        assert_eq!(pool.name, "compute");
        assert_eq!(pool.resource_type, "cpu");
        assert_eq!(pool.total_capacity, 100);
        assert_eq!(pool.allocated, 0);
        assert_eq!(pool.unit, "cores");
        assert!(community.updated_at > original_updated_at);
    }

    #[test]
    fn test_create_multiple_pools() {
        let manager = ResourceManager::new();
        let mut community = create_test_community();

        manager
            .create_pool(
                &mut community,
                "compute".to_string(),
                "cpu".to_string(),
                100,
                "cores".to_string(),
            )
            .unwrap();

        manager
            .create_pool(
                &mut community,
                "storage".to_string(),
                "disk".to_string(),
                1000,
                "GB".to_string(),
            )
            .unwrap();

        manager
            .create_pool(
                &mut community,
                "credits".to_string(),
                "mutual_credit".to_string(),
                5000,
                "credits".to_string(),
            )
            .unwrap();

        assert_eq!(community.resource_pools.len(), 3);
        assert!(community.resource_pools.contains_key("compute"));
        assert!(community.resource_pools.contains_key("storage"));
        assert!(community.resource_pools.contains_key("credits"));
    }

    #[test]
    fn test_create_pool_overwrites_existing() {
        let manager = ResourceManager::new();
        let mut community = create_test_community();

        manager
            .create_pool(
                &mut community,
                "compute".to_string(),
                "cpu".to_string(),
                100,
                "cores".to_string(),
            )
            .unwrap();

        // Overwrite with new capacity
        manager
            .create_pool(
                &mut community,
                "compute".to_string(),
                "gpu".to_string(),
                50,
                "units".to_string(),
            )
            .unwrap();

        assert_eq!(community.resource_pools.len(), 1);
        let pool = community.resource_pools.get("compute").unwrap();
        assert_eq!(pool.resource_type, "gpu");
        assert_eq!(pool.total_capacity, 50);
    }

    #[test]
    fn test_create_pool_zero_capacity() {
        let manager = ResourceManager::new();
        let mut community = create_test_community();

        manager
            .create_pool(
                &mut community,
                "empty".to_string(),
                "nothing".to_string(),
                0,
                "units".to_string(),
            )
            .unwrap();

        let pool = community.resource_pools.get("empty").unwrap();
        assert_eq!(pool.total_capacity, 0);
        assert_eq!(pool.available(), 0);
    }

    // === allocate tests ===

    #[test]
    fn test_allocate_basic() {
        let manager = ResourceManager::new();
        let mut community = create_test_community();

        manager
            .create_pool(
                &mut community,
                "compute".to_string(),
                "cpu".to_string(),
                100,
                "cores".to_string(),
            )
            .unwrap();

        let original_updated_at = community.updated_at;
        std::thread::sleep(std::time::Duration::from_millis(10));

        manager.allocate(&mut community, "compute", 30).unwrap();

        let pool = community.resource_pools.get("compute").unwrap();
        assert_eq!(pool.allocated, 30);
        assert_eq!(pool.available(), 70);
        assert!(community.updated_at > original_updated_at);
    }

    #[test]
    fn test_allocate_multiple_times() {
        let manager = ResourceManager::new();
        let mut community = create_test_community();

        manager
            .create_pool(
                &mut community,
                "compute".to_string(),
                "cpu".to_string(),
                100,
                "cores".to_string(),
            )
            .unwrap();

        manager.allocate(&mut community, "compute", 20).unwrap();
        manager.allocate(&mut community, "compute", 30).unwrap();
        manager.allocate(&mut community, "compute", 25).unwrap();

        let pool = community.resource_pools.get("compute").unwrap();
        assert_eq!(pool.allocated, 75);
        assert_eq!(pool.available(), 25);
    }

    #[test]
    fn test_allocate_exact_capacity() {
        let manager = ResourceManager::new();
        let mut community = create_test_community();

        manager
            .create_pool(
                &mut community,
                "compute".to_string(),
                "cpu".to_string(),
                100,
                "cores".to_string(),
            )
            .unwrap();

        manager.allocate(&mut community, "compute", 100).unwrap();

        let pool = community.resource_pools.get("compute").unwrap();
        assert_eq!(pool.allocated, 100);
        assert_eq!(pool.available(), 0);
    }

    #[test]
    fn test_allocate_zero_amount() {
        let manager = ResourceManager::new();
        let mut community = create_test_community();

        manager
            .create_pool(
                &mut community,
                "compute".to_string(),
                "cpu".to_string(),
                100,
                "cores".to_string(),
            )
            .unwrap();

        manager.allocate(&mut community, "compute", 0).unwrap();

        let pool = community.resource_pools.get("compute").unwrap();
        assert_eq!(pool.allocated, 0);
    }

    #[test]
    fn test_allocate_insufficient_resources() {
        let manager = ResourceManager::new();
        let mut community = create_test_community();

        manager
            .create_pool(
                &mut community,
                "compute".to_string(),
                "cpu".to_string(),
                100,
                "cores".to_string(),
            )
            .unwrap();

        let result = manager.allocate(&mut community, "compute", 150);

        assert!(result.is_err());
        match result.unwrap_err() {
            CommunityError::InsufficientResources {
                required,
                available,
            } => {
                assert_eq!(required, 150);
                assert_eq!(available, 100);
            }
            _ => panic!("Expected InsufficientResources error"),
        }
    }

    #[test]
    fn test_allocate_after_partial_use() {
        let manager = ResourceManager::new();
        let mut community = create_test_community();

        manager
            .create_pool(
                &mut community,
                "compute".to_string(),
                "cpu".to_string(),
                100,
                "cores".to_string(),
            )
            .unwrap();

        manager.allocate(&mut community, "compute", 60).unwrap();

        // Try to allocate more than remaining
        let result = manager.allocate(&mut community, "compute", 50);

        assert!(result.is_err());
        match result.unwrap_err() {
            CommunityError::InsufficientResources {
                required,
                available,
            } => {
                assert_eq!(required, 50);
                assert_eq!(available, 40);
            }
            _ => panic!("Expected InsufficientResources error"),
        }
    }

    #[test]
    fn test_allocate_nonexistent_pool() {
        let manager = ResourceManager::new();
        let mut community = create_test_community();

        let result = manager.allocate(&mut community, "nonexistent", 10);

        assert!(result.is_err());
        match result.unwrap_err() {
            CommunityError::NotFound(msg) => {
                assert!(msg.contains("nonexistent"));
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    // === deallocate tests ===

    #[test]
    fn test_deallocate_basic() {
        let manager = ResourceManager::new();
        let mut community = create_test_community();

        manager
            .create_pool(
                &mut community,
                "compute".to_string(),
                "cpu".to_string(),
                100,
                "cores".to_string(),
            )
            .unwrap();

        manager.allocate(&mut community, "compute", 50).unwrap();

        let original_updated_at = community.updated_at;
        std::thread::sleep(std::time::Duration::from_millis(10));

        manager.deallocate(&mut community, "compute", 20).unwrap();

        let pool = community.resource_pools.get("compute").unwrap();
        assert_eq!(pool.allocated, 30);
        assert_eq!(pool.available(), 70);
        assert!(community.updated_at > original_updated_at);
    }

    #[test]
    fn test_deallocate_all() {
        let manager = ResourceManager::new();
        let mut community = create_test_community();

        manager
            .create_pool(
                &mut community,
                "compute".to_string(),
                "cpu".to_string(),
                100,
                "cores".to_string(),
            )
            .unwrap();

        manager.allocate(&mut community, "compute", 75).unwrap();
        manager.deallocate(&mut community, "compute", 75).unwrap();

        let pool = community.resource_pools.get("compute").unwrap();
        assert_eq!(pool.allocated, 0);
        assert_eq!(pool.available(), 100);
    }

    #[test]
    fn test_deallocate_more_than_allocated_saturates() {
        let manager = ResourceManager::new();
        let mut community = create_test_community();

        manager
            .create_pool(
                &mut community,
                "compute".to_string(),
                "cpu".to_string(),
                100,
                "cores".to_string(),
            )
            .unwrap();

        manager.allocate(&mut community, "compute", 30).unwrap();

        // Deallocate more than allocated - should saturate at 0
        manager.deallocate(&mut community, "compute", 100).unwrap();

        let pool = community.resource_pools.get("compute").unwrap();
        assert_eq!(pool.allocated, 0);
    }

    #[test]
    fn test_deallocate_zero_amount() {
        let manager = ResourceManager::new();
        let mut community = create_test_community();

        manager
            .create_pool(
                &mut community,
                "compute".to_string(),
                "cpu".to_string(),
                100,
                "cores".to_string(),
            )
            .unwrap();

        manager.allocate(&mut community, "compute", 50).unwrap();
        manager.deallocate(&mut community, "compute", 0).unwrap();

        let pool = community.resource_pools.get("compute").unwrap();
        assert_eq!(pool.allocated, 50);
    }

    #[test]
    fn test_deallocate_nonexistent_pool() {
        let manager = ResourceManager::new();
        let mut community = create_test_community();

        let result = manager.deallocate(&mut community, "nonexistent", 10);

        assert!(result.is_err());
        match result.unwrap_err() {
            CommunityError::NotFound(msg) => {
                assert!(msg.contains("nonexistent"));
            }
            _ => panic!("Expected NotFound error"),
        }
    }

    #[test]
    fn test_deallocate_from_empty_pool() {
        let manager = ResourceManager::new();
        let mut community = create_test_community();

        manager
            .create_pool(
                &mut community,
                "compute".to_string(),
                "cpu".to_string(),
                100,
                "cores".to_string(),
            )
            .unwrap();

        // Pool is empty (allocated = 0), deallocating should still work
        manager.deallocate(&mut community, "compute", 50).unwrap();

        let pool = community.resource_pools.get("compute").unwrap();
        assert_eq!(pool.allocated, 0);
    }

    // === Combined allocate/deallocate workflow tests ===

    #[test]
    fn test_allocate_deallocate_cycle() {
        let manager = ResourceManager::new();
        let mut community = create_test_community();

        manager
            .create_pool(
                &mut community,
                "compute".to_string(),
                "cpu".to_string(),
                100,
                "cores".to_string(),
            )
            .unwrap();

        // Allocate -> deallocate -> allocate again
        manager.allocate(&mut community, "compute", 80).unwrap();
        assert_eq!(
            community.resource_pools.get("compute").unwrap().available(),
            20
        );

        manager.deallocate(&mut community, "compute", 50).unwrap();
        assert_eq!(
            community.resource_pools.get("compute").unwrap().available(),
            70
        );

        manager.allocate(&mut community, "compute", 40).unwrap();
        assert_eq!(
            community.resource_pools.get("compute").unwrap().allocated,
            70
        );
    }

    #[test]
    fn test_multiple_pools_independent() {
        let manager = ResourceManager::new();
        let mut community = create_test_community();

        manager
            .create_pool(
                &mut community,
                "compute".to_string(),
                "cpu".to_string(),
                100,
                "cores".to_string(),
            )
            .unwrap();

        manager
            .create_pool(
                &mut community,
                "storage".to_string(),
                "disk".to_string(),
                1000,
                "GB".to_string(),
            )
            .unwrap();

        // Allocate from compute only
        manager.allocate(&mut community, "compute", 50).unwrap();

        // Storage should be unaffected
        assert_eq!(
            community.resource_pools.get("compute").unwrap().allocated,
            50
        );
        assert_eq!(
            community.resource_pools.get("storage").unwrap().allocated,
            0
        );

        // Allocate from storage
        manager.allocate(&mut community, "storage", 300).unwrap();

        // Compute should be unaffected
        assert_eq!(
            community.resource_pools.get("compute").unwrap().allocated,
            50
        );
        assert_eq!(
            community.resource_pools.get("storage").unwrap().allocated,
            300
        );
    }

    // === Community status tests ===

    #[test]
    fn test_operations_on_different_community_statuses() {
        let manager = ResourceManager::new();

        for status in [
            CommunityStatus::Forming,
            CommunityStatus::Active,
            CommunityStatus::Suspended,
            CommunityStatus::Dissolved,
        ] {
            let mut community = create_test_community();
            community.status = status;

            // All operations should work regardless of status
            // (business logic for status restrictions would be in higher layers)
            manager
                .create_pool(
                    &mut community,
                    "compute".to_string(),
                    "cpu".to_string(),
                    100,
                    "cores".to_string(),
                )
                .unwrap();

            manager.allocate(&mut community, "compute", 50).unwrap();
            manager.deallocate(&mut community, "compute", 25).unwrap();

            assert_eq!(
                community.resource_pools.get("compute").unwrap().allocated,
                25
            );
        }
    }
}
