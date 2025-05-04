/*!
 * ICN Federation Receipt Verification Service
 *
 * Provides services for verification of execution receipts across federations.
 */

use anyhow::Result;
use thiserror::Error;
use icn_wallet_agent::{
    EncryptedBundle, decrypt_receipt_bundle, ExecutionReceipt
};
use icn_wallet_core::replay::replay_and_verify_receipt;
use icn_wallet_core::dag::DagStorageManager;
use serde::{Serialize, Deserialize};

pub mod server;

/// Error types for receipt verification
#[derive(Error, Debug)]
pub enum VerifierError {
    #[error("Decryption error: {0}")]
    DecryptionError(String),
    
    #[error("Verification error: {0}")]
    VerificationError(String),
    
    #[error("DAG error: {0}")]
    DagError(String),
    
    #[error("Invalid bundle format: {0}")]
    InvalidBundleFormat(String),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Federation not authorized: {0}")]
    NotAuthorized(String),
}

/// Request to verify a receipt bundle
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifyBundleRequest {
    /// The encrypted bundle to verify
    pub bundle: String,
    
    /// Optional federation ID that the sender belongs to
    pub sender_federation: Option<String>,
}

/// Results of bundle verification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationResult {
    /// Whether the verification was successful
    pub success: bool,
    
    /// Number of receipts in the bundle
    pub receipt_count: usize,
    
    /// Number of verified receipts
    pub verified_count: usize,
    
    /// Federation ID of the sender
    pub sender_federation: String,
    
    /// Federation scope of the receipts
    pub federation_scope: String,
    
    /// Timestamp of verification
    pub verification_timestamp: String,
    
    /// If successful, the CID that anchors this verification
    pub anchor_cid: Option<String>,
    
    /// Detailed results for each receipt
    pub receipt_results: Vec<ReceiptVerificationResult>,
}

/// Result of verifying a single receipt
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReceiptVerificationResult {
    /// Receipt ID
    pub id: String,
    
    /// Whether the receipt verified successfully
    pub verified: bool,
    
    /// Error message if verification failed
    pub error: Option<String>,
    
    /// Proposal ID from the receipt
    pub proposal_id: String,
    
    /// DAG anchor CID from the receipt
    pub dag_anchor: Option<String>,
}

/// Configuration for the verifier
#[derive(Debug, Clone)]
pub struct VerifierConfig {
    /// Private key for decrypting bundles (base64 encoded)
    pub private_key: String,
    
    /// Federation ID that this verifier belongs to
    pub federation_id: String,
    
    /// Authorized federation IDs for verification
    pub authorized_federations: Vec<String>,
}

/// Receipt verifier service
pub struct ReceiptVerifier<D: DagStorageManager> {
    /// Configuration
    config: VerifierConfig,
    
    /// DAG storage manager for verification
    dag_store: D,
}

impl<D: DagStorageManager> ReceiptVerifier<D> {
    /// Create a new receipt verifier
    pub fn new(config: VerifierConfig, dag_store: D) -> Self {
        Self {
            config,
            dag_store,
        }
    }
    
    /// Verify a receipt bundle
    pub async fn verify_bundle(&self, request: VerifyBundleRequest) -> Result<VerificationResult, VerifierError> {
        // Decode the bundle
        let bundle_json = base64::Engine::decode(
            &base64::engine::general_purpose::STANDARD, 
            &request.bundle
        ).map_err(|e| VerifierError::InvalidBundleFormat(format!("Failed to decode bundle: {}", e)))?;
        
        let bundle: EncryptedBundle = serde_json::from_slice(&bundle_json)
            .map_err(|e| VerifierError::InvalidBundleFormat(format!("Failed to parse bundle: {}", e)))?;
        
        // Check if the sender is authorized
        if let Some(sender_fed) = &request.sender_federation {
            if !self.config.authorized_federations.contains(sender_fed) {
                return Err(VerifierError::NotAuthorized(
                    format!("Federation {} is not authorized", sender_fed)
                ));
            }
        }
        
        // Decrypt the bundle
        let credentials = decrypt_receipt_bundle(&bundle, &self.config.private_key)
            .map_err(|e| VerifierError::DecryptionError(format!("Failed to decrypt bundle: {}", e)))?;
        
        // Convert credentials to ExecutionReceipt objects
        let mut receipts = Vec::new();
        for credential in &credentials {
            // In a real implementation, we would use the TryFrom trait
            // For now, we'll manually construct ExecutionReceipt objects
            let subject = &credential.credential_subject;
            
            let proposal_id = subject["proposal_id"].as_str()
                .ok_or_else(|| VerifierError::InvalidBundleFormat("Missing proposal_id".to_string()))?
                .to_string();
                
            let outcome = subject["outcome"].as_str()
                .ok_or_else(|| VerifierError::InvalidBundleFormat("Missing outcome".to_string()))?
                .to_string();
                
            let federation_scope = subject["federation_scope"].as_str()
                .ok_or_else(|| VerifierError::InvalidBundleFormat("Missing federation_scope".to_string()))?
                .to_string();
                
            let dag_anchor = subject["dag_anchor"].as_str().map(|s| s.to_string());
            
            let receipt = ExecutionReceipt {
                credential: credential.clone(),
                proposal_id,
                dag_anchor,
                federation_scope,
                outcome,
            };
            
            receipts.push(receipt);
        }
        
        // Verify each receipt
        let mut verified_count = 0;
        let mut receipt_results = Vec::new();
        
        for receipt in &receipts {
            let result = match replay_and_verify_receipt(receipt, &self.dag_store).await {
                Ok(true) => {
                    verified_count += 1;
                    ReceiptVerificationResult {
                        id: receipt.credential.id.clone(),
                        verified: true,
                        error: None,
                        proposal_id: receipt.proposal_id.clone(),
                        dag_anchor: receipt.dag_anchor.clone(),
                    }
                },
                Ok(false) => {
                    ReceiptVerificationResult {
                        id: receipt.credential.id.clone(),
                        verified: false,
                        error: Some("Receipt failed verification".to_string()),
                        proposal_id: receipt.proposal_id.clone(),
                        dag_anchor: receipt.dag_anchor.clone(),
                    }
                },
                Err(e) => {
                    ReceiptVerificationResult {
                        id: receipt.credential.id.clone(),
                        verified: false,
                        error: Some(format!("Verification error: {}", e)),
                        proposal_id: receipt.proposal_id.clone(),
                        dag_anchor: receipt.dag_anchor.clone(),
                    }
                },
            };
            
            receipt_results.push(result);
        }
        
        // If all receipts verified successfully, create a DAG anchor
        // In a real implementation, we would create a DAG node to anchor this verification
        let anchor_cid = if verified_count == receipts.len() && !receipts.is_empty() {
            Some("bafybeihczzwsuj5huiqnuoo7nmwdkahxi7ny2qgwib4g34lqebzs5mmz4q".to_string())
        } else {
            None
        };
        
        // Create the verification result
        let result = VerificationResult {
            success: verified_count == receipts.len() && !receipts.is_empty(),
            receipt_count: receipts.len(),
            verified_count,
            sender_federation: bundle.metadata.sender_did.clone(),
            federation_scope: bundle.metadata.federation_scope.clone(),
            verification_timestamp: chrono::Utc::now().to_rfc3339(),
            anchor_cid,
            receipt_results,
        };
        
        Ok(result)
    }
} 