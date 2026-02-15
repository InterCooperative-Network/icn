use super::error::{CommunityError, Result};
use super::types::{Community, CommunityId, CommunityStatus};
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
    use crate::community_core::types::{CommunityType, Member, MemberType};
    use chrono::Utc;
    use icn_store::SledStore;

    fn create_test_store() -> CommunityStore {
        let store: Arc<dyn Store> = Arc::new(SledStore::temporary().unwrap());
        CommunityStore::new(store)
    }

    fn create_test_community(id: &str, name: &str, ctype: CommunityType) -> Community {
        Community::new(
            id.to_string(),
            name.to_string(),
            ctype,
            format!("domain-{id}"),
        )
    }

    // === Basic put/get tests ===

    #[test]
    fn test_store_and_retrieve() {
        let comm_store = create_test_store();

        let community =
            create_test_community("test-comm", "Test Community", CommunityType::Geographic);

        comm_store.put(&community).unwrap();
        let retrieved = comm_store.get(&"test-comm".to_string()).unwrap();

        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().name, "Test Community");
    }

    #[test]
    fn test_get_non_existent_returns_none() {
        let comm_store = create_test_store();

        let result = comm_store.get(&"non-existent".to_string()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_put_overwrites_existing() {
        let comm_store = create_test_store();

        let community1 =
            create_test_community("comm-1", "Original Name", CommunityType::Geographic);
        comm_store.put(&community1).unwrap();

        let mut community2 =
            create_test_community("comm-1", "Updated Name", CommunityType::Interest);
        community2.status = CommunityStatus::Active;

        comm_store.put(&community2).unwrap();

        let retrieved = comm_store.get(&"comm-1".to_string()).unwrap().unwrap();
        assert_eq!(retrieved.name, "Updated Name");
        assert_eq!(retrieved.community_type, CommunityType::Interest);
        assert_eq!(retrieved.status, CommunityStatus::Active);
    }

    #[test]
    fn test_store_all_community_types() {
        let comm_store = create_test_store();

        for (i, ctype) in [
            CommunityType::Geographic,
            CommunityType::Interest,
            CommunityType::Solidarity,
            CommunityType::Ecosystem,
        ]
        .iter()
        .enumerate()
        {
            let community =
                create_test_community(&format!("comm-{i}"), &format!("Community {i}"), *ctype);
            comm_store.put(&community).unwrap();

            let retrieved = comm_store.get(&format!("comm-{i}")).unwrap().unwrap();
            assert_eq!(retrieved.community_type, *ctype);
        }
    }

    // === Delete tests ===

    #[test]
    fn test_delete_existing_community() {
        let comm_store = create_test_store();

        let community = create_test_community("to-delete", "Delete Me", CommunityType::Geographic);
        comm_store.put(&community).unwrap();

        // Verify it exists
        assert!(comm_store.get(&"to-delete".to_string()).unwrap().is_some());

        // Delete it
        comm_store.delete(&"to-delete".to_string()).unwrap();

        // Verify it's gone
        assert!(comm_store.get(&"to-delete".to_string()).unwrap().is_none());
    }

    #[test]
    fn test_delete_non_existent_succeeds() {
        let comm_store = create_test_store();

        // Should not error when deleting non-existent key
        let result = comm_store.delete(&"never-existed".to_string());
        assert!(result.is_ok());
    }

    #[test]
    fn test_delete_does_not_affect_other_communities() {
        let comm_store = create_test_store();

        let community1 = create_test_community("comm-1", "Community 1", CommunityType::Geographic);
        let community2 = create_test_community("comm-2", "Community 2", CommunityType::Interest);

        comm_store.put(&community1).unwrap();
        comm_store.put(&community2).unwrap();

        // Delete first, second should remain
        comm_store.delete(&"comm-1".to_string()).unwrap();

        assert!(comm_store.get(&"comm-1".to_string()).unwrap().is_none());
        assert!(comm_store.get(&"comm-2".to_string()).unwrap().is_some());
    }

    // === List tests ===

    #[test]
    fn test_list_empty_store() {
        let comm_store = create_test_store();

        let communities = comm_store.list().unwrap();
        assert!(communities.is_empty());
    }

    #[test]
    fn test_list_single_community() {
        let comm_store = create_test_store();

        let community =
            create_test_community("single", "Single Community", CommunityType::Geographic);
        comm_store.put(&community).unwrap();

        let communities = comm_store.list().unwrap();
        assert_eq!(communities.len(), 1);
        assert_eq!(communities[0].id, "single");
    }

    #[test]
    fn test_list_multiple_communities() {
        let comm_store = create_test_store();

        for i in 0..5 {
            let community = create_test_community(
                &format!("comm-{i}"),
                &format!("Community {i}"),
                CommunityType::Geographic,
            );
            comm_store.put(&community).unwrap();
        }

        let communities = comm_store.list().unwrap();
        assert_eq!(communities.len(), 5);

        // Verify all IDs are present (order may vary)
        let ids: Vec<_> = communities.iter().map(|c| c.id.as_str()).collect();
        for i in 0..5 {
            assert!(ids.contains(&format!("comm-{i}").as_str()));
        }
    }

    #[test]
    fn test_list_after_delete() {
        let comm_store = create_test_store();

        for i in 0..3 {
            let community = create_test_community(
                &format!("comm-{i}"),
                &format!("Community {i}"),
                CommunityType::Interest,
            );
            comm_store.put(&community).unwrap();
        }

        // Delete middle one
        comm_store.delete(&"comm-1".to_string()).unwrap();

        let communities = comm_store.list().unwrap();
        assert_eq!(communities.len(), 2);

        let ids: Vec<_> = communities.iter().map(|c| c.id.as_str()).collect();
        assert!(ids.contains(&"comm-0"));
        assert!(!ids.contains(&"comm-1"));
        assert!(ids.contains(&"comm-2"));
    }

    // === Community with members tests ===

    #[test]
    fn test_store_community_with_members() {
        let comm_store = create_test_store();

        let mut community =
            create_test_community("with-members", "With Members", CommunityType::Solidarity);

        // Add some members
        community.members.insert(
            "member-1".to_string(),
            Member {
                id: "member-1".to_string(),
                member_type: MemberType::Individual("did:icn:alice".to_string()),
                joined_at: Utc::now(),
                voting_weight: 1,
                active: true,
            },
        );
        community.members.insert(
            "member-2".to_string(),
            Member {
                id: "member-2".to_string(),
                member_type: MemberType::Cooperative("coop:workers".to_string()),
                joined_at: Utc::now(),
                voting_weight: 5,
                active: true,
            },
        );

        comm_store.put(&community).unwrap();

        let retrieved = comm_store
            .get(&"with-members".to_string())
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.members.len(), 2);
        assert!(retrieved.members.contains_key("member-1"));
        assert!(retrieved.members.contains_key("member-2"));
        assert_eq!(retrieved.total_voting_weight(), 6);
    }

    // === Community with resource pools tests ===

    #[test]
    fn test_store_community_with_resource_pools() {
        let comm_store = create_test_store();

        let mut community =
            create_test_community("with-resources", "With Resources", CommunityType::Ecosystem);

        // Add resource pools
        community.resource_pools.insert(
            "compute".to_string(),
            crate::community_core::types::ResourcePool {
                name: "compute".to_string(),
                resource_type: "cpu".to_string(),
                total_capacity: 100,
                allocated: 25,
                unit: "cores".to_string(),
            },
        );
        community.resource_pools.insert(
            "storage".to_string(),
            crate::community_core::types::ResourcePool {
                name: "storage".to_string(),
                resource_type: "disk".to_string(),
                total_capacity: 1000,
                allocated: 500,
                unit: "GB".to_string(),
            },
        );

        comm_store.put(&community).unwrap();

        let retrieved = comm_store
            .get(&"with-resources".to_string())
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.resource_pools.len(), 2);
        assert!(retrieved.resource_pools.contains_key("compute"));
        assert!(retrieved.resource_pools.contains_key("storage"));

        let compute = retrieved.resource_pools.get("compute").unwrap();
        assert_eq!(compute.available(), 75);
    }

    // === Community with metadata tests ===

    #[test]
    fn test_store_community_with_metadata() {
        let comm_store = create_test_store();

        let mut community =
            create_test_community("with-metadata", "With Metadata", CommunityType::Geographic);

        community
            .metadata
            .insert("region".to_string(), "pacific-northwest".to_string());
        community
            .metadata
            .insert("language".to_string(), "en-US".to_string());
        community.charter = "cooperative://charter/v1".to_string();

        comm_store.put(&community).unwrap();

        let retrieved = comm_store
            .get(&"with-metadata".to_string())
            .unwrap()
            .unwrap();
        assert_eq!(
            retrieved.metadata.get("region").unwrap(),
            "pacific-northwest"
        );
        assert_eq!(retrieved.metadata.get("language").unwrap(), "en-US");
        assert_eq!(retrieved.charter, "cooperative://charter/v1");
    }

    // === Community status persistence ===

    #[test]
    fn test_store_all_community_statuses() {
        let comm_store = create_test_store();

        for (i, status) in [
            CommunityStatus::Forming,
            CommunityStatus::Active,
            CommunityStatus::Suspended,
            CommunityStatus::Dissolved,
        ]
        .iter()
        .enumerate()
        {
            let mut community = create_test_community(
                &format!("status-{i}"),
                &format!("Status {i}"),
                CommunityType::Geographic,
            );
            community.status = *status;

            comm_store.put(&community).unwrap();

            let retrieved = comm_store.get(&format!("status-{i}")).unwrap().unwrap();
            assert_eq!(retrieved.status, *status);
        }
    }

    // === CommunityQuery tests ===

    #[test]
    fn test_community_query_default() {
        let query = CommunityQuery::default();
        assert!(query.status.is_none());
        assert!(query.member_id.is_none());
    }

    #[test]
    fn test_community_query_with_status() {
        let query = CommunityQuery {
            status: Some(CommunityStatus::Active),
            member_id: None,
        };
        assert_eq!(query.status, Some(CommunityStatus::Active));
    }

    #[test]
    fn test_community_query_with_member_id() {
        let query = CommunityQuery {
            status: None,
            member_id: Some("did:icn:abc123".to_string()),
        };
        assert_eq!(query.member_id, Some("did:icn:abc123".to_string()));
    }

    // === Edge cases ===

    #[test]
    fn test_store_community_with_empty_name() {
        let comm_store = create_test_store();

        let community = create_test_community("empty-name", "", CommunityType::Geographic);
        comm_store.put(&community).unwrap();

        let retrieved = comm_store.get(&"empty-name".to_string()).unwrap().unwrap();
        assert_eq!(retrieved.name, "");
    }

    #[test]
    fn test_store_community_with_unicode_name() {
        let comm_store = create_test_store();

        let mut community = Community::new(
            "unicode-comm".to_string(),
            "合作社 Кооператив 協同組合".to_string(),
            CommunityType::Interest,
            "global-domain".to_string(),
        );
        community.metadata.insert(
            "description".to_string(),
            "🌍 International 合作".to_string(),
        );

        comm_store.put(&community).unwrap();

        let retrieved = comm_store
            .get(&"unicode-comm".to_string())
            .unwrap()
            .unwrap();
        assert_eq!(retrieved.name, "合作社 Кооператив 協同組合");
        assert_eq!(
            retrieved.metadata.get("description").unwrap(),
            "🌍 International 合作"
        );
    }

    #[test]
    fn test_store_community_with_long_id() {
        let comm_store = create_test_store();

        let long_id = "a".repeat(256);
        let community =
            create_test_community(&long_id, "Long ID Community", CommunityType::Geographic);

        comm_store.put(&community).unwrap();

        let retrieved = comm_store.get(&long_id).unwrap().unwrap();
        assert_eq!(retrieved.id, long_id);
    }
}
