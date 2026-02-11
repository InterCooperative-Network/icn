//! Membership service adapter for kernel-safe membership effects.
//!
//! This module implements the `MembershipService` trait from `icn-kernel-api`,
//! bridging the kernel-safe effect boundary to the actual icn-coop CoopStore.
//!
//! # Architecture
//!
//! ```text
//! [Kernel Effect]                [This Adapter]              [icn-coop]
//! MembershipEffect::AddMember --> MembershipServiceImpl --> CoopStore::save_member
//! MembershipEffect::Freeze    --> MembershipServiceImpl --> CoopStore::save_member
//! ```
//!
//! # Provenance Tracking
//!
//! All membership operations carry `decision_receipt_id` and `decision_hash`
//! to maintain the audit chain from governance decisions to state changes.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use anyhow::Result;
use icn_coop::{CoopStore, Member, MemberRole, MemberStatus, MembershipTier};
use icn_identity::Did;
use icn_kernel_api::{
    AddMemberRequest, AddMemberResult, FreezeMemberRequest, FreezeMemberResult, MembershipService,
    RemoveMemberRequest, RemoveMemberResult, UnfreezeMemberRequest, UnfreezeMemberResult,
    UpdateMemberRequest, UpdateMemberResult,
};
use sha2::{Digest, Sha256};
use tracing::{info, warn};

/// Provenance record for a membership operation.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MembershipProvenance {
    /// Decision receipt ID from governance
    pub decision_receipt_id: String,
    /// Decision hash for verification
    pub decision_hash: String,
    /// Operation type: "add", "remove", "update", "freeze", "unfreeze"
    pub operation_type: String,
    /// When the operation occurred
    pub timestamp: u64,
}

/// Adapter implementing `MembershipService` using `CoopStore`.
///
/// This bridges kernel-safe membership effects to the actual cooperative
/// storage, tracking provenance for audit purposes.
pub struct MembershipServiceImpl {
    /// The underlying cooperative store
    store: Arc<CoopStore>,

    /// Provenance tracking for membership operations
    /// Key format: "{entity_id}:{member_did}"
    provenance: RwLock<HashMap<String, MembershipProvenance>>,
}

impl MembershipServiceImpl {
    /// Create a new MembershipServiceImpl wrapping the given store.
    pub fn new(store: Arc<CoopStore>) -> Self {
        Self {
            store,
            provenance: RwLock::new(HashMap::new()),
        }
    }

    /// Build a provenance key from entity_id and member_did.
    fn provenance_key(entity_id: &str, member_did: &str) -> String {
        format!("{}:{}", entity_id, member_did)
    }

    /// Compute a state change hash for an operation.
    fn compute_state_change_hash(
        operation: &str,
        entity_id: &str,
        member_did: &str,
        decision_receipt_id: &str,
        timestamp: u64,
    ) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"membership:");
        hasher.update(operation.as_bytes());
        hasher.update(b":");
        hasher.update(entity_id.as_bytes());
        hasher.update(b":");
        hasher.update(member_did.as_bytes());
        hasher.update(b":");
        hasher.update(decision_receipt_id.as_bytes());
        hasher.update(b":");
        hasher.update(timestamp.to_le_bytes());
        let hash = hasher.finalize();
        hex::encode(&hash[..20]) // First 20 bytes for reasonable length
    }

    /// Store provenance for an operation.
    fn store_provenance(&self, key: String, provenance: MembershipProvenance) {
        if let Ok(mut map) = self.provenance.write() {
            map.insert(key, provenance);
        }
    }

    /// Parse a role string to MemberRole.
    fn parse_role(role: &str) -> MemberRole {
        match role.to_lowercase().as_str() {
            "founder" => MemberRole::Founder,
            "worker" => MemberRole::Worker,
            "consumer" => MemberRole::Consumer,
            "producer" => MemberRole::Producer,
            "boardmember" | "board_member" | "board" => MemberRole::BoardMember,
            "officer" => MemberRole::Officer,
            _ => MemberRole::Member,
        }
    }

    /// Parse a DID string.
    fn parse_did(did_str: &str) -> Result<Did, String> {
        did_str.parse().map_err(|e| format!("Invalid DID: {}", e))
    }
}

impl MembershipService for MembershipServiceImpl {
    fn add_member(&self, request: AddMemberRequest) -> Result<AddMemberResult> {
        info!(
            entity_id = %request.entity_id,
            member_did = %request.member_did,
            role = %request.role,
            tier = %request.tier,
            decision_receipt_id = %request.decision_receipt_id,
            "Processing add member request"
        );

        let timestamp = icn_time::current_timestamp_secs();

        // Parse the member DID
        let did = match Self::parse_did(&request.member_did) {
            Ok(d) => d,
            Err(e) => {
                return Ok(AddMemberResult {
                    success: false,
                    state_change_hash: String::new(),
                    error: Some(e),
                });
            }
        };

        // Create member with role and tier
        let role = Self::parse_role(&request.role);
        let mut member = Member::new(did, request.entity_id.clone(), role);

        // Set tier if specified
        if !request.tier.is_empty() {
            let tier = MembershipTier::standard(&request.tier);
            member = member.with_tier(tier);
        }

        // Start as Active (governance already approved via proposal)
        member.status = MemberStatus::Active;

        // Store the member
        match self.store.save_member(&member) {
            Ok(()) => {
                let state_change_hash = Self::compute_state_change_hash(
                    "add",
                    &request.entity_id,
                    &request.member_did,
                    &request.decision_receipt_id,
                    timestamp,
                );

                // Store provenance
                let key = Self::provenance_key(&request.entity_id, &request.member_did);
                self.store_provenance(
                    key,
                    MembershipProvenance {
                        decision_receipt_id: request.decision_receipt_id.clone(),
                        decision_hash: request.decision_hash.clone(),
                        operation_type: "add".to_string(),
                        timestamp,
                    },
                );

                info!(
                    entity_id = %request.entity_id,
                    member_did = %request.member_did,
                    state_change_hash = %state_change_hash,
                    "Member added successfully"
                );

                Ok(AddMemberResult {
                    success: true,
                    state_change_hash,
                    error: None,
                })
            }
            Err(e) => {
                warn!(
                    entity_id = %request.entity_id,
                    member_did = %request.member_did,
                    error = %e,
                    "Failed to add member"
                );
                Ok(AddMemberResult {
                    success: false,
                    state_change_hash: String::new(),
                    error: Some(e.to_string()),
                })
            }
        }
    }

    fn remove_member(&self, request: RemoveMemberRequest) -> Result<RemoveMemberResult> {
        info!(
            entity_id = %request.entity_id,
            member_did = %request.member_did,
            reason = %request.reason,
            decision_receipt_id = %request.decision_receipt_id,
            "Processing remove member request"
        );

        let timestamp = icn_time::current_timestamp_secs();

        // Parse the member DID
        let did = match Self::parse_did(&request.member_did) {
            Ok(d) => d,
            Err(e) => {
                return Ok(RemoveMemberResult {
                    success: false,
                    state_change_hash: String::new(),
                    error: Some(e),
                });
            }
        };

        // Get existing member
        match self.store.get_member(&request.entity_id, &did) {
            Ok(mut member) => {
                // Mark as removed
                member.status = MemberStatus::Removed;
                member
                    .metadata
                    .insert("removal_reason".to_string(), request.reason.clone());
                member
                    .metadata
                    .insert("removed_at".to_string(), timestamp.to_string());

                // Save updated member
                match self.store.save_member(&member) {
                    Ok(()) => {
                        let state_change_hash = Self::compute_state_change_hash(
                            "remove",
                            &request.entity_id,
                            &request.member_did,
                            &request.decision_receipt_id,
                            timestamp,
                        );

                        // Store provenance
                        let key = Self::provenance_key(&request.entity_id, &request.member_did);
                        self.store_provenance(
                            key,
                            MembershipProvenance {
                                decision_receipt_id: request.decision_receipt_id.clone(),
                                decision_hash: request.decision_hash.clone(),
                                operation_type: "remove".to_string(),
                                timestamp,
                            },
                        );

                        info!(
                            entity_id = %request.entity_id,
                            member_did = %request.member_did,
                            state_change_hash = %state_change_hash,
                            "Member removed successfully"
                        );

                        Ok(RemoveMemberResult {
                            success: true,
                            state_change_hash,
                            error: None,
                        })
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to save removed member");
                        Ok(RemoveMemberResult {
                            success: false,
                            state_change_hash: String::new(),
                            error: Some(e.to_string()),
                        })
                    }
                }
            }
            Err(e) => {
                warn!(
                    entity_id = %request.entity_id,
                    member_did = %request.member_did,
                    error = %e,
                    "Member not found for removal"
                );
                Ok(RemoveMemberResult {
                    success: false,
                    state_change_hash: String::new(),
                    error: Some(format!("Member not found: {}", e)),
                })
            }
        }
    }

    fn update_member(&self, request: UpdateMemberRequest) -> Result<UpdateMemberResult> {
        info!(
            entity_id = %request.entity_id,
            member_did = %request.member_did,
            new_role = ?request.new_role,
            new_tier = ?request.new_tier,
            decision_receipt_id = %request.decision_receipt_id,
            "Processing update member request"
        );

        let timestamp = icn_time::current_timestamp_secs();

        // Parse the member DID
        let did = match Self::parse_did(&request.member_did) {
            Ok(d) => d,
            Err(e) => {
                return Ok(UpdateMemberResult {
                    success: false,
                    state_change_hash: String::new(),
                    error: Some(e),
                });
            }
        };

        // Get existing member
        match self.store.get_member(&request.entity_id, &did) {
            Ok(mut member) => {
                // Update role if specified
                if let Some(new_role) = &request.new_role {
                    member.role = Self::parse_role(new_role);
                }

                // Update tier if specified
                if let Some(new_tier) = &request.new_tier {
                    member.tier = Some(MembershipTier::standard(new_tier));
                }

                // Save updated member
                match self.store.save_member(&member) {
                    Ok(()) => {
                        let state_change_hash = Self::compute_state_change_hash(
                            "update",
                            &request.entity_id,
                            &request.member_did,
                            &request.decision_receipt_id,
                            timestamp,
                        );

                        // Store provenance
                        let key = Self::provenance_key(&request.entity_id, &request.member_did);
                        self.store_provenance(
                            key,
                            MembershipProvenance {
                                decision_receipt_id: request.decision_receipt_id.clone(),
                                decision_hash: request.decision_hash.clone(),
                                operation_type: "update".to_string(),
                                timestamp,
                            },
                        );

                        info!(
                            entity_id = %request.entity_id,
                            member_did = %request.member_did,
                            state_change_hash = %state_change_hash,
                            "Member updated successfully"
                        );

                        Ok(UpdateMemberResult {
                            success: true,
                            state_change_hash,
                            error: None,
                        })
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to save updated member");
                        Ok(UpdateMemberResult {
                            success: false,
                            state_change_hash: String::new(),
                            error: Some(e.to_string()),
                        })
                    }
                }
            }
            Err(e) => {
                warn!(
                    entity_id = %request.entity_id,
                    member_did = %request.member_did,
                    error = %e,
                    "Member not found for update"
                );
                Ok(UpdateMemberResult {
                    success: false,
                    state_change_hash: String::new(),
                    error: Some(format!("Member not found: {}", e)),
                })
            }
        }
    }

    fn freeze_member(&self, request: FreezeMemberRequest) -> Result<FreezeMemberResult> {
        info!(
            entity_id = %request.entity_id,
            member_did = %request.member_did,
            reason = %request.reason,
            duration_secs = ?request.duration_secs,
            decision_receipt_id = %request.decision_receipt_id,
            "Processing freeze member request"
        );

        let timestamp = icn_time::current_timestamp_secs();

        // Parse the member DID
        let did = match Self::parse_did(&request.member_did) {
            Ok(d) => d,
            Err(e) => {
                return Ok(FreezeMemberResult {
                    success: false,
                    state_change_hash: String::new(),
                    expires_at: None,
                    error: Some(e),
                });
            }
        };

        // Get existing member
        match self.store.get_member(&request.entity_id, &did) {
            Ok(mut member) => {
                // Check if member can be frozen (must be Active)
                if member.status != MemberStatus::Active {
                    return Ok(FreezeMemberResult {
                        success: false,
                        state_change_hash: String::new(),
                        expires_at: None,
                        error: Some(format!(
                            "Cannot freeze member with status {:?}",
                            member.status
                        )),
                    });
                }

                // Freeze the member
                member.status = MemberStatus::Suspended;
                member
                    .metadata
                    .insert("freeze_reason".to_string(), request.reason.clone());
                member
                    .metadata
                    .insert("frozen_at".to_string(), timestamp.to_string());

                // Calculate expiration if duration specified
                let expires_at = request.duration_secs.map(|d| timestamp + d);
                if let Some(exp) = expires_at {
                    member
                        .metadata
                        .insert("freeze_expires_at".to_string(), exp.to_string());
                }

                // Save updated member
                match self.store.save_member(&member) {
                    Ok(()) => {
                        let state_change_hash = Self::compute_state_change_hash(
                            "freeze",
                            &request.entity_id,
                            &request.member_did,
                            &request.decision_receipt_id,
                            timestamp,
                        );

                        // Store provenance
                        let key = Self::provenance_key(&request.entity_id, &request.member_did);
                        self.store_provenance(
                            key,
                            MembershipProvenance {
                                decision_receipt_id: request.decision_receipt_id.clone(),
                                decision_hash: request.decision_hash.clone(),
                                operation_type: "freeze".to_string(),
                                timestamp,
                            },
                        );

                        info!(
                            entity_id = %request.entity_id,
                            member_did = %request.member_did,
                            state_change_hash = %state_change_hash,
                            expires_at = ?expires_at,
                            "Member frozen successfully"
                        );

                        Ok(FreezeMemberResult {
                            success: true,
                            state_change_hash,
                            expires_at,
                            error: None,
                        })
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to save frozen member");
                        Ok(FreezeMemberResult {
                            success: false,
                            state_change_hash: String::new(),
                            expires_at: None,
                            error: Some(e.to_string()),
                        })
                    }
                }
            }
            Err(e) => {
                warn!(
                    entity_id = %request.entity_id,
                    member_did = %request.member_did,
                    error = %e,
                    "Member not found for freeze"
                );
                Ok(FreezeMemberResult {
                    success: false,
                    state_change_hash: String::new(),
                    expires_at: None,
                    error: Some(format!("Member not found: {}", e)),
                })
            }
        }
    }

    fn unfreeze_member(&self, request: UnfreezeMemberRequest) -> Result<UnfreezeMemberResult> {
        info!(
            entity_id = %request.entity_id,
            member_did = %request.member_did,
            decision_receipt_id = %request.decision_receipt_id,
            "Processing unfreeze member request"
        );

        let timestamp = icn_time::current_timestamp_secs();

        // Parse the member DID
        let did = match Self::parse_did(&request.member_did) {
            Ok(d) => d,
            Err(e) => {
                return Ok(UnfreezeMemberResult {
                    success: false,
                    state_change_hash: String::new(),
                    error: Some(e),
                });
            }
        };

        // Get existing member
        match self.store.get_member(&request.entity_id, &did) {
            Ok(mut member) => {
                // Check if member is frozen
                if member.status != MemberStatus::Suspended {
                    return Ok(UnfreezeMemberResult {
                        success: false,
                        state_change_hash: String::new(),
                        error: Some(format!(
                            "Cannot unfreeze member with status {:?}",
                            member.status
                        )),
                    });
                }

                // Unfreeze the member
                member.status = MemberStatus::Active;
                member
                    .metadata
                    .insert("unfrozen_at".to_string(), timestamp.to_string());
                member.metadata.remove("freeze_expires_at");

                // Save updated member
                match self.store.save_member(&member) {
                    Ok(()) => {
                        let state_change_hash = Self::compute_state_change_hash(
                            "unfreeze",
                            &request.entity_id,
                            &request.member_did,
                            &request.decision_receipt_id,
                            timestamp,
                        );

                        // Store provenance
                        let key = Self::provenance_key(&request.entity_id, &request.member_did);
                        self.store_provenance(
                            key,
                            MembershipProvenance {
                                decision_receipt_id: request.decision_receipt_id.clone(),
                                decision_hash: request.decision_hash.clone(),
                                operation_type: "unfreeze".to_string(),
                                timestamp,
                            },
                        );

                        info!(
                            entity_id = %request.entity_id,
                            member_did = %request.member_did,
                            state_change_hash = %state_change_hash,
                            "Member unfrozen successfully"
                        );

                        Ok(UnfreezeMemberResult {
                            success: true,
                            state_change_hash,
                            error: None,
                        })
                    }
                    Err(e) => {
                        warn!(error = %e, "Failed to save unfrozen member");
                        Ok(UnfreezeMemberResult {
                            success: false,
                            state_change_hash: String::new(),
                            error: Some(e.to_string()),
                        })
                    }
                }
            }
            Err(e) => {
                warn!(
                    entity_id = %request.entity_id,
                    member_did = %request.member_did,
                    error = %e,
                    "Member not found for unfreeze"
                );
                Ok(UnfreezeMemberResult {
                    success: false,
                    state_change_hash: String::new(),
                    error: Some(format!("Member not found: {}", e)),
                })
            }
        }
    }

    fn is_member(&self, entity_id: &str, member_did: &str) -> bool {
        let did = match Self::parse_did(member_did) {
            Ok(d) => d,
            Err(_) => return false,
        };

        self.store.get_member(entity_id, &did).is_ok()
    }

    fn is_active_member(&self, entity_id: &str, member_did: &str) -> bool {
        let did = match Self::parse_did(member_did) {
            Ok(d) => d,
            Err(_) => return false,
        };

        match self.store.get_member(entity_id, &did) {
            Ok(member) => member.status == MemberStatus::Active,
            Err(_) => false,
        }
    }

    fn get_membership_provenance(
        &self,
        entity_id: &str,
        member_did: &str,
    ) -> Option<(String, String)> {
        let key = Self::provenance_key(entity_id, member_did);
        let prov = self.provenance.read().ok()?;
        prov.get(&key)
            .map(|p| (p.decision_receipt_id.clone(), p.decision_hash.clone()))
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::KeyPair;
    use std::path::Path;
    use tempfile::TempDir;

    fn make_test_did() -> Did {
        KeyPair::generate()
            .expect("keypair generation failed")
            .did()
            .clone()
    }

    fn make_store_at_path(path: &Path) -> Arc<CoopStore> {
        let db = sled::open(path).expect("Failed to open sled db");
        Arc::new(CoopStore::new(Arc::new(db)))
    }

    fn make_test_store() -> (Arc<CoopStore>, TempDir) {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let store = make_store_at_path(temp_dir.path());
        (store, temp_dir)
    }

    #[test]
    fn test_add_member_creates_durable_record() {
        let (store, _temp) = make_test_store();
        let service = MembershipServiceImpl::new(store.clone());

        let test_did = make_test_did();
        let request = AddMemberRequest {
            entity_id: "coop:test-coop".to_string(),
            member_did: test_did.to_string(),
            role: "Worker".to_string(),
            tier: "Standard".to_string(),
            decision_receipt_id: "gov:proposal:add-member:receipt:test-123".to_string(),
            decision_hash: "sha256:addtest12345".to_string(),
        };

        let result = service
            .add_member(request.clone())
            .expect("add_member failed");

        // Verify success
        assert!(result.success, "Add should succeed");
        assert!(
            !result.state_change_hash.is_empty(),
            "Should have state change hash"
        );
        assert!(result.error.is_none(), "Should have no error");

        // Verify durable record exists
        let stored = store
            .get_member("coop:test-coop", &test_did)
            .expect("get_member failed");
        assert_eq!(stored.role, MemberRole::Worker);
        assert_eq!(stored.status, MemberStatus::Active);

        // Verify provenance tracking
        let prov = service.get_membership_provenance(&request.entity_id, &request.member_did);
        assert!(prov.is_some(), "Provenance should be tracked");
        let (receipt_id, hash) = prov.unwrap();
        assert_eq!(receipt_id, request.decision_receipt_id);
        assert_eq!(hash, request.decision_hash);
    }

    /// Two-phase durability test: write, close, reopen, verify record survives
    #[test]
    fn test_add_member_survives_reload() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let store_path = temp_dir.path().to_path_buf();

        let test_did = make_test_did();
        let entity_id = "coop:reload-test".to_string();
        let member_did_str = test_did.to_string();
        let state_change_hash;

        // === PHASE A: Write ===
        {
            let store = make_store_at_path(&store_path);
            let service = MembershipServiceImpl::new(store);

            let request = AddMemberRequest {
                entity_id: entity_id.clone(),
                member_did: member_did_str.clone(),
                role: "BoardMember".to_string(),
                tier: "Founder".to_string(),
                decision_receipt_id: "gov:proposal:add:receipt:reload-123".to_string(),
                decision_hash: "sha256:reload-test-hash".to_string(),
            };

            let result = service.add_member(request).expect("add_member failed");
            assert!(result.success, "Add should succeed");
            state_change_hash = result.state_change_hash;
            assert!(
                !state_change_hash.is_empty(),
                "Should have state change hash"
            );

            // Store drops here, simulating process boundary
        }

        // === PHASE B: Reload and verify ===
        {
            let store_b = make_store_at_path(&store_path);

            // Verify record exists after reload
            let stored = store_b
                .get_member(&entity_id, &test_did)
                .expect("get_member failed");
            assert_eq!(stored.role, MemberRole::BoardMember, "Role should match");
            assert_eq!(
                stored.status,
                MemberStatus::Active,
                "Status should be active"
            );

            println!();
            println!("╔══════════════════════════════════════════════════════════════════╗");
            println!("║          MEMBERSHIP ADD RELOAD DURABILITY VERIFIED               ║");
            println!("╠══════════════════════════════════════════════════════════════════╣");
            println!(
                "║ state_change_hash: {:<43} ║",
                &state_change_hash[..43.min(state_change_hash.len())]
            );
            println!("║ entity_id:         {:<43} ║", &entity_id);
            println!(
                "║ member_role:       {:<43} ║",
                format!("{:?}", stored.role)
            );
            println!("╚══════════════════════════════════════════════════════════════════╝");
            println!();
        }
    }

    #[test]
    fn test_freeze_member_survives_reload() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let store_path = temp_dir.path().to_path_buf();

        let test_did = make_test_did();
        let entity_id = "coop:freeze-reload-test".to_string();
        let member_did_str = test_did.to_string();

        // === PHASE A: Add member then freeze ===
        {
            let store = make_store_at_path(&store_path);
            let service = MembershipServiceImpl::new(store);

            // First add the member
            let add_req = AddMemberRequest {
                entity_id: entity_id.clone(),
                member_did: member_did_str.clone(),
                role: "Worker".to_string(),
                tier: "Standard".to_string(),
                decision_receipt_id: "gov:add:receipt:1".to_string(),
                decision_hash: "sha256:add1".to_string(),
            };
            let add_result = service.add_member(add_req).expect("add failed");
            assert!(add_result.success, "Add should succeed");

            // Then freeze them
            let freeze_req = FreezeMemberRequest {
                entity_id: entity_id.clone(),
                member_did: member_did_str.clone(),
                reason: "Policy violation".to_string(),
                duration_secs: Some(86400),
                decision_receipt_id: "gov:freeze:receipt:1".to_string(),
                decision_hash: "sha256:freeze1".to_string(),
            };
            let freeze_result = service.freeze_member(freeze_req).expect("freeze failed");
            assert!(freeze_result.success, "Freeze should succeed");
            assert!(freeze_result.expires_at.is_some(), "Should have expiration");
        }

        // === PHASE B: Reload and verify freeze persisted ===
        {
            let store_b = make_store_at_path(&store_path);

            let stored = store_b
                .get_member(&entity_id, &test_did)
                .expect("get_member failed");
            assert_eq!(
                stored.status,
                MemberStatus::Suspended,
                "Status should be suspended"
            );
            assert!(
                stored.metadata.contains_key("freeze_reason"),
                "Should have freeze reason"
            );
            assert!(
                stored.metadata.contains_key("freeze_expires_at"),
                "Should have expiration"
            );

            println!();
            println!("╔══════════════════════════════════════════════════════════════════╗");
            println!("║          MEMBERSHIP FREEZE RELOAD DURABILITY VERIFIED            ║");
            println!("╠══════════════════════════════════════════════════════════════════╣");
            println!("║ entity_id:     {:<48} ║", &entity_id);
            println!("║ status:        {:<48} ║", format!("{:?}", stored.status));
            println!(
                "║ freeze_reason: {:<48} ║",
                stored
                    .metadata
                    .get("freeze_reason")
                    .unwrap_or(&"".to_string())
            );
            println!("╚══════════════════════════════════════════════════════════════════╝");
            println!();
        }
    }

    #[test]
    fn test_remove_member_survives_reload() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let store_path = temp_dir.path().to_path_buf();

        let test_did = make_test_did();
        let entity_id = "coop:remove-reload-test".to_string();
        let member_did_str = test_did.to_string();

        // === PHASE A: Add member then remove ===
        {
            let store = make_store_at_path(&store_path);
            let service = MembershipServiceImpl::new(store);

            // First add the member
            let add_req = AddMemberRequest {
                entity_id: entity_id.clone(),
                member_did: member_did_str.clone(),
                role: "Member".to_string(),
                tier: "Standard".to_string(),
                decision_receipt_id: "gov:add:receipt:2".to_string(),
                decision_hash: "sha256:add2".to_string(),
            };
            service.add_member(add_req).expect("add failed");

            // Then remove them
            let remove_req = RemoveMemberRequest {
                entity_id: entity_id.clone(),
                member_did: member_did_str.clone(),
                reason: "Voluntary resignation".to_string(),
                decision_receipt_id: "gov:remove:receipt:2".to_string(),
                decision_hash: "sha256:remove2".to_string(),
            };
            let remove_result = service.remove_member(remove_req).expect("remove failed");
            assert!(remove_result.success, "Remove should succeed");
        }

        // === PHASE B: Reload and verify removal persisted ===
        {
            let store_b = make_store_at_path(&store_path);

            let stored = store_b
                .get_member(&entity_id, &test_did)
                .expect("get_member failed");
            assert_eq!(
                stored.status,
                MemberStatus::Removed,
                "Status should be removed"
            );
            assert!(
                stored.metadata.contains_key("removal_reason"),
                "Should have removal reason"
            );

            println!();
            println!("╔══════════════════════════════════════════════════════════════════╗");
            println!("║          MEMBERSHIP REMOVE RELOAD DURABILITY VERIFIED            ║");
            println!("╠══════════════════════════════════════════════════════════════════╣");
            println!("║ entity_id:      {:<47} ║", &entity_id);
            println!("║ status:         {:<47} ║", format!("{:?}", stored.status));
            println!(
                "║ removal_reason: {:<47} ║",
                stored
                    .metadata
                    .get("removal_reason")
                    .unwrap_or(&"".to_string())
            );
            println!("╚══════════════════════════════════════════════════════════════════╝");
            println!();
        }
    }

    #[test]
    fn test_update_member_role_survives_reload() {
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let store_path = temp_dir.path().to_path_buf();

        let test_did = make_test_did();
        let entity_id = "coop:update-reload-test".to_string();
        let member_did_str = test_did.to_string();

        // === PHASE A: Add member then update role ===
        {
            let store = make_store_at_path(&store_path);
            let service = MembershipServiceImpl::new(store);

            // First add as Member
            let add_req = AddMemberRequest {
                entity_id: entity_id.clone(),
                member_did: member_did_str.clone(),
                role: "Member".to_string(),
                tier: "Standard".to_string(),
                decision_receipt_id: "gov:add:receipt:3".to_string(),
                decision_hash: "sha256:add3".to_string(),
            };
            service.add_member(add_req).expect("add failed");

            // Then promote to BoardMember
            let update_req = UpdateMemberRequest {
                entity_id: entity_id.clone(),
                member_did: member_did_str.clone(),
                new_role: Some("BoardMember".to_string()),
                new_tier: Some("Senior".to_string()),
                decision_receipt_id: "gov:update:receipt:3".to_string(),
                decision_hash: "sha256:update3".to_string(),
            };
            let update_result = service.update_member(update_req).expect("update failed");
            assert!(update_result.success, "Update should succeed");
        }

        // === PHASE B: Reload and verify update persisted ===
        {
            let store_b = make_store_at_path(&store_path);

            let stored = store_b
                .get_member(&entity_id, &test_did)
                .expect("get_member failed");
            assert_eq!(
                stored.role,
                MemberRole::BoardMember,
                "Role should be BoardMember"
            );
            assert!(stored.tier.is_some(), "Should have tier");
            assert_eq!(
                stored.tier.as_ref().unwrap().name,
                "Senior",
                "Tier name should be Senior"
            );

            println!();
            println!("╔══════════════════════════════════════════════════════════════════╗");
            println!("║          MEMBERSHIP UPDATE RELOAD DURABILITY VERIFIED            ║");
            println!("╠══════════════════════════════════════════════════════════════════╣");
            println!("║ entity_id: {:<52} ║", &entity_id);
            println!("║ new_role:  {:<52} ║", format!("{:?}", stored.role));
            println!(
                "║ new_tier:  {:<52} ║",
                stored
                    .tier
                    .as_ref()
                    .map(|t| t.name.as_str())
                    .unwrap_or("N/A")
            );
            println!("╚══════════════════════════════════════════════════════════════════╝");
            println!();
        }
    }

    #[test]
    fn test_is_member_and_is_active_member() {
        let (store, _temp) = make_test_store();
        let service = MembershipServiceImpl::new(store);

        let test_did = make_test_did();
        let entity_id = "coop:status-test";
        let member_did_str = test_did.to_string();

        // Before adding, should not be member
        assert!(!service.is_member(entity_id, &member_did_str));
        assert!(!service.is_active_member(entity_id, &member_did_str));

        // Add member
        let add_req = AddMemberRequest {
            entity_id: entity_id.to_string(),
            member_did: member_did_str.clone(),
            role: "Worker".to_string(),
            tier: "Standard".to_string(),
            decision_receipt_id: "gov:add:status".to_string(),
            decision_hash: "sha256:status".to_string(),
        };
        service.add_member(add_req).expect("add failed");

        // Now should be member and active
        assert!(service.is_member(entity_id, &member_did_str));
        assert!(service.is_active_member(entity_id, &member_did_str));

        // Freeze member
        let freeze_req = FreezeMemberRequest {
            entity_id: entity_id.to_string(),
            member_did: member_did_str.clone(),
            reason: "test".to_string(),
            duration_secs: None,
            decision_receipt_id: "gov:freeze:status".to_string(),
            decision_hash: "sha256:freezestatus".to_string(),
        };
        service.freeze_member(freeze_req).expect("freeze failed");

        // Still a member but not active
        assert!(service.is_member(entity_id, &member_did_str));
        assert!(!service.is_active_member(entity_id, &member_did_str));
    }

    #[test]
    fn test_compute_state_change_hash_deterministic() {
        let hash1 = MembershipServiceImpl::compute_state_change_hash(
            "add",
            "entity-1",
            "did:icn:member1",
            "receipt-123",
            1700000000,
        );
        let hash2 = MembershipServiceImpl::compute_state_change_hash(
            "add",
            "entity-1",
            "did:icn:member1",
            "receipt-123",
            1700000000,
        );
        assert_eq!(hash1, hash2, "Same inputs should produce same hash");
    }

    #[test]
    fn test_compute_state_change_hash_varies_by_operation() {
        let hash1 = MembershipServiceImpl::compute_state_change_hash(
            "add",
            "entity-1",
            "did:icn:member1",
            "receipt-123",
            1700000000,
        );
        let hash2 = MembershipServiceImpl::compute_state_change_hash(
            "remove",
            "entity-1",
            "did:icn:member1",
            "receipt-123",
            1700000000,
        );
        assert_ne!(
            hash1, hash2,
            "Different operations should produce different hashes"
        );
    }
}
