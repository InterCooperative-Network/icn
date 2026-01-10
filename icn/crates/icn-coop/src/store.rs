use crate::{CoopError, Cooperative, Member, Result};
use icn_identity::Did;
use sled::Db;
use std::sync::Arc;

pub struct CoopStore {
    db: Arc<Db>,
}

impl CoopStore {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    pub fn save_cooperative(&self, coop: &Cooperative) -> Result<()> {
        let key = format!("coop:{}", coop.id);
        let value = bincode::serde::encode_to_vec(coop, bincode::config::legacy())?;
        self.db.insert(key.as_bytes(), value)?;
        Ok(())
    }

    pub fn get_cooperative(&self, coop_id: &str) -> Result<Cooperative> {
        let key = format!("coop:{coop_id}");
        let value = self
            .db
            .get(key.as_bytes())?
            .ok_or_else(|| CoopError::NotFound(coop_id.to_string()))?;
        Ok(bincode::serde::decode_from_slice(&value, bincode::config::legacy()).map(|(v, _)| v)?)
    }

    pub fn list_cooperatives(&self) -> Result<Vec<Cooperative>> {
        let mut coops = Vec::new();
        let prefix = b"coop:";

        for item in self.db.scan_prefix(prefix) {
            let (_, value) = item?;
            let coop: Cooperative =
                bincode::serde::decode_from_slice(&value, bincode::config::legacy())
                    .map(|(v, _)| v)?;
            coops.push(coop);
        }

        Ok(coops)
    }

    pub fn delete_cooperative(&self, coop_id: &str) -> Result<()> {
        let key = format!("coop:{coop_id}");
        self.db.remove(key.as_bytes())?;
        Ok(())
    }

    pub fn save_member(&self, member: &Member) -> Result<()> {
        let key = format!("member:{}:{}", member.coop_id, member.did);
        let value = bincode::serde::encode_to_vec(member, bincode::config::legacy())?;
        self.db.insert(key.as_bytes(), value)?;
        Ok(())
    }

    pub fn get_member(&self, coop_id: &str, did: &Did) -> Result<Member> {
        let key = format!("member:{coop_id}:{did}");
        let value = self
            .db
            .get(key.as_bytes())?
            .ok_or_else(|| CoopError::MemberNotFound(did.to_string()))?;
        Ok(bincode::serde::decode_from_slice(&value, bincode::config::legacy()).map(|(v, _)| v)?)
    }

    pub fn list_members(&self, coop_id: &str) -> Result<Vec<Member>> {
        let mut members = Vec::new();
        let prefix = format!("member:{coop_id}:");

        for item in self.db.scan_prefix(prefix.as_bytes()) {
            let (_, value) = item?;
            let member: Member =
                bincode::serde::decode_from_slice(&value, bincode::config::legacy())
                    .map(|(v, _)| v)?;
            members.push(member);
        }

        Ok(members)
    }

    pub fn delete_member(&self, coop_id: &str, did: &Did) -> Result<()> {
        let key = format!("member:{coop_id}:{did}");
        self.db.remove(key.as_bytes())?;
        Ok(())
    }

    pub fn get_member_coops(&self, did: &Did) -> Result<Vec<String>> {
        let mut coop_ids = Vec::new();
        let prefix = b"member:";

        for item in self.db.scan_prefix(prefix) {
            let (_key, value) = item?;
            let member: Member =
                bincode::serde::decode_from_slice(&value, bincode::config::legacy())
                    .map(|(v, _)| v)?;
            if member.did == *did {
                coop_ids.push(member.coop_id.clone());
            }
        }

        Ok(coop_ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CoopStatus, CoopType, MemberRole, MemberStatus};
    use icn_identity::KeyPair;
    use tempfile::tempdir;

    fn create_test_store() -> CoopStore {
        let dir = tempdir().unwrap();
        let db = sled::open(dir.path()).unwrap();
        CoopStore::new(Arc::new(db))
    }

    fn create_test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    // === Cooperative storage tests ===

    #[test]
    fn test_coop_storage() {
        let store = create_test_store();

        let coop = Cooperative::new("Test Coop".to_string(), CoopType::Worker);
        store.save_cooperative(&coop).unwrap();

        let loaded = store.get_cooperative(&coop.id).unwrap();
        assert_eq!(loaded.name, coop.name);
        assert_eq!(loaded.coop_type, coop.coop_type);
    }

    #[test]
    fn test_coop_storage_with_explicit_id() {
        let store = create_test_store();

        let coop = Cooperative::new_with_id(
            "my-coop-123".to_string(),
            "My Coop".to_string(),
            CoopType::Consumer,
        );
        store.save_cooperative(&coop).unwrap();

        let loaded = store.get_cooperative("my-coop-123").unwrap();
        assert_eq!(loaded.id, "my-coop-123");
        assert_eq!(loaded.name, "My Coop");
        assert_eq!(loaded.coop_type, CoopType::Consumer);
    }

    #[test]
    fn test_coop_storage_all_types() {
        let store = create_test_store();

        for coop_type in [
            CoopType::Worker,
            CoopType::Consumer,
            CoopType::Producer,
            CoopType::MultiStakeholder,
            CoopType::Platform,
            CoopType::Housing,
            CoopType::Credit,
        ] {
            let coop = Cooperative::new(format!("Coop {:?}", coop_type), coop_type);
            store.save_cooperative(&coop).unwrap();

            let loaded = store.get_cooperative(&coop.id).unwrap();
            assert_eq!(loaded.coop_type, coop_type);
        }
    }

    #[test]
    fn test_coop_update() {
        let store = create_test_store();

        let mut coop = Cooperative::new("Test Coop".to_string(), CoopType::Worker);
        store.save_cooperative(&coop).unwrap();

        // Update the cooperative
        coop.name = "Updated Coop".to_string();
        coop.status = CoopStatus::Active;
        coop.charter_hash = Some("abc123".to_string());
        store.save_cooperative(&coop).unwrap();

        let loaded = store.get_cooperative(&coop.id).unwrap();
        assert_eq!(loaded.name, "Updated Coop");
        assert_eq!(loaded.status, CoopStatus::Active);
        assert_eq!(loaded.charter_hash, Some("abc123".to_string()));
    }

    #[test]
    fn test_coop_not_found() {
        let store = create_test_store();

        let result = store.get_cooperative("nonexistent-coop");
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(matches!(err, CoopError::NotFound(_)));
    }

    #[test]
    fn test_list_cooperatives_empty() {
        let store = create_test_store();

        let coops = store.list_cooperatives().unwrap();
        assert!(coops.is_empty());
    }

    #[test]
    fn test_list_cooperatives() {
        let store = create_test_store();

        let coop1 = Cooperative::new("Coop 1".to_string(), CoopType::Worker);
        let coop2 = Cooperative::new("Coop 2".to_string(), CoopType::Consumer);
        let coop3 = Cooperative::new("Coop 3".to_string(), CoopType::Producer);

        store.save_cooperative(&coop1).unwrap();
        store.save_cooperative(&coop2).unwrap();
        store.save_cooperative(&coop3).unwrap();

        let coops = store.list_cooperatives().unwrap();
        assert_eq!(coops.len(), 3);

        let names: Vec<_> = coops.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"Coop 1"));
        assert!(names.contains(&"Coop 2"));
        assert!(names.contains(&"Coop 3"));
    }

    #[test]
    fn test_delete_cooperative() {
        let store = create_test_store();

        let coop = Cooperative::new("Test Coop".to_string(), CoopType::Worker);
        store.save_cooperative(&coop).unwrap();

        // Verify it exists
        assert!(store.get_cooperative(&coop.id).is_ok());

        // Delete it
        store.delete_cooperative(&coop.id).unwrap();

        // Verify it's gone
        assert!(store.get_cooperative(&coop.id).is_err());
    }

    #[test]
    fn test_delete_cooperative_nonexistent() {
        let store = create_test_store();

        // Deleting a nonexistent coop doesn't error (sled returns Ok)
        let result = store.delete_cooperative("nonexistent");
        assert!(result.is_ok());
    }

    // === Member storage tests ===

    #[test]
    fn test_member_storage() {
        let store = create_test_store();

        let did = create_test_did();
        let member = Member::new(did.clone(), "coop:123".to_string(), MemberRole::Worker);
        store.save_member(&member).unwrap();

        let loaded = store.get_member("coop:123", &did).unwrap();
        assert_eq!(loaded.did, member.did);
        assert_eq!(loaded.role, member.role);
    }

    #[test]
    fn test_member_storage_all_roles() {
        let store = create_test_store();

        for role in [
            MemberRole::Founder,
            MemberRole::Member,
            MemberRole::Worker,
            MemberRole::Consumer,
            MemberRole::Producer,
            MemberRole::BoardMember,
            MemberRole::Officer,
        ] {
            let did = create_test_did();
            let member = Member::new(did.clone(), "coop:roles".to_string(), role);
            store.save_member(&member).unwrap();

            let loaded = store.get_member("coop:roles", &did).unwrap();
            assert_eq!(loaded.role, role);
        }
    }

    #[test]
    fn test_member_update() {
        let store = create_test_store();

        let did = create_test_did();
        let mut member = Member::new(did.clone(), "coop:123".to_string(), MemberRole::Member);
        member.status = MemberStatus::Pending;
        store.save_member(&member).unwrap();

        // Update the member
        member.status = MemberStatus::Active;
        member.role = MemberRole::Worker;
        member.shares = 100;
        store.save_member(&member).unwrap();

        let loaded = store.get_member("coop:123", &did).unwrap();
        assert_eq!(loaded.status, MemberStatus::Active);
        assert_eq!(loaded.role, MemberRole::Worker);
        assert_eq!(loaded.shares, 100);
    }

    #[test]
    fn test_member_not_found() {
        let store = create_test_store();

        let did = create_test_did();
        let result = store.get_member("coop:123", &did);
        assert!(result.is_err());

        let err = result.unwrap_err();
        assert!(matches!(err, CoopError::MemberNotFound(_)));
    }

    #[test]
    fn test_list_members_empty() {
        let store = create_test_store();

        let members = store.list_members("coop:123").unwrap();
        assert!(members.is_empty());
    }

    #[test]
    fn test_list_members() {
        let store = create_test_store();

        let did1 = create_test_did();
        let did2 = create_test_did();
        let did3 = create_test_did();

        let member1 = Member::new(did1.clone(), "coop:123".to_string(), MemberRole::Founder);
        let member2 = Member::new(did2.clone(), "coop:123".to_string(), MemberRole::Worker);
        let member3 = Member::new(did3.clone(), "coop:123".to_string(), MemberRole::Member);

        store.save_member(&member1).unwrap();
        store.save_member(&member2).unwrap();
        store.save_member(&member3).unwrap();

        let members = store.list_members("coop:123").unwrap();
        assert_eq!(members.len(), 3);

        let dids: Vec<_> = members.iter().map(|m| &m.did).collect();
        assert!(dids.contains(&&did1));
        assert!(dids.contains(&&did2));
        assert!(dids.contains(&&did3));
    }

    #[test]
    fn test_list_members_different_coops() {
        let store = create_test_store();

        let did1 = create_test_did();
        let did2 = create_test_did();
        let did3 = create_test_did();

        let member1 = Member::new(did1, "coop:A".to_string(), MemberRole::Worker);
        let member2 = Member::new(did2, "coop:A".to_string(), MemberRole::Worker);
        let member3 = Member::new(did3, "coop:B".to_string(), MemberRole::Worker);

        store.save_member(&member1).unwrap();
        store.save_member(&member2).unwrap();
        store.save_member(&member3).unwrap();

        // List members of coop:A
        let members_a = store.list_members("coop:A").unwrap();
        assert_eq!(members_a.len(), 2);

        // List members of coop:B
        let members_b = store.list_members("coop:B").unwrap();
        assert_eq!(members_b.len(), 1);
    }

    #[test]
    fn test_delete_member() {
        let store = create_test_store();

        let did = create_test_did();
        let member = Member::new(did.clone(), "coop:123".to_string(), MemberRole::Worker);
        store.save_member(&member).unwrap();

        // Verify it exists
        assert!(store.get_member("coop:123", &did).is_ok());

        // Delete it
        store.delete_member("coop:123", &did).unwrap();

        // Verify it's gone
        assert!(store.get_member("coop:123", &did).is_err());
    }

    #[test]
    fn test_delete_member_nonexistent() {
        let store = create_test_store();

        let did = create_test_did();
        // Deleting a nonexistent member doesn't error
        let result = store.delete_member("coop:123", &did);
        assert!(result.is_ok());
    }

    // === Cross-entity query tests ===

    #[test]
    fn test_get_member_coops_empty() {
        let store = create_test_store();

        let did = create_test_did();
        let coops = store.get_member_coops(&did).unwrap();
        assert!(coops.is_empty());
    }

    #[test]
    fn test_get_member_coops() {
        let store = create_test_store();

        let did = create_test_did();

        // Add member to multiple coops
        let member1 = Member::new(did.clone(), "coop:A".to_string(), MemberRole::Worker);
        let member2 = Member::new(did.clone(), "coop:B".to_string(), MemberRole::Member);
        let member3 = Member::new(did.clone(), "coop:C".to_string(), MemberRole::Founder);

        store.save_member(&member1).unwrap();
        store.save_member(&member2).unwrap();
        store.save_member(&member3).unwrap();

        let coops = store.get_member_coops(&did).unwrap();
        assert_eq!(coops.len(), 3);
        assert!(coops.contains(&"coop:A".to_string()));
        assert!(coops.contains(&"coop:B".to_string()));
        assert!(coops.contains(&"coop:C".to_string()));
    }

    #[test]
    fn test_get_member_coops_excludes_other_members() {
        let store = create_test_store();

        let did1 = create_test_did();
        let did2 = create_test_did();

        // did1 is in coop:A and coop:B
        let member1a = Member::new(did1.clone(), "coop:A".to_string(), MemberRole::Worker);
        let member1b = Member::new(did1.clone(), "coop:B".to_string(), MemberRole::Worker);

        // did2 is only in coop:C
        let member2c = Member::new(did2.clone(), "coop:C".to_string(), MemberRole::Worker);

        store.save_member(&member1a).unwrap();
        store.save_member(&member1b).unwrap();
        store.save_member(&member2c).unwrap();

        // Query for did1 should only return A and B
        let coops1 = store.get_member_coops(&did1).unwrap();
        assert_eq!(coops1.len(), 2);
        assert!(coops1.contains(&"coop:A".to_string()));
        assert!(coops1.contains(&"coop:B".to_string()));
        assert!(!coops1.contains(&"coop:C".to_string()));

        // Query for did2 should only return C
        let coops2 = store.get_member_coops(&did2).unwrap();
        assert_eq!(coops2.len(), 1);
        assert!(coops2.contains(&"coop:C".to_string()));
    }

    // === Member metadata tests ===

    #[test]
    fn test_member_with_metadata() {
        let store = create_test_store();

        let did = create_test_did();
        let mut member = Member::new(did.clone(), "coop:123".to_string(), MemberRole::Worker);
        member
            .metadata
            .insert("department".to_string(), "engineering".to_string());
        member
            .metadata
            .insert("start_date".to_string(), "2024-01-01".to_string());

        store.save_member(&member).unwrap();

        let loaded = store.get_member("coop:123", &did).unwrap();
        assert_eq!(
            loaded.metadata.get("department"),
            Some(&"engineering".to_string())
        );
        assert_eq!(
            loaded.metadata.get("start_date"),
            Some(&"2024-01-01".to_string())
        );
    }

    // === Cooperative metadata tests ===

    #[test]
    fn test_coop_with_metadata() {
        let store = create_test_store();

        let mut coop = Cooperative::new("Test Coop".to_string(), CoopType::Worker);
        coop.metadata
            .insert("location".to_string(), "San Francisco".to_string());
        coop.metadata
            .insert("founded".to_string(), "2024".to_string());

        store.save_cooperative(&coop).unwrap();

        let loaded = store.get_cooperative(&coop.id).unwrap();
        assert_eq!(
            loaded.metadata.get("location"),
            Some(&"San Francisco".to_string())
        );
        assert_eq!(loaded.metadata.get("founded"), Some(&"2024".to_string()));
    }

    // === Member with tier and capital tests ===

    #[test]
    fn test_member_with_tier() {
        let store = create_test_store();

        let did = create_test_did();
        let tier = crate::MembershipTier {
            name: "Senior Worker".to_string(),
            voting_weight: 2,
            profit_share_weight: 3,
            governance_rights: vec!["vote".to_string(), "propose".to_string()],
        };

        let member = Member::new(did.clone(), "coop:123".to_string(), MemberRole::Worker)
            .with_tier(tier)
            .with_capital(1000);

        store.save_member(&member).unwrap();

        let loaded = store.get_member("coop:123", &did).unwrap();
        assert_eq!(loaded.capital_contribution, 1000);
        assert!(loaded.tier.is_some());
        let loaded_tier = loaded.tier.unwrap();
        assert_eq!(loaded_tier.name, "Senior Worker");
        assert_eq!(loaded_tier.voting_weight, 2);
    }

    // === Cooperative with advanced fields tests ===

    #[test]
    fn test_coop_with_tiers() {
        let store = create_test_store();

        let mut coop = Cooperative::new("Test Coop".to_string(), CoopType::Worker);
        coop.add_tier(crate::MembershipTier::founder());
        coop.add_tier(crate::MembershipTier::standard("Worker"));
        coop.add_capital(10000);

        store.save_cooperative(&coop).unwrap();

        let loaded = store.get_cooperative(&coop.id).unwrap();
        assert_eq!(loaded.tiers.len(), 2);
        assert_eq!(loaded.capital_pool, 10000);
    }

    #[test]
    fn test_coop_with_bylaws() {
        let store = create_test_store();

        let mut coop = Cooperative::new("Test Coop".to_string(), CoopType::Worker);
        coop.bylaws = vec!["bylaw-hash-1".to_string(), "bylaw-hash-2".to_string()];

        store.save_cooperative(&coop).unwrap();

        let loaded = store.get_cooperative(&coop.id).unwrap();
        assert_eq!(loaded.bylaws.len(), 2);
        assert!(loaded.bylaws.contains(&"bylaw-hash-1".to_string()));
    }
}
