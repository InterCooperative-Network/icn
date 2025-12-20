use crate::error::{CommunityError, Result};
use crate::types::{Community, CommunityId, CommunityStatus};
use icn_store::Store;
use std::sync::Arc;

const COMMUNITY_PREFIX: &str = "community:";

#[derive(Debug, Clone, Default)]
pub struct CommunityQuery {
    pub status: Option<CommunityStatus>,
    pub member_id: Option<String>,
}

pub struct CommunityStore {
    store: Arc<dyn Store>,
}

impl CommunityStore {
    pub fn new(store: Arc<dyn Store>) -> Self {
        Self { store }
    }

    pub fn put(&self, community: &Community) -> Result<()> {
        let key = format!("{}{}", COMMUNITY_PREFIX, community.id);
        let value = serde_json::to_vec(community)
            .map_err(|e| CommunityError::Serialization(e.to_string()))?;
        self.store
            .put(key.as_bytes(), &value)
            .map_err(|e| CommunityError::Storage(e.to_string()))?;
        Ok(())
    }

    pub fn get(&self, id: &CommunityId) -> Result<Option<Community>> {
        let key = format!("{COMMUNITY_PREFIX}{id}");
        match self.store.get(key.as_bytes()) {
            Ok(Some(value)) => {
                let community = serde_json::from_slice(&value)
                    .map_err(|e| CommunityError::Serialization(e.to_string()))?;
                Ok(Some(community))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(CommunityError::Storage(e.to_string())),
        }
    }

    pub fn delete(&self, id: &CommunityId) -> Result<()> {
        let key = format!("{COMMUNITY_PREFIX}{id}");
        self.store
            .delete(key.as_bytes())
            .map_err(|e| CommunityError::Storage(e.to_string()))?;
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<Community>> {
        let prefix = COMMUNITY_PREFIX.as_bytes();
        let items = self
            .store
            .scan(prefix)
            .map_err(|e| CommunityError::Storage(e.to_string()))?;

        items
            .into_iter()
            .map(|(_, value)| {
                serde_json::from_slice(&value)
                    .map_err(|e| CommunityError::Serialization(e.to_string()))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::CommunityType;
    use icn_store::SledStore;

    #[test]
    fn test_store_and_retrieve() {
        let store: Arc<dyn Store> = Arc::new(SledStore::temporary().unwrap());
        let comm_store = CommunityStore::new(store);

        let community = Community::new(
            "test-comm".to_string(),
            "Test Community".to_string(),
            CommunityType::Geographic,
            "test-domain".to_string(),
        );

        comm_store.put(&community).unwrap();
        let retrieved = comm_store.get(&"test-comm".to_string()).unwrap();

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Test Community");
    }
}
