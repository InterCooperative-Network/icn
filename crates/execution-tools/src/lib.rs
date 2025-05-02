/*!
# ICN Execution Tools

This crate implements CLI helpers, replay logic, and common utilities for the ICN Runtime.
It serves as a bridge between the core runtime and the CLI tools.

## Architectural Tenets
- Replayability is a fundamental building block for trust and auditability
- CLI tools provide user-friendly access to runtime functionality
- Common utilities for interacting with the runtime
*/

use anyhow::{Context, Result};
use icn_dag::DagNode;
use icn_identity::{IdentityId, IdentityScope, VerifiableCredential};
use thiserror::Error;
use std::fs;
use std::path::Path;
use icn_core_vm::{ResourceType, ResourceAuthorization};
use icn_governance_kernel::config::ProposalTemplate;
use std::collections::HashMap;

/// Errors that can occur during execution
#[derive(Debug, Error)]
pub enum ExecutionError {
    #[error("Replay failed: {0}")]
    ReplayFailed(String),
    
    #[error("Export failed: {0}")]
    ExportFailed(String),
    
    #[error("Import failed: {0}")]
    ImportFailed(String),
    
    #[error("Command failed: {0}")]
    CommandFailed(String),
}

/// Helper for replaying operations from the DAG
pub struct ReplayHelper {
    start_node: DagNode,
    end_node: Option<DagNode>,
}

impl ReplayHelper {
    /// Create a new replay helper
    pub fn new(start_node: DagNode, end_node: Option<DagNode>) -> Self {
        Self {
            start_node,
            end_node,
        }
    }
    
    /// Replay operations between the start and end nodes
    pub fn replay(&self) -> Result<(), ExecutionError> {
        // Placeholder implementation
        Err(ExecutionError::ReplayFailed("Not implemented".to_string()))
    }
    
    /// Get the list of operations to replay
    pub fn get_operations(&self) -> Result<Vec<DagNode>, ExecutionError> {
        // Placeholder implementation
        Err(ExecutionError::ReplayFailed("Not implemented".to_string()))
    }
}

/// Helper for exporting verifiable credentials
// TODO(V3-MVP): Implement Credential export pipeline
pub struct CredentialHelper;

impl CredentialHelper {
    /// Export a verifiable credential to a file
    pub fn export_credential(credential: &VerifiableCredential, path: &str) -> Result<(), ExecutionError> {
        // Serialize the credential to JSON
        let json = serde_json::to_string_pretty(credential)
            .map_err(|e| ExecutionError::ExportFailed(format!("Failed to serialize credential: {}", e)))?;
        
        // Ensure directory exists
        if let Some(parent) = Path::new(path).parent() {
            fs::create_dir_all(parent)
                .map_err(|e| ExecutionError::ExportFailed(format!("Failed to create directory: {}", e)))?;
        }
        
        // Write to file
        fs::write(path, json)
            .map_err(|e| ExecutionError::ExportFailed(format!("Failed to write to file: {}", e)))?;
        
        Ok(())
    }
    
    /// Import a verifiable credential from a file
    pub fn import_credential(path: &str) -> Result<VerifiableCredential, ExecutionError> {
        // Read file
        let json = fs::read_to_string(path)
            .map_err(|e| ExecutionError::ImportFailed(format!("Failed to read file: {}", e)))?;
        
        // Deserialize
        let credential: VerifiableCredential = serde_json::from_str(&json)
            .map_err(|e| ExecutionError::ImportFailed(format!("Failed to deserialize credential: {}", e)))?;
        
        Ok(credential)
    }
    
    /// Verify a verifiable credential
    pub async fn verify_credential(credential: &VerifiableCredential) -> Result<bool, ExecutionError> {
        // Use the verify method from the credential itself
        credential.verify().await
            .map_err(|e| ExecutionError::CommandFailed(format!("Verification failed: {}", e)))
    }
}

/// CLI command helpers
pub mod cli_helpers {
    use super::*;
    
    /// Helper for propose command
    pub fn propose_command(
        template_path: &str,
        input_path: &str,
        identity: &IdentityId,
    ) -> Result<DagNode> {
        // Placeholder implementation
        Err(anyhow::anyhow!("Not implemented"))
    }
    
    /// Helper for vote command
    pub fn vote_command(
        proposal_id: &str,
        vote: bool,
        reason: &str,
        identity: &IdentityId,
    ) -> Result<DagNode> {
        // Placeholder implementation
        Err(anyhow::anyhow!("Not implemented"))
    }
    
    /// Helper for execute command
    pub fn execute_command(
        proposal_id: &str,
        identity: &IdentityId,
    ) -> Result<DagNode> {
        // Placeholder implementation
        Err(anyhow::anyhow!("Not implemented"))
    }
    
    /// Helper for anchor command
    pub fn anchor_command(
        dag_root: &[u8],
        identity: &IdentityId,
    ) -> Result<DagNode> {
        // Placeholder implementation
        Err(anyhow::anyhow!("Not implemented"))
    }
    
    /// Helper for identity register command
    pub fn identity_register_command(
        scope: IdentityScope,
        name: &str,
    ) -> Result<IdentityId> {
        // Placeholder implementation
        Err(anyhow::anyhow!("Not implemented"))
    }
}

/// Derive resource authorizations from a proposal template
pub fn derive_authorizations(template: &ProposalTemplate) -> Vec<ResourceAuthorization> {
    let mut authorizations = Vec::new();
    
    // Start with base authorizations that every proposal needs
    authorizations.push(ResourceAuthorization::new(
        ResourceType::Compute, 
        1_000_000, // Base computation allowance
        None,     // No specific context
        "Base computation allowance for proposal execution".to_string()
    ));
    
    authorizations.push(ResourceAuthorization::new(
        ResourceType::Storage, 
        500_000, // Base storage allowance (bytes)
        None,    // No specific context
        "Base storage allowance for proposal execution".to_string()
    ));
    
    // If the template indicates it works with the DAG, add DAG authorization
    if template.uses_dag {
        authorizations.push(ResourceAuthorization::new(
            ResourceType::DAG, 
            10,     // Number of DAG operations
            None,   // No specific context
            "DAG operations allowance for proposal execution".to_string()
        ));
    }
    
    // If the template indicates it needs to perform economic operations
    if template.uses_economics {
        authorizations.push(ResourceAuthorization::new(
            ResourceType::Budget, 
            5,      // Number of budget operations
            None,   // No specific context
            "Budget operations allowance for proposal execution".to_string()
        ));
    }
    
    // If the template indicates identity operations
    if template.uses_identity {
        authorizations.push(ResourceAuthorization::new(
            ResourceType::Identity, 
            10,     // Number of identity operations
            None,   // No specific context
            "Identity operations allowance for proposal execution".to_string()
        ));
    }
    
    // Add custom authorizations from the template
    for (resource, amount) in &template.resource_authorizations {
        // Check if we already have this resource type
        let existing_index = authorizations.iter().position(|a| a.resource_type == *resource);
        
        if let Some(index) = existing_index {
            // Update existing authorization if the new amount is higher
            if authorizations[index].amount < *amount {
                authorizations[index].amount = *amount;
            }
        } else {
            // Add a new authorization
            authorizations.push(ResourceAuthorization::new(
                resource.clone(),
                *amount,
                None,
                format!("Custom {} authorization from template", resource)
            ));
        }
    }
    
    authorizations
}

/// Prepare a VM context for CCL execution based on proposal and template
pub fn prepare_execution_context(
    proposal_cid: cid::Cid,
    template: &ProposalTemplate,
    caller_did: String,
    caller_scope: icn_identity::IdentityScope
) -> icn_core_vm::VmContext {
    // Derive authorizations from the template
    let authorizations = derive_authorizations(template);
    
    // Get all resource types from the authorizations
    let resource_types = authorizations.iter()
        .map(|auth| auth.resource_type.clone())
        .collect();
    
    // Create a VM context with the appropriate authorizations
    icn_core_vm::VmContext::with_authorizations(
        caller_did,
        caller_scope,
        resource_types,
        authorizations,
        uuid::Uuid::new_v4().to_string(), // Generate a unique execution ID
        chrono::Utc::now().timestamp(),   // Current timestamp
        Some(proposal_cid.to_string())    // Associated proposal CID
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_identity::{IdentityId, VerifiableCredential};
    use std::fs;
    use std::path::Path;
    
    #[test]
    fn test_export_import_credential() {
        // Create a temporary test file
        let test_file = "test_credential.json";
        
        // Clean up any previous test file
        if Path::new(test_file).exists() {
            fs::remove_file(test_file).unwrap();
        }
        
        // Create a simple credential
        let issuer = IdentityId::new("did:icn:test:issuer");
        let subject = IdentityId::new("did:icn:test:subject");
        let claims = serde_json::json!({
            "name": "Test Subject",
            "property": "value"
        });
        
        let vc = VerifiableCredential::new(
            vec!["VerifiableCredential".to_string(), "TestCredential".to_string()],
            &issuer,
            &subject,
            claims,
        );
        
        // Export the credential
        let export_result = CredentialHelper::export_credential(&vc, test_file);
        assert!(export_result.is_ok(), "Failed to export credential");
        
        // Verify file exists
        assert!(Path::new(test_file).exists(), "Credential file wasn't created");
        
        // Import the credential
        let import_result = CredentialHelper::import_credential(test_file);
        assert!(import_result.is_ok(), "Failed to import credential");
        
        let imported_vc = import_result.unwrap();
        
        // Verify it's the same credential
        assert_eq!(imported_vc.issuer, vc.issuer);
        assert_eq!(imported_vc.credential_type, vc.credential_type);
        
        // Clean up
        fs::remove_file(test_file).unwrap();
    }
    
    #[tokio::test]
    async fn test_credential_verification() {
        // Create a simple credential
        let issuer = IdentityId::new("did:icn:test:issuer");
        let subject = IdentityId::new("did:icn:test:subject");
        let claims = serde_json::json!({
            "name": "Test Subject",
            "property": "value"
        });
        
        let vc = VerifiableCredential::new(
            vec!["VerifiableCredential".to_string(), "TestCredential".to_string()],
            &issuer,
            &subject,
            claims,
        );
        
        // Test the verify function - without a proof, it should return false but not error
        let verify_result = CredentialHelper::verify_credential(&vc).await;
        assert!(verify_result.is_ok(), "Verification failed with error");
        assert!(!verify_result.unwrap(), "Verification should return false for unsigned credential");
    }
} 