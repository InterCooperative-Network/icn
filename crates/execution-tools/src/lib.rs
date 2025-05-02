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