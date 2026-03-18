use super::error::{CoopError, Result};
use super::types::{Cooperative, Member};
use icn_identity::Did;
use sled::Db;
use std::sync::Arc;

pub struct CoopStore {
    db: Arc<Db>,
}

/// Storage key prefix for treasury nonces.
///
/// The canonical nonce value lives at `treasury_nonce:{coop_id}` as an
/// 8-byte big-endian u64, separate from the cooperative blob. This
/// allows atomic check-and-increment in a sled transaction without
/// deserializing the full `Cooperative` struct.
const TREASURY_NONCE_PREFIX: &str = "treasury_nonce:";

impl CoopStore {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    pub fn save_cooperative(&self, coop: &Cooperative) -> Result<()> {
        let key = format!("coop:{}", coop.id);
        let value = icn_encoding::encode_versioned(coop)?;
        self.db.insert(key.as_bytes(), value)?;
        Ok(())
    }

    pub fn get_cooperative(&self, coop_id: &str) -> Result<Cooperative> {
        let key = format!("coop:{coop_id}");
        let value = self
            .db
            .get(key.as_bytes())?
            .ok_or_else(|| CoopError::NotFound(coop_id.to_string()))?;
        Ok(icn_encoding::decode_versioned(&value)?)
    }

    pub fn list_cooperatives(&self) -> Result<Vec<Cooperative>> {
        let mut coops = Vec::new();
        let prefix = b"coop:";

        for item in self.db.scan_prefix(prefix) {
            let (_, value) = item?;
            let coop: Cooperative = icn_encoding::decode_versioned(&value)?;
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
        let value = icn_encoding::encode_versioned(member)?;
        self.db.insert(key.as_bytes(), value)?;
        Ok(())
    }

    pub fn get_member(&self, coop_id: &str, did: &Did) -> Result<Member> {
        let key = format!("member:{coop_id}:{did}");
        let value = self
            .db
            .get(key.as_bytes())?
            .ok_or_else(|| CoopError::MemberNotFound(did.to_string()))?;
        Ok(icn_encoding::decode_versioned(&value)?)
    }

    pub fn list_members(&self, coop_id: &str) -> Result<Vec<Member>> {
        let mut members = Vec::new();
        let prefix = format!("member:{coop_id}:");

        for item in self.db.scan_prefix(prefix.as_bytes()) {
            let (_, value) = item?;
            let member: Member = icn_encoding::decode_versioned(&value)?;
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
            let member: Member = icn_encoding::decode_versioned(&value)?;
            if member.did == *did {
                coop_ids.push(member.coop_id.clone());
            }
        }

        Ok(coop_ids)
    }

    // =========================================================================
    // Treasury nonce operations
    // =========================================================================

    /// Build the sled key for a treasury nonce.
    ///
    /// The `treasury_id` is typically the treasury DID string, which is
    /// unique per cooperative.
    fn nonce_key(treasury_id: &str) -> Vec<u8> {
        format!("{TREASURY_NONCE_PREFIX}{treasury_id}").into_bytes()
    }

    /// Read the current treasury nonce.
    ///
    /// Returns `0` if no nonce has been stored yet (new cooperative or
    /// cooperative created before nonce support was added).
    ///
    /// # Arguments
    /// * `treasury_id` - Unique treasury identifier (typically the treasury DID)
    pub fn get_treasury_nonce(&self, treasury_id: &str) -> Result<u64> {
        let key = Self::nonce_key(treasury_id);
        match self.db.get(&key)? {
            Some(bytes) => {
                let arr: [u8; 8] = bytes
                    .as_ref()
                    .try_into()
                    .map_err(|_| CoopError::Serialization("corrupt treasury nonce".to_string()))?;
                Ok(u64::from_be_bytes(arr))
            }
            None => Ok(0),
        }
    }

    /// Atomically check and increment the treasury nonce.
    ///
    /// This is the critical double-spend guard for treasury operations.
    /// Within a sled transaction it:
    /// 1. Reads the stored nonce (defaults to 0 if absent).
    /// 2. Verifies it matches `expected_nonce`.
    /// 3. Writes `expected_nonce + 1`.
    ///
    /// If the stored nonce does not match `expected_nonce`, the transaction
    /// aborts and returns `CoopError::NonceMismatch`.
    ///
    /// # Arguments
    /// * `treasury_id` - Unique treasury identifier (typically the treasury DID)
    /// * `expected_nonce` - The nonce value the caller expects to be stored
    ///
    /// # Retry semantics
    ///
    /// Sled may retry the closure on internal conflicts, which is safe because
    /// the closure performs only deterministic reads and writes with no side
    /// effects.
    pub fn check_and_increment_treasury_nonce(
        &self,
        treasury_id: &str,
        expected_nonce: u64,
    ) -> Result<()> {
        use sled::transaction::ConflictableTransactionError;

        let key = Self::nonce_key(treasury_id);
        let treasury_id_owned = treasury_id.to_string();

        let result = self.db.transaction(move |tx| {
            // Read current nonce (0 if absent)
            let stored = match tx.get(&key)? {
                Some(bytes) => {
                    let arr: [u8; 8] = bytes.as_ref().try_into().map_err(|_| {
                        ConflictableTransactionError::Abort(CoopError::Serialization(
                            "corrupt treasury nonce".to_string(),
                        ))
                    })?;
                    u64::from_be_bytes(arr)
                }
                None => 0,
            };

            if stored != expected_nonce {
                return Err(ConflictableTransactionError::Abort(
                    CoopError::NonceMismatch {
                        coop_id: treasury_id_owned.clone(),
                        expected: expected_nonce,
                        stored,
                    },
                ));
            }

            // Increment
            let next = expected_nonce + 1;
            tx.insert(key.as_slice(), &next.to_be_bytes())?;
            Ok(())
        });

        match result {
            Ok(()) => Ok(()),
            Err(sled::transaction::TransactionError::Abort(coop_err)) => Err(coop_err),
            Err(sled::transaction::TransactionError::Storage(e)) => Err(CoopError::Storage(e)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coop_core::types::{CoopStatus, CoopType, MemberRole, MemberStatus};
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
        let tier = crate::coop_core::types::MembershipTier {
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
        coop.add_tier(crate::coop_core::types::MembershipTier::founder());
        coop.add_tier(crate::coop_core::types::MembershipTier::standard("Worker"));
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

    // === Treasury nonce tests (Issue #1089) ===

    #[test]
    fn test_treasury_nonce_defaults_to_zero() {
        let store = create_test_store();
        let nonce = store.get_treasury_nonce("did:icn:treasury1").unwrap();
        assert_eq!(nonce, 0, "Fresh treasury nonce must default to 0");
    }

    #[test]
    fn test_treasury_nonce_check_and_increment() {
        let store = create_test_store();
        let tid = "did:icn:treasury1";

        // First spend: nonce 0 -> 1
        store.check_and_increment_treasury_nonce(tid, 0).unwrap();
        assert_eq!(store.get_treasury_nonce(tid).unwrap(), 1);

        // Second spend: nonce 1 -> 2
        store.check_and_increment_treasury_nonce(tid, 1).unwrap();
        assert_eq!(store.get_treasury_nonce(tid).unwrap(), 2);

        // Third spend: nonce 2 -> 3
        store.check_and_increment_treasury_nonce(tid, 2).unwrap();
        assert_eq!(store.get_treasury_nonce(tid).unwrap(), 3);
    }

    #[test]
    fn test_treasury_nonce_rejects_mismatch() {
        let store = create_test_store();
        let tid = "did:icn:treasury1";

        // Advance nonce to 1
        store.check_and_increment_treasury_nonce(tid, 0).unwrap();

        // Replay with nonce 0 must fail
        let err = store
            .check_and_increment_treasury_nonce(tid, 0)
            .unwrap_err();
        assert!(
            matches!(
                err,
                CoopError::NonceMismatch {
                    expected: 0,
                    stored: 1,
                    ..
                }
            ),
            "Expected NonceMismatch, got: {err:?}"
        );

        // Nonce must remain at 1 (not incremented on failure)
        assert_eq!(store.get_treasury_nonce(tid).unwrap(), 1);
    }

    #[test]
    fn test_treasury_nonce_double_submit_fails() {
        let store = create_test_store();
        let tid = "did:icn:treasury1";

        // First submit succeeds
        store.check_and_increment_treasury_nonce(tid, 0).unwrap();

        // Same nonce again must fail
        let err = store
            .check_and_increment_treasury_nonce(tid, 0)
            .unwrap_err();
        assert!(matches!(err, CoopError::NonceMismatch { .. }));
    }

    #[test]
    fn test_treasury_nonce_future_nonce_fails() {
        let store = create_test_store();
        let tid = "did:icn:treasury1";

        // Trying nonce=5 when stored=0 must fail
        let err = store
            .check_and_increment_treasury_nonce(tid, 5)
            .unwrap_err();
        assert!(
            matches!(
                err,
                CoopError::NonceMismatch {
                    expected: 5,
                    stored: 0,
                    ..
                }
            ),
            "Expected NonceMismatch for future nonce, got: {err:?}"
        );
    }

    #[test]
    fn test_treasury_nonce_independent_per_treasury() {
        let store = create_test_store();
        let t1 = "did:icn:treasury1";
        let t2 = "did:icn:treasury2";

        // Advance t1 to nonce 3
        store.check_and_increment_treasury_nonce(t1, 0).unwrap();
        store.check_and_increment_treasury_nonce(t1, 1).unwrap();
        store.check_and_increment_treasury_nonce(t1, 2).unwrap();

        // t2 should still be at 0
        assert_eq!(store.get_treasury_nonce(t2).unwrap(), 0);

        // t2's first spend should use nonce 0
        store.check_and_increment_treasury_nonce(t2, 0).unwrap();
        assert_eq!(store.get_treasury_nonce(t2).unwrap(), 1);

        // t1 should still be at 3
        assert_eq!(store.get_treasury_nonce(t1).unwrap(), 3);
    }

    #[test]
    fn test_treasury_nonce_survives_reopen() {
        // Test that the nonce persists across store reopens (disk persistence)
        let dir = tempdir().unwrap();
        let db_path = dir.path().to_path_buf();

        // Open store, increment nonce, close
        {
            let db = sled::open(&db_path).unwrap();
            let store = CoopStore::new(Arc::new(db));
            store
                .check_and_increment_treasury_nonce("did:icn:persistent", 0)
                .unwrap();
            store
                .check_and_increment_treasury_nonce("did:icn:persistent", 1)
                .unwrap();
            // db drops here, flushing to disk
        }

        // Reopen store and verify nonce was persisted
        {
            let db = sled::open(&db_path).unwrap();
            let store = CoopStore::new(Arc::new(db));
            let nonce = store.get_treasury_nonce("did:icn:persistent").unwrap();
            assert_eq!(nonce, 2, "Nonce must survive store reopen");

            // Next increment should work
            store
                .check_and_increment_treasury_nonce("did:icn:persistent", 2)
                .unwrap();
            assert_eq!(store.get_treasury_nonce("did:icn:persistent").unwrap(), 3);
        }
    }

    #[test]
    fn test_treasury_nonce_monotonic_no_gaps() {
        let store = create_test_store();
        let tid = "did:icn:monotonic";

        // Increment through 0..100
        for i in 0u64..100 {
            store.check_and_increment_treasury_nonce(tid, i).unwrap();
            assert_eq!(store.get_treasury_nonce(tid).unwrap(), i + 1);
        }
    }

    #[test]
    fn test_treasury_nonce_concurrent_proposals() {
        use std::sync::Barrier;
        use std::thread;

        let dir = tempdir().unwrap();
        let db = Arc::new(sled::open(dir.path()).unwrap());
        let store = Arc::new(CoopStore::new(db));
        let tid = "did:icn:concurrent";
        let num_threads = 8;

        let barrier = Arc::new(Barrier::new(num_threads));
        let mut handles = Vec::new();

        // All threads try to claim nonce=0 simultaneously
        for _ in 0..num_threads {
            let store = Arc::clone(&store);
            let barrier = Arc::clone(&barrier);
            let tid = tid.to_string();
            handles.push(thread::spawn(move || {
                barrier.wait();
                store.check_and_increment_treasury_nonce(&tid, 0)
            }));
        }

        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let successes = results.iter().filter(|r| r.is_ok()).count();
        let failures = results.iter().filter(|r| r.is_err()).count();

        // Exactly one thread should succeed with nonce=0
        assert_eq!(
            successes, 1,
            "Exactly one concurrent proposal must win the nonce race"
        );
        assert_eq!(failures, num_threads - 1);

        // Nonce should be exactly 1
        assert_eq!(store.get_treasury_nonce(tid).unwrap(), 1);
    }
}
