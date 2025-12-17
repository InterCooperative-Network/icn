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

    pub fn create_pool(&self, community: &mut Community, name: String, resource_type: String, capacity: u64, unit: String) -> Result<()> {
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
        let pool = community.resource_pools.get_mut(pool_name)
            .ok_or_else(|| CommunityError::NotFound(format!("Resource pool: {}", pool_name)))?;
        
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

    pub fn deallocate(&self, community: &mut Community, pool_name: &str, amount: u64) -> Result<()> {
        let pool = community.resource_pools.get_mut(pool_name)
            .ok_or_else(|| CommunityError::NotFound(format!("Resource pool: {}", pool_name)))?;
        
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
