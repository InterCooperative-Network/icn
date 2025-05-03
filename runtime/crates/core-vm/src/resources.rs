/*! 
# Resource Tracking for Core VM

This module handles resource tracking and authorization for the Core VM.
*/

use std::fmt;

/// Type of resource that can be authorized and consumed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResourceType {
    /// Computational resources (CPU, memory)
    Compute,
    /// Storage resources (disk space)
    Storage,
    /// Network resources (bandwidth)
    Network,
    /// Token operations
    Token,
}

impl fmt::Display for ResourceType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ResourceType::Compute => write!(f, "compute"),
            ResourceType::Storage => write!(f, "storage"),
            ResourceType::Network => write!(f, "network"),
            ResourceType::Token => write!(f, "token"),
        }
    }
}

/// Authorization for a specific resource type
#[derive(Debug, Clone)]
pub struct ResourceAuthorization {
    /// Type of resource
    pub resource_type: ResourceType,
    /// Maximum amount of resource that can be consumed
    pub limit: u64,
    /// Optional context for the authorization (e.g., specific storage key pattern)
    pub context: Option<String>,
    /// Description of what the authorization is for
    pub description: String,
}

impl ResourceAuthorization {
    /// Create a new resource authorization
    pub fn new(
        resource_type: ResourceType,
        limit: u64,
        context: Option<String>,
        description: String,
    ) -> Self {
        Self {
            resource_type,
            limit,
            context,
            description,
        }
    }

    /// Check if this authorization allows the given amount of resource to be consumed
    pub fn allows(&self, current: u64, additional: u64) -> bool {
        match current.checked_add(additional) {
            Some(total) => total <= self.limit,
            None => false, // Overflow would exceed limit
        }
    }
}

/// Resources consumed during execution
#[derive(Debug, Clone, Default)]
pub struct ResourceConsumption {
    /// Compute resources consumed
    pub compute: u64,
    /// Storage resources consumed
    pub storage: u64,
    /// Network resources consumed
    pub network: u64,
    /// Token operations performed
    pub token: u64,
}

impl ResourceConsumption {
    /// Create a new empty resource consumption tracker
    pub fn new() -> Self {
        Self::default()
    }

    /// Get consumption for a specific resource type
    pub fn get(&self, resource_type: ResourceType) -> u64 {
        match resource_type {
            ResourceType::Compute => self.compute,
            ResourceType::Storage => self.storage,
            ResourceType::Network => self.network,
            ResourceType::Token => self.token,
        }
    }

    /// Add consumption for a specific resource type
    pub fn add(&mut self, resource_type: ResourceType, amount: u64) -> Result<(), String> {
        match resource_type {
            ResourceType::Compute => {
                self.compute = self.compute.checked_add(amount).ok_or_else(|| 
                    format!("Compute resource consumption overflow")
                )?;
            },
            ResourceType::Storage => {
                self.storage = self.storage.checked_add(amount).ok_or_else(|| 
                    format!("Storage resource consumption overflow")
                )?;
            },
            ResourceType::Network => {
                self.network = self.network.checked_add(amount).ok_or_else(|| 
                    format!("Network resource consumption overflow")
                )?;
            },
            ResourceType::Token => {
                self.token = self.token.checked_add(amount).ok_or_else(|| 
                    format!("Token resource consumption overflow")
                )?;
            },
        }
        Ok(())
    }
} 