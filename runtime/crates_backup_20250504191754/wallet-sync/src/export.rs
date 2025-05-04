/*!
 * ICN Wallet Receipt Export
 *
 * Provides functionality for exporting execution receipts in various formats
 * for cross-federation use, user presentation, or governance proof.
 */

use crate::federation::{VerifiableCredential, ExportFormat, FederationSyncError};
use std::path::Path;
use std::fs;
use thiserror::Error;

/// Error types for exporting
#[derive(Error, Debug)]
pub enum ExportError {
    #[error("Format error: {0}")]
    FormatError(String),
    
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    SerializationError(String),
    
    #[error("Verification error: {0}")]
    VerificationError(String),
}

/// Exports receipts to a file
pub fn export_receipts_to_file(
    receipts: &[VerifiableCredential],
    format: ExportFormat,
    output_path: &Path,
) -> Result<(), ExportError> {
    // Get string representation
    let content = crate::federation::export_receipts(receipts, format)
        .map_err(|e| match e {
            FederationSyncError::ParseError(msg) => 
                ExportError::SerializationError(msg),
            _ => 
                ExportError::FormatError(format!("Failed to export receipts: {}", e))
        })?;
    
    // Write to file
    fs::write(output_path, content)?;
    
    Ok(())
}

/// Imports receipts from a file
pub fn import_receipts_from_file(
    input_path: &Path,
    verify: bool,
) -> Result<Vec<VerifiableCredential>, ExportError> {
    // Read file content
    let content = fs::read_to_string(input_path)?;
    
    // Determine format by extension
    let ext = input_path.extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    
    let receipts = match ext.to_lowercase().as_str() {
        "json" => {
            // Try to parse as a bundle first
            if let Ok(bundle) = serde_json::from_str::<crate::federation::SignedReceiptBundle>(&content) {
                bundle.receipts
            } else {
                // Try to parse as a regular array
                serde_json::from_str::<Vec<VerifiableCredential>>(&content)
                    .map_err(|e| ExportError::SerializationError(
                        format!("Failed to parse JSON: {}", e)
                    ))?
            }
        },
        "csv" => {
            // Simple CSV parsing - in a real implementation, use a CSV library
            let mut receipts = Vec::new();
            let lines = content.lines().skip(1); // Skip header
            
            for line in lines {
                let parts: Vec<&str> = line.split(',').collect();
                if parts.len() < 6 {
                    continue;
                }
                
                // Create a basic receipt from CSV fields
                // This is a simplification - real parsing would be more robust
                let credential_subject = serde_json::json!({
                    "id": parts[1], // Use issuer as ID
                    "proposal_id": parts[3],
                    "outcome": parts[4],
                    "federation_scope": parts[5],
                });
                
                let receipt = VerifiableCredential {
                    context: vec!["https://www.w3.org/2018/credentials/v1".to_string()],
                    id: parts[0].to_string(),
                    types: vec!["VerifiableCredential".to_string(), "ExecutionReceipt".to_string()],
                    issuer: parts[1].to_string(),
                    issuance_date: parts[2].to_string(),
                    credential_subject,
                    proof: None,
                };
                
                receipts.push(receipt);
            }
            
            receipts
        },
        _ => {
            return Err(ExportError::FormatError(
                format!("Unsupported file extension: {}", ext)
            ));
        }
    };
    
    // Verify receipts if requested
    if verify {
        let mut verified_receipts = Vec::new();
        for receipt in receipts {
            if crate::federation::verify_execution_receipt(&receipt) {
                verified_receipts.push(receipt);
            } else {
                // Log the issue but don't fail - just skip invalid receipts
                eprintln!("Skipping unverified receipt: {}", receipt.id);
            }
        }
        Ok(verified_receipts)
    } else {
        Ok(receipts)
    }
} 