use crate::{Cooperative, Member, Result, CoopError};
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
        let value = bincode::serialize(coop)?;
        self.db.insert(key.as_bytes(), value)?;
        Ok(())
    }

    pub fn get_cooperative(&self, coop_id: &str) -> Result<Cooperative> {
        let key = format!("coop:{coop_id}");
        let value = self.db.get(key.as_bytes())?
            .ok_or_else(|| CoopError::NotFound(coop_id.to_string()))?;
        Ok(bincode::deserialize(&value)?)
    }

    pub fn list_cooperatives(&self) -> Result<Vec<Cooperative>> {
        let mut coops = Vec::new();
        let prefix = b"coop:";
        
        for item in self.db.scan_prefix(prefix) {
            let (_, value) = item?;
            let coop: Cooperative = bincode::deserialize(&value)?;
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
        let value = bincode::serialize(member)?;
        self.db.insert(key.as_bytes(), value)?;
        Ok(())
    }

    pub fn get_member(&self, coop_id: &str, did: &Did) -> Result<Member> {
        let key = format!("member:{coop_id}:{did}");
        let value = self.db.get(key.as_bytes())?
            .ok_or_else(|| CoopError::MemberNotFound(did.to_string()))?;
        Ok(bincode::deserialize(&value)?)
    }

    pub fn list_members(&self, coop_id: &str) -> Result<Vec<Member>> {
        let mut members = Vec::new();
        let prefix = format!("member:{coop_id}:");
        
        for item in self.db.scan_prefix(prefix.as_bytes()) {
            let (_, value) = item?;
            let member: Member = bincode::deserialize(&value)?;
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
            let member: Member = bincode::deserialize(&value)?;
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
    use crate::{CoopType, MemberRole};
    use tempfile::tempdir;

    #[test]
    fn test_coop_storage() {
        let dir = tempdir().unwrap();
        let db = sled::open(dir.path()).unwrap();
        let store = CoopStore::new(Arc::new(db));

        let coop = Cooperative::new("Test Coop".to_string(), CoopType::Worker);
        store.save_cooperative(&coop).unwrap();

        let loaded = store.get_cooperative(&coop.id).unwrap();
        assert_eq!(loaded.name, coop.name);
        assert_eq!(loaded.coop_type, coop.coop_type);
    }

    #[test]
    fn test_member_storage() {
        let dir = tempdir().unwrap();
        let db = sled::open(dir.path()).unwrap();
        let store = CoopStore::new(Arc::new(db));

        // Generate a valid DID
        let keypair = icn_identity::KeyPair::generate().unwrap();
        let did = keypair.did().clone();
        
        let member = Member::new(did.clone(), "coop:123".to_string(), MemberRole::Worker);
        store.save_member(&member).unwrap();

        let loaded = store.get_member("coop:123", &did).unwrap();
        assert_eq!(loaded.did, member.did);
        assert_eq!(loaded.role, member.role);
    }
}
