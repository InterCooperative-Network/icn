/*!
 * ICN Wallet Receipt Import
 *
 * Provides functionality for importing execution receipts from various formats
 * and preparing them for local verification.
 */

use std::path::Path;
use thiserror::Error;
use icn_wallet_sync::VerifiableCredential;
use icn_wallet_sync::{import_receipts_from_file as sync_import, ExportError};

/// Error types for importing execution receipts
#[derive(Error, Debug)]
pub enum ImportError {
    #[error("Format error: {0}")]
    FormatError(String),
    
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Verification error: {0}")]
    VerificationError(String),
    
    #[error("Wallet sync error: {0}")]
    WalletSyncError(String),
}

impl From<ExportError> for ImportError {
    fn from(err: ExportError) -> Self {
        match err {
            ExportError::FormatError(msg) => ImportError::FormatError(msg),
            ExportError::IoError(e) => ImportError::IoError(e),
            ExportError::SerializationError(msg) => ImportError::SerializationError(msg),
            ExportError::VerificationError(msg) => ImportError::VerificationError(msg),
        }
    }
}

/// A parsed execution receipt ready for verification
#[derive(Debug, Clone)]
pub struct ExecutionReceipt {
    /// The original verifiable credential
    pub credential: VerifiableCredential,
    /// The proposal ID referenced in the receipt
    pub proposal_id: String,
    /// The DAG anchor CID referenced in the receipt
    pub dag_anchor: Option<String>,
    /// The federation scope this receipt belongs to
    pub federation_scope: String,
    /// The outcome of the execution
    pub outcome: String,
}

impl TryFrom<VerifiableCredential> for ExecutionReceipt {
    type Error = ImportError;
    
    fn try_from(credential: VerifiableCredential) -> Result<Self, Self::Error> {
        // Extract the required fields from the credential
        let subject = &credential.credential_subject;
        
        // Check if it's an execution receipt
        if !credential.types.iter().any(|t| t == "ExecutionReceipt") {
            return Err(ImportError::FormatError(
                "Credential is not an ExecutionReceipt".to_string()
            ));
        }
        
        // Extract proposal ID
        let proposal_id = subject["proposal_id"]
            .as_str()
            .ok_or_else(|| ImportError::FormatError(
                "Receipt missing proposal_id field".to_string()
            ))?
            .to_string();
        
        // Extract DAG anchor (optional)
        let dag_anchor = subject["dag_anchor"]
            .as_str()
            .map(|s| s.to_string());
        
        // Extract federation scope
        let federation_scope = subject["federation_scope"]
            .as_str()
            .ok_or_else(|| ImportError::FormatError(
                "Receipt missing federation_scope field".to_string()
            ))?
            .to_string();
        
        // Extract outcome
        let outcome = subject["outcome"]
            .as_str()
            .ok_or_else(|| ImportError::FormatError(
                "Receipt missing outcome field".to_string()
            ))?
            .to_string();
        
        Ok(ExecutionReceipt {
            credential,
            proposal_id,
            dag_anchor,
            federation_scope,
            outcome,
        })
    }
}

/// Import receipts from a file on disk
pub fn import_receipts_from_file(path: &Path) -> Result<Vec<ExecutionReceipt>, ImportError> {
    // Use the existing sync import function but with verification enabled
    let credentials = sync_import(path, true)?;
    
    // Convert the credentials to ExecutionReceipt objects
    let mut receipts = Vec::new();
    
    for credential in credentials {
        match ExecutionReceipt::try_from(credential) {
            Ok(receipt) => receipts.push(receipt),
            Err(e) => {
                // Log the error but continue processing other receipts
                eprintln!("Skipping invalid receipt: {}", e);
            }
        }
    }
    
    Ok(receipts)
} 