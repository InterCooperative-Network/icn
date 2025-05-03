#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GovernanceKernel, GovernanceError, RoleAssignmentOptions};
    use icn_identity::{IdentityId, IdentityScope};
    use icn_core_vm::IdentityContext;
    use icn_storage::memory::MemoryStorage;
    use std::sync::Arc;
    use tokio::sync::Mutex;
    use crate::config::{GovernanceConfig, GovernanceStructure, Role};

    // Helper function to create a test kernel with memory storage
    async fn create_test_kernel() -> GovernanceKernel<MemoryStorage> {
        // Create a memory storage backend
        let storage = Arc::new(Mutex::new(MemoryStorage::new()));
        
        // Create a test identity context
        let test_did = "did:icn:test:kernel";
        let identity = Arc::new(IdentityContext::new_with_did(test_did));
        
        // Create a governance kernel
        let kernel = GovernanceKernel::new(storage, identity);
        
        // Create and store a test governance config with roles
        let config = GovernanceConfig {
            template_type: "test_template".to_string(),
            template_version: "v1".to_string(),
            governing_scope: IdentityScope::Cooperative,
            identity: None,
            governance: Some(GovernanceStructure {
                decision_making: Some("majority".to_string()),
                quorum: Some(0.51),
                majority: Some(0.67),
                term_length: Some(365),
                roles: Some(vec![
                    Role {
                        name: "admin".to_string(),
                        permissions: vec![
                            "create_proposals".to_string(),
                            "vote_on_proposals".to_string(),
                            "execute_proposals".to_string(),
                            "assign_roles".to_string(),
                        ],
                    },
                    Role {
                        name: "member".to_string(),
                        permissions: vec![
                            "create_proposals".to_string(),
                            "vote_on_proposals".to_string(),
                        ],
                    },
                    Role {
                        name: "guest".to_string(),
                        permissions: vec![
                            "view_proposals".to_string(),
                        ],
                    },
                ]),
            }),
            membership: None,
            proposals: None,
            working_groups: None,
            dispute_resolution: None,
            economic_model: None,
        };
        
        // Store the config
        kernel.store_governance_config("test-coop", config).await.unwrap();
        
        kernel
    }

    #[tokio::test]
    async fn test_role_assignment_and_verification() {
        println!("Starting role assignment and verification test");
        
        // Create a test kernel
        let kernel = create_test_kernel().await;
        
        // Create test identity
        let alice_id = IdentityId("did:icn:test:alice".to_string());
        
        // Assign roles to Alice
        let scope_id = "test-coop";
        let roles = vec!["admin".to_string()];
        
        // Create options with expiration
        let options = RoleAssignmentOptions {
            expiration_days: Some(30),
            scope_type: Some(IdentityScope::Cooperative),
            store_in_dag: false,
        };
        
        // Assign the role with a credential
        let credential_id = kernel.assign_roles(&alice_id, scope_id, roles, Some(options)).await.unwrap();
        
        // Verify the format of the credential ID
        assert!(credential_id.starts_with("credential:role:test-coop:did:icn:test:alice:"));
        
        // Get the verified roles
        let alice_roles = kernel.get_verified_roles(&alice_id, scope_id).await.unwrap();
        
        // Check that Alice has the admin role
        assert_eq!(alice_roles.len(), 1);
        assert!(alice_roles.contains(&"admin".to_string()));
        
        // Check if Alice has permission to create proposals
        let can_create = kernel.check_permission(&alice_id, scope_id, "create_proposals").await.unwrap();
        assert!(can_create);
        
        // Check if Alice has permission to assign roles
        let can_assign = kernel.check_permission(&alice_id, scope_id, "assign_roles").await.unwrap();
        assert!(can_assign);
        
        // Check that Alice doesn't have a permission not granted to admin
        let can_do_special = kernel.check_permission(&alice_id, scope_id, "special_action").await.unwrap();
        assert!(!can_do_special);
        
        // Test role verification with a non-existent identity
        let bob_id = IdentityId("did:icn:test:bob".to_string());
        let bob_roles = kernel.get_verified_roles(&bob_id, scope_id).await.unwrap();
        assert!(bob_roles.is_empty());
        
        // Bob should not have permission to create proposals
        let bob_can_create = kernel.check_permission(&bob_id, scope_id, "create_proposals").await.unwrap();
        assert!(!bob_can_create);
    }
} 