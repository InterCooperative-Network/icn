use crate::error::{CooperativeError, Result};
use crate::types::{Cooperative, CooperativeId, CooperativeStatus, CooperativeType};
use icn_store::Store;
use std::sync::Arc;

const COOP_PREFIX: &str = "cooperative:";
const INDEX_BY_STATUS: &str = "coop_index:status:";
const INDEX_BY_TYPE: &str = "coop_index:type:";
const INDEX_BY_MEMBER: &str = "coop_index:member:";

/// Query parameters for cooperative search
#[derive(Debug, Clone, Default)]
pub struct CooperativeQuery {
    pub status: Option<CooperativeStatus>,
    pub coop_type: Option<CooperativeType>,
    pub member_did: Option<String>,
}

/// Storage manager for cooperatives
pub struct CooperativeStore {
    store: Arc<dyn Store>,
}

impl CooperativeStore {
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self { store }
    }

    /// Store a cooperative
    pub fn put(&self, coop: &Cooperative) -> Result<()> {
        let key = format!("{}{}", COOP_PREFIX, coop.id);
        let value = serde_json::to_vec(coop)
            .map_err(|e| CooperativeError::Serialization(e.to_string()))?;

        self.store
            .put(key.as_bytes(), &value)
            .map_err(|e| CooperativeError::Storage(e.to_string()))?;

        // Update indices
        self.index_by_status(coop)?;
        self.index_by_type(coop)?;
        self.index_by_members(coop)?;

        Ok(())
    }

    /// Retrieve a cooperative by ID
    pub fn get(&self, id: &CooperativeId) -> Result<Option<Cooperative>> {
        let key = format!("{}{}", COOP_PREFIX, id);
        
        match self.store.get(key.as_bytes()) {
            Ok(Some(value)) => {
                let coop: Cooperative = serde_json::from_slice(&value)
                    .map_err(|e| CooperativeError::Serialization(e.to_string()))?;
                Ok(Some(coop))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(CooperativeError::Storage(e.to_string())),
        }
    }

    /// Delete a cooperative
    pub fn delete(&self, id: &CooperativeId) -> Result<()> {
        // Get first to update indices
        if let Some(coop) = self.get(id)? {
            self.remove_from_indices(&coop)?;
        }

        let key = format!("{}{}", COOP_PREFIX, id);
        self.store
            .delete(key.as_bytes())
            .map_err(|e| CooperativeError::Storage(e.to_string()))?;

        Ok(())
    }

    /// Query cooperatives
    pub fn query(&self, query: &CooperativeQuery) -> Result<Vec<Cooperative>> {
        let mut results = Vec::new();

        // If we have specific query params, use indices
        if let Some(status) = query.status {
            let index_key = format!("{}{}:{:?}", INDEX_BY_STATUS, status as u8, status);
            if let Ok(Some(index_data)) = self.store.get(index_key.as_bytes()) {
                let ids: Vec<CooperativeId> = serde_json::from_slice(&index_data)
                    .map_err(|e| CooperativeError::Serialization(e.to_string()))?;
                
                for id in ids {
                    if let Some(coop) = self.get(&id)? {
                        if self.matches_query(&coop, query) {
                            results.push(coop);
                        }
                    }
                }
            }
        } else if let Some(member_did) = &query.member_did {
            let index_key = format!("{}{}", INDEX_BY_MEMBER, member_did);
            if let Ok(Some(index_data)) = self.store.get(index_key.as_bytes()) {
                let ids: Vec<CooperativeId> = serde_json::from_slice(&index_data)
                    .map_err(|e| CooperativeError::Serialization(e.to_string()))?;
                
                for id in ids {
                    if let Some(coop) = self.get(&id)? {
                        if self.matches_query(&coop, query) {
                            results.push(coop);
                        }
                    }
                }
            }
        } else {
            // Scan all cooperatives
            let prefix = COOP_PREFIX.as_bytes();
            let items = self.store.scan(prefix)
                .map_err(|e| CooperativeError::Storage(e.to_string()))?;
            
            for (_, value) in items {
                let coop: Cooperative = serde_json::from_slice(&value)
                    .map_err(|e| CooperativeError::Serialization(e.to_string()))?;
                
                if self.matches_query(&coop, query) {
                    results.push(coop);
                }
            }
        }

        Ok(results)
    }

    fn matches_query(&self, coop: &Cooperative, query: &CooperativeQuery) -> bool {
        if let Some(status) = query.status {
            if coop.status != status {
                return false;
            }
        }

        if let Some(coop_type) = query.coop_type {
            if coop.coop_type != coop_type {
                return false;
            }
        }

        if let Some(member_did) = &query.member_did {
            if !coop.members.contains_key(member_did) {
                return false;
            }
        }

        true
    }

    fn index_by_status(&self, coop: &Cooperative) -> Result<()> {
        let index_key = format!("{}{}:{:?}", INDEX_BY_STATUS, coop.status as u8, coop.status);
        self.add_to_index(&index_key, &coop.id)
    }

    fn index_by_type(&self, coop: &Cooperative) -> Result<()> {
        let index_key = format!("{}{}:{:?}", INDEX_BY_TYPE, coop.coop_type as u8, coop.coop_type);
        self.add_to_index(&index_key, &coop.id)
    }

    fn index_by_members(&self, coop: &Cooperative) -> Result<()> {
        for did in coop.members.keys() {
            let index_key = format!("{}{}", INDEX_BY_MEMBER, did);
            self.add_to_index(&index_key, &coop.id)?;
        }
        Ok(())
    }

    fn add_to_index(&self, index_key: &str, coop_id: &CooperativeId) -> Result<()> {
        let mut ids: Vec<CooperativeId> = match self.store.get(index_key.as_bytes()) {
            Ok(Some(data)) => serde_json::from_slice(&data)
                .map_err(|e| CooperativeError::Serialization(e.to_string()))?,
            _ => Vec::new(),
        };

        if !ids.contains(coop_id) {
            ids.push(coop_id.clone());
            let value = serde_json::to_vec(&ids)
                .map_err(|e| CooperativeError::Serialization(e.to_string()))?;
            self.store
                .put(index_key.as_bytes(), &value)
                .map_err(|e| CooperativeError::Storage(e.to_string()))?;
        }

        Ok(())
    }

    fn remove_from_indices(&self, coop: &Cooperative) -> Result<()> {
        // Remove from status index
        let status_key = format!("{}{}:{:?}", INDEX_BY_STATUS, coop.status as u8, coop.status);
        self.remove_from_index(&status_key, &coop.id)?;

        // Remove from type index
        let type_key = format!("{}{}:{:?}", INDEX_BY_TYPE, coop.coop_type as u8, coop.coop_type);
        self.remove_from_index(&type_key, &coop.id)?;

        // Remove from member indices
        for did in coop.members.keys() {
            let member_key = format!("{}{}", INDEX_BY_MEMBER, did);
            self.remove_from_index(&member_key, &coop.id)?;
        }

        Ok(())
    }

    fn remove_from_index(&self, index_key: &str, coop_id: &CooperativeId) -> Result<()> {
        if let Ok(Some(data)) = self.store.get(index_key.as_bytes()) {
            let mut ids: Vec<CooperativeId> = serde_json::from_slice(&data)
                .map_err(|e| CooperativeError::Serialization(e.to_string()))?;
            
            ids.retain(|id| id != coop_id);
            
            let value = serde_json::to_vec(&ids)
                .map_err(|e| CooperativeError::Serialization(e.to_string()))?;
            self.store
                .put(index_key.as_bytes(), &value)
                .map_err(|e| CooperativeError::Storage(e.to_string()))?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CooperativeType;
    use icn_store::SledStore;

    #[test]
    fn test_store_and_retrieve() {
        let store: Arc<dyn Store> = Arc::new(SledStore::temporary().unwrap());
        let coop_store = CooperativeStore::new(store);

        let coop = Cooperative::new(
            "test-id".to_string(),
            "Test Coop".to_string(),
            CooperativeType::Worker,
            "domain".to_string(),
            3,
        );

        coop_store.put(&coop).unwrap();
        let retrieved = coop_store.get(&"test-id".to_string()).unwrap();
        
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Test Coop");
    }

    #[test]
    fn test_query_by_status() {
        let store: Arc<dyn Store> = Arc::new(SledStore::temporary().unwrap());
        let coop_store = CooperativeStore::new(store);

        let coop = Cooperative::new(
            "test-id".to_string(),
            "Test Coop".to_string(),
            CooperativeType::Worker,
            "domain".to_string(),
            3,
        );

        coop_store.put(&coop).unwrap();

        let query = CooperativeQuery {
            status: Some(CooperativeStatus::Forming),
            ..Default::default()
        };

        let results = coop_store.query(&query).unwrap();
        assert_eq!(results.len(), 1);
    }
}
