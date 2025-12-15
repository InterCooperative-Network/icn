//! Revocation Registry Store
//!
//! Provides persistent storage for RevocationRecords with indexes for:
//! - Target ID lookup (is this entity revoked?)
//! - Target type filtering
//! - Scope filtering
//! - Appeal status filtering

use crate::revocation::{
    AppealStatus, RevocationCheck, RevocationRecord, RevocationScope, RevocationType,
};
use crate::JurisdictionId;
use std::collections::HashMap;
use std::sync::RwLock;

/// In-memory revocation registry store
pub struct RevocationRegistry {
    /// All revocation records by revocation_id (hex)
    records: RwLock<HashMap<String, RevocationRecord>>,

    /// Index: target_id -> list of revocation_ids
    /// (a target can have multiple revocations at different scopes)
    by_target: RwLock<HashMap<String, Vec<String>>>,

    /// Index: target_type -> list of revocation_ids
    by_type: RwLock<HashMap<RevocationType, Vec<String>>>,
}

impl RevocationRegistry {
    /// Create a new empty registry
    pub fn new() -> Self {
        Self {
            records: RwLock::new(HashMap::new()),
            by_target: RwLock::new(HashMap::new()),
            by_type: RwLock::new(HashMap::new()),
        }
    }

    /// Add a revocation record to the registry
    pub fn add(&self, record: RevocationRecord) -> Result<(), String> {
        let revocation_id = record.id_hex();
        let target_id = record.target_id.clone();
        let target_type = record.target_type;

        // Store the record
        {
            let mut records = self.records.write().map_err(|e| e.to_string())?;
            if records.contains_key(&revocation_id) {
                return Err("Revocation already exists".to_string());
            }
            records.insert(revocation_id.clone(), record);
        }

        // Update target index
        {
            let mut by_target = self.by_target.write().map_err(|e| e.to_string())?;
            by_target
                .entry(target_id)
                .or_default()
                .push(revocation_id.clone());
        }

        // Update type index
        {
            let mut by_type = self.by_type.write().map_err(|e| e.to_string())?;
            by_type.entry(target_type).or_default().push(revocation_id);
        }

        Ok(())
    }

    /// Get a revocation record by ID
    pub fn get(&self, revocation_id: &str) -> Option<RevocationRecord> {
        let records = self.records.read().ok()?;
        records.get(revocation_id).cloned()
    }

    /// Check if a target is revoked at any scope
    pub fn check(&self, target_id: &str) -> RevocationCheck {
        let by_target = match self.by_target.read() {
            Ok(t) => t,
            Err(_) => return RevocationCheck::not_revoked(),
        };

        let revocation_ids = match by_target.get(target_id) {
            Some(ids) => ids,
            None => return RevocationCheck::not_revoked(),
        };

        let records = match self.records.read() {
            Ok(r) => r,
            Err(_) => return RevocationCheck::not_revoked(),
        };

        // Find the first effective revocation
        for id in revocation_ids {
            if let Some(record) = records.get(id) {
                if record.is_effective() {
                    return RevocationCheck::revoked(record.clone());
                }
            }
        }

        RevocationCheck::not_revoked()
    }

    /// Check if a target is revoked at a specific scope
    pub fn check_at_scope(&self, target_id: &str, scope: &RevocationScope) -> RevocationCheck {
        let by_target = match self.by_target.read() {
            Ok(t) => t,
            Err(_) => return RevocationCheck::not_revoked(),
        };

        let revocation_ids = match by_target.get(target_id) {
            Some(ids) => ids,
            None => return RevocationCheck::not_revoked(),
        };

        let records = match self.records.read() {
            Ok(r) => r,
            Err(_) => return RevocationCheck::not_revoked(),
        };

        for id in revocation_ids {
            if let Some(record) = records.get(id) {
                if record.is_effective() && self.scope_applies(&record.scope, scope) {
                    return RevocationCheck::revoked(record.clone());
                }
            }
        }

        RevocationCheck::not_revoked()
    }

    /// Check if a scope applies to another scope
    /// Global applies everywhere, Federation applies within federation, etc.
    fn scope_applies(&self, revocation_scope: &RevocationScope, check_scope: &RevocationScope) -> bool {
        match revocation_scope {
            RevocationScope::Global => true, // Global applies everywhere
            RevocationScope::Federation { federation_id } => {
                // Federation scope applies to the federation and its jurisdictions
                match check_scope {
                    RevocationScope::Global => false,
                    RevocationScope::Federation { federation_id: check_fed } => {
                        federation_id == check_fed
                    }
                    RevocationScope::Jurisdiction { jurisdiction_id } => {
                        // Check if jurisdiction is within this federation
                        // For now, simple check - in practice would need federation membership lookup
                        jurisdiction_id.as_str().contains(federation_id.as_str())
                    }
                }
            }
            RevocationScope::Jurisdiction { jurisdiction_id } => {
                // Jurisdiction scope only applies within that jurisdiction
                match check_scope {
                    RevocationScope::Jurisdiction { jurisdiction_id: check_jur } => {
                        jurisdiction_id == check_jur
                    }
                    _ => false,
                }
            }
        }
    }

    /// List all revocations for a target
    pub fn list_for_target(&self, target_id: &str) -> Vec<RevocationRecord> {
        let by_target = match self.by_target.read() {
            Ok(t) => t,
            Err(_) => return Vec::new(),
        };

        let revocation_ids = match by_target.get(target_id) {
            Some(ids) => ids,
            None => return Vec::new(),
        };

        let records = match self.records.read() {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        revocation_ids
            .iter()
            .filter_map(|id| records.get(id).cloned())
            .collect()
    }

    /// List all revocations by type
    pub fn list_by_type(&self, target_type: RevocationType) -> Vec<RevocationRecord> {
        let by_type = match self.by_type.read() {
            Ok(t) => t,
            Err(_) => return Vec::new(),
        };

        let revocation_ids = match by_type.get(&target_type) {
            Some(ids) => ids,
            None => return Vec::new(),
        };

        let records = match self.records.read() {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        revocation_ids
            .iter()
            .filter_map(|id| records.get(id).cloned())
            .collect()
    }

    /// List all pending appeals
    pub fn list_pending_appeals(&self) -> Vec<RevocationRecord> {
        let records = match self.records.read() {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        records
            .values()
            .filter(|r| matches!(r.appeal_status, AppealStatus::Pending { .. }))
            .cloned()
            .collect()
    }

    /// List all effective revocations
    pub fn list_effective(&self) -> Vec<RevocationRecord> {
        let records = match self.records.read() {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        records.values().filter(|r| r.is_effective()).cloned().collect()
    }

    /// Update a revocation record (for appeals)
    pub fn update(&self, record: RevocationRecord) -> Result<(), String> {
        let revocation_id = record.id_hex();

        let mut records = self.records.write().map_err(|e| e.to_string())?;
        if !records.contains_key(&revocation_id) {
            return Err("Revocation not found".to_string());
        }
        records.insert(revocation_id, record);
        Ok(())
    }

    /// Check if target is revoked within a specific jurisdiction
    pub fn is_revoked_in_jurisdiction(
        &self,
        target_id: &str,
        jurisdiction_id: &JurisdictionId,
    ) -> bool {
        let check = self.check_at_scope(
            target_id,
            &RevocationScope::Jurisdiction {
                jurisdiction_id: jurisdiction_id.clone(),
            },
        );
        check.is_revoked
    }

    /// Count total revocations
    pub fn count(&self) -> usize {
        self.records.read().map(|r| r.len()).unwrap_or(0)
    }

    /// Count effective revocations
    pub fn count_effective(&self) -> usize {
        self.records
            .read()
            .map(|r| r.values().filter(|rec| rec.is_effective()).count())
            .unwrap_or(0)
    }
}

impl Default for RevocationRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::revocation::CommonsRevocationReason;
    use crate::KeyPair;

    fn test_authority() -> crate::Did {
        KeyPair::generate().unwrap().did().clone()
    }

    #[test]
    fn test_registry_add_and_get() {
        let registry = RevocationRegistry::new();
        let authority = test_authority();

        let record = RevocationRecord::immediate(
            RevocationType::Membership,
            "member123".to_string(),
            RevocationScope::Jurisdiction {
                jurisdiction_id: JurisdictionId::coop("test-coop"),
            },
            authority,
            CommonsRevocationReason::PolicyViolation {
                policy_ref: "POL-001".to_string(),
                details: "Violation".to_string(),
            },
        );

        let revocation_id = record.id_hex();
        registry.add(record).unwrap();

        let retrieved = registry.get(&revocation_id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().target_id, "member123");
    }

    #[test]
    fn test_check_revocation() {
        let registry = RevocationRegistry::new();
        let authority = test_authority();

        // Not revoked initially
        let check = registry.check("member123");
        assert!(!check.is_revoked);

        // Add immediate revocation
        let record = RevocationRecord::immediate(
            RevocationType::Membership,
            "member123".to_string(),
            RevocationScope::Global,
            authority,
            CommonsRevocationReason::SybilAttack {
                evidence_summary: "Duplicate VUI".to_string(),
            },
        );
        registry.add(record).unwrap();

        // Now revoked
        let check = registry.check("member123");
        assert!(check.is_revoked);
    }

    #[test]
    fn test_check_at_scope() {
        let registry = RevocationRegistry::new();
        let authority = test_authority();

        // Add jurisdiction-scoped revocation
        let record = RevocationRecord::immediate(
            RevocationType::Membership,
            "member456".to_string(),
            RevocationScope::Jurisdiction {
                jurisdiction_id: JurisdictionId::coop("test-coop"),
            },
            authority,
            CommonsRevocationReason::PolicyViolation {
                policy_ref: "POL-001".to_string(),
                details: "Local violation".to_string(),
            },
        );
        registry.add(record).unwrap();

        // Revoked in test-coop
        let check = registry.check_at_scope(
            "member456",
            &RevocationScope::Jurisdiction {
                jurisdiction_id: JurisdictionId::coop("test-coop"),
            },
        );
        assert!(check.is_revoked);

        // Not revoked in other-coop
        let check = registry.check_at_scope(
            "member456",
            &RevocationScope::Jurisdiction {
                jurisdiction_id: JurisdictionId::coop("other-coop"),
            },
        );
        assert!(!check.is_revoked);
    }

    #[test]
    fn test_global_scope_applies_everywhere() {
        let registry = RevocationRegistry::new();
        let authority = test_authority();

        // Add global revocation
        let record = RevocationRecord::immediate(
            RevocationType::Anchor,
            "anchor789".to_string(),
            RevocationScope::Global,
            authority,
            CommonsRevocationReason::SybilAttack {
                evidence_summary: "Global Sybil".to_string(),
            },
        );
        registry.add(record).unwrap();

        // Revoked globally
        let check = registry.check_at_scope("anchor789", &RevocationScope::Global);
        assert!(check.is_revoked);

        // Also revoked in any jurisdiction (global applies everywhere)
        let check = registry.check_at_scope(
            "anchor789",
            &RevocationScope::Jurisdiction {
                jurisdiction_id: JurisdictionId::coop("any-coop"),
            },
        );
        assert!(check.is_revoked);
    }

    #[test]
    fn test_list_for_target() {
        let registry = RevocationRegistry::new();
        let authority = test_authority();

        // Add multiple revocations for same target
        let record1 = RevocationRecord::immediate(
            RevocationType::Membership,
            "member-multi".to_string(),
            RevocationScope::Jurisdiction {
                jurisdiction_id: JurisdictionId::coop("coop1"),
            },
            authority.clone(),
            CommonsRevocationReason::PolicyViolation {
                policy_ref: "POL-001".to_string(),
                details: "First".to_string(),
            },
        );

        let record2 = RevocationRecord::immediate(
            RevocationType::Membership,
            "member-multi".to_string(),
            RevocationScope::Jurisdiction {
                jurisdiction_id: JurisdictionId::coop("coop2"),
            },
            authority,
            CommonsRevocationReason::PolicyViolation {
                policy_ref: "POL-002".to_string(),
                details: "Second".to_string(),
            },
        );

        registry.add(record1).unwrap();
        registry.add(record2).unwrap();

        let revocations = registry.list_for_target("member-multi");
        assert_eq!(revocations.len(), 2);
    }

    #[test]
    fn test_list_by_type() {
        let registry = RevocationRegistry::new();
        let authority = test_authority();

        registry
            .add(RevocationRecord::immediate(
                RevocationType::Anchor,
                "anchor1".to_string(),
                RevocationScope::Global,
                authority.clone(),
                CommonsRevocationReason::SybilAttack {
                    evidence_summary: "Sybil".to_string(),
                },
            ))
            .unwrap();

        registry
            .add(RevocationRecord::immediate(
                RevocationType::Membership,
                "member1".to_string(),
                RevocationScope::Global,
                authority.clone(),
                CommonsRevocationReason::PolicyViolation {
                    policy_ref: "POL".to_string(),
                    details: "Details".to_string(),
                },
            ))
            .unwrap();

        registry
            .add(RevocationRecord::immediate(
                RevocationType::Anchor,
                "anchor2".to_string(),
                RevocationScope::Global,
                authority,
                CommonsRevocationReason::IdentityFraud {
                    details: "Fraud".to_string(),
                },
            ))
            .unwrap();

        let anchors = registry.list_by_type(RevocationType::Anchor);
        assert_eq!(anchors.len(), 2);

        let memberships = registry.list_by_type(RevocationType::Membership);
        assert_eq!(memberships.len(), 1);
    }

    #[test]
    fn test_pending_appeals() {
        let registry = RevocationRegistry::new();
        let authority = test_authority();

        let mut record = RevocationRecord::new(
            RevocationType::Holder,
            "holder-appeal".to_string(),
            RevocationScope::Global,
            authority,
            CommonsRevocationReason::IdentityFraud {
                details: "Suspected".to_string(),
            },
            30,
        );

        record.file_appeal("I'm innocent".to_string());
        registry.add(record).unwrap();

        let pending = registry.list_pending_appeals();
        assert_eq!(pending.len(), 1);
    }

    #[test]
    fn test_count() {
        let registry = RevocationRegistry::new();
        let authority = test_authority();

        assert_eq!(registry.count(), 0);
        assert_eq!(registry.count_effective(), 0);

        // Add immediate (effective)
        registry
            .add(RevocationRecord::immediate(
                RevocationType::Credential,
                "cred1".to_string(),
                RevocationScope::Global,
                authority.clone(),
                CommonsRevocationReason::KeyCompromise,
            ))
            .unwrap();

        // Add with appeal window (not effective yet)
        registry
            .add(RevocationRecord::new(
                RevocationType::Credential,
                "cred2".to_string(),
                RevocationScope::Global,
                authority,
                CommonsRevocationReason::KeyCompromise,
                30,
            ))
            .unwrap();

        assert_eq!(registry.count(), 2);
        assert_eq!(registry.count_effective(), 1);
    }

    #[test]
    fn test_is_revoked_in_jurisdiction() {
        let registry = RevocationRegistry::new();
        let authority = test_authority();

        let coop_id = JurisdictionId::coop("my-coop");

        registry
            .add(RevocationRecord::immediate(
                RevocationType::Membership,
                "member-jur".to_string(),
                RevocationScope::Jurisdiction {
                    jurisdiction_id: coop_id.clone(),
                },
                authority,
                CommonsRevocationReason::PolicyViolation {
                    policy_ref: "POL".to_string(),
                    details: "Details".to_string(),
                },
            ))
            .unwrap();

        assert!(registry.is_revoked_in_jurisdiction("member-jur", &coop_id));
        assert!(!registry.is_revoked_in_jurisdiction(
            "member-jur",
            &JurisdictionId::coop("other-coop")
        ));
    }
}
