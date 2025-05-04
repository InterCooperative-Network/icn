/*!
# Credentials System

This module implements the Verifiable Credential system for the ICN Runtime.
It provides functionality for issuing, verifying, and managing credentials
that document governance actions, economic flows, and execution results.
*/

use crate::{ConcreteHostEnvironment, InternalHostError, ResourceType};
use chrono::{DateTime, Utc};
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use uuid::Uuid;
use thiserror::Error;

/// Type of credential being issued
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CredentialType {
    /// Receipt for a proposal execution
    ExecutionReceipt,
    /// Economic resource transfer
    ResourceTransfer,
    /// Proposal outcome (approval, rejection, etc.)
    ProposalOutcome,
    /// Membership credential
    MembershipCredential,
}

/// Subject data for an execution receipt
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionReceiptSubject {
    /// ID of the subject entity (usually a DID)
    pub id: String,
    /// ID of the proposal that was executed
    pub proposal_id: String,
    /// Outcome of the execution (success, failure)
    pub outcome: String,
    /// Resources consumed during execution
    pub resource_usage: HashMap<String, u64>,
    /// CID of the DAG anchor containing execution data
    pub dag_anchor: String,
    /// Federation scope of the execution
    pub federation_scope: String,
    /// Timestamp of the execution
    pub execution_timestamp: DateTime<Utc>,
    /// Optional AgoraNet thread ID associated with this proposal
    pub thread_id: Option<String>,
}

/// A W3C Verifiable Credential
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VerifiableCredential<T> {
    /// Context defines the schema
    #[serde(rename = "@context")]
    pub context: Vec<String>,
    /// ID of the credential (usually a UUID)
    pub id: String,
    /// Types of the credential
    #[serde(rename = "type")]
    pub types: Vec<String>,
    /// Entity that issued the credential
    pub issuer: String,
    /// When the credential was issued
    pub issuance_date: DateTime<Utc>,
    /// Optional expiration date
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expiration_date: Option<DateTime<Utc>>,
    /// Subject data of the credential
    pub credential_subject: T,
    /// Optional proof of the credential
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proof: Option<CredentialProof>,
}

/// Proof for a Verifiable Credential
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialProof {
    /// Type of proof
    #[serde(rename = "type")]
    pub proof_type: String,
    /// When the proof was created
    pub created: DateTime<Utc>,
    /// Verification method
    pub verification_method: String,
    /// Purpose of the proof
    pub proof_purpose: String,
    /// The signature value
    pub proof_value: String,
}

/// Error type for credential operations
#[derive(Error, Debug)]
pub enum CredentialError {
    #[error("Storage error: {0}")]
    StorageError(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Invalid credential format: {0}")]
    InvalidFormat(String),
    
    #[error("Not found")]
    NotFound,
    
    #[error("Other error: {0}")]
    Other(String),
}

/// Issues an execution receipt credential for a proposal execution
pub async fn issue_execution_receipt(
    host_env: &ConcreteHostEnvironment,
    proposal_id: &str,
    outcome: &str,
    resource_usage: HashMap<ResourceType, u64>,
    dag_anchor_cid: &str,
    federation_scope: &str,
    thread_id: Option<String>,
) -> Result<String, InternalHostError> {
    // Create a map of resource types to usage amounts with string keys
    let resource_map: HashMap<String, u64> = resource_usage.iter()
        .map(|(k, v)| (format!("{:?}", k), *v))
        .collect();
    
    // Create the credential subject
    let subject = ExecutionReceiptSubject {
        id: host_env.caller_did().to_string(),
        proposal_id: proposal_id.to_string(),
        outcome: outcome.to_string(),
        resource_usage: resource_map,
        dag_anchor: dag_anchor_cid.to_string(),
        federation_scope: federation_scope.to_string(),
        execution_timestamp: Utc::now(),
        thread_id,
    };
    
    // Create the credential
    let credential = VerifiableCredential {
        context: vec![
            "https://www.w3.org/2018/credentials/v1".to_string(),
            "https://icn.network/schemas/2023/credentials/execution/v1".to_string(),
        ],
        id: format!("urn:uuid:{}", Uuid::new_v4()),
        types: vec![
            "VerifiableCredential".to_string(),
            "ExecutionReceipt".to_string(),
        ],
        issuer: host_env.caller_did().to_string(),
        issuance_date: Utc::now(),
        expiration_date: None,
        credential_subject: subject,
        proof: None, // No proof for now, could be added later
    };
    
    // Serialize the credential to JSON
    let cred_json = serde_json::to_string(&credential)
        .map_err(|e| InternalHostError::CodecError(format!("Failed to serialize credential: {}", e)))?;
        
    // Anchor the credential to the DAG
    let anchor_key = format!("credential:execution_receipt:{}", proposal_id);
    let cid = host_env.anchor_to_dag(&anchor_key, cred_json.into_bytes()).await?;
    
    // Log the credential issuance
    tracing::info!(
        proposal_id = %proposal_id,
        credential_id = %credential.id,
        dag_cid = %cid,
        "Issued execution receipt credential"
    );
    
    Ok(cid)
}

/// Retrieves an execution receipt credential by its DAG CID
pub async fn get_execution_receipt_by_cid(
    host_env: &ConcreteHostEnvironment,
    cid: &str,
) -> Result<Option<VerifiableCredential<ExecutionReceiptSubject>>, InternalHostError> {
    // Helper method could be implemented here to retrieve the credential from the DAG
    // For now, we'll return None as this would require additional host environment methods
    Ok(None)
}

/// Retrieves execution receipt credentials by proposal ID
pub async fn get_execution_receipts_by_proposal(
    host_env: &ConcreteHostEnvironment,
    proposal_id: &str,
) -> Result<Vec<VerifiableCredential<ExecutionReceiptSubject>>, InternalHostError> {
    // This would query the DAG for credentials with a specific key pattern
    // For now, return an empty vector
    Ok(Vec::new())
}

/// Issues a resource transfer credential
pub async fn issue_resource_transfer_credential(
    host_env: &ConcreteHostEnvironment,
    from_did: &str,
    to_did: &str,
    resource_type: ResourceType,
    amount: u64,
    related_proposal_id: Option<&str>,
) -> Result<String, InternalHostError> {
    // Implementation similar to execution receipt
    // For now, we'll focus on the execution receipt as requested
    unimplemented!("Resource transfer credential issuance not implemented yet")
}

/// Issues a proposal outcome credential
pub async fn issue_proposal_outcome_credential(
    host_env: &ConcreteHostEnvironment,
    proposal_id: &str,
    outcome: &str,
    voters: Vec<String>,
    vote_counts: HashMap<String, u32>,
) -> Result<String, InternalHostError> {
    // Implementation similar to execution receipt
    // For now, we'll focus on the execution receipt as requested
    unimplemented!("Proposal outcome credential issuance not implemented yet")
}

/// Retrieves execution receipts by scope and optional timestamp
pub async fn get_execution_receipts(
    host_env: &ConcreteHostEnvironment,
    scope: &str,
    since_timestamp: Option<i64>,
) -> Result<Vec<VerifiableCredential<ExecutionReceiptSubject>>, CredentialError> {
    // Get the storage manager
    let storage_manager = host_env.storage_manager()
        .map_err(|e| CredentialError::StorageError(format!("Failed to get storage manager: {}", e)))?;
    
    // Build the prefix for querying
    let prefix = format!("credential:execution_receipt");
    
    // Query all credentials that match the prefix pattern
    // In a real implementation, this would use a more efficient index-based query
    let mut receipts = Vec::new();
    
    // Get all keys that match the prefix
    // This is a simplified implementation - in a real system you'd use a proper query
    // against an indexed store
    let credentials = storage_manager.list_anchors(&prefix)
        .await
        .map_err(|e| CredentialError::StorageError(format!("Failed to list anchors: {}", e)))?;
    
    // Process each credential
    for (key, data) in credentials {
        // Parse the credential
        let credential: VerifiableCredential<ExecutionReceiptSubject> = serde_json::from_slice(&data)
            .map_err(|e| CredentialError::SerializationError(format!("Failed to deserialize credential: {}", e)))?;
        
        // Check scope
        if credential.credential_subject.federation_scope != scope {
            continue;
        }
        
        // Check timestamp if provided
        if let Some(since) = since_timestamp {
            let cred_time = credential.credential_subject.execution_timestamp
                .timestamp();
            
            if cred_time < since {
                continue;
            }
        }
        
        // Add to results
        receipts.push(credential);
    }
    
    Ok(receipts)
}

/// Retrieves simplified execution receipts by scope and optional timestamp
/// Returns JSON format for easy consumption by WASM modules
pub async fn get_simplified_execution_receipts(
    host_env: &ConcreteHostEnvironment,
    scope: &str,
    since_timestamp: Option<i64>,
) -> Result<String, CredentialError> {
    // Get the storage manager
    let storage_manager = host_env.storage_manager()
        .map_err(|e| CredentialError::StorageError(format!("Failed to get storage manager: {}", e)))?;
    
    // Build the prefix for querying
    let prefix = format!("{}:ExecutionReceipt", scope);
    
    // Query all receipts that match the prefix pattern
    let mut receipts = Vec::new();
    
    // Get all keys that match the prefix
    let anchored_data = storage_manager.list_anchors(&prefix)
        .await
        .map_err(|e| CredentialError::StorageError(format!("Failed to list anchors: {}", e)))?;
    
    // Process each receipt
    for (_, data) in anchored_data {
        // Parse the receipt
        let receipt: serde_json::Value = serde_json::from_slice(&data)
            .map_err(|e| CredentialError::SerializationError(format!("Failed to deserialize receipt: {}", e)))?;
        
        // Check timestamp if provided
        if let Some(since) = since_timestamp {
            if let Some(timestamp) = receipt["timestamp"].as_i64() {
                if timestamp < since {
                    continue;
                }
            }
        }
        
        // Add to results
        receipts.push(receipt);
    }
    
    // Serialize to JSON
    serde_json::to_string(&receipts)
        .map_err(|e| CredentialError::SerializationError(format!("Failed to serialize receipts: {}", e)))
} 