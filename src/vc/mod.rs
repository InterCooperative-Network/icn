// Verifiable Credentials module
use crate::identity::{Identity, IdentityError};
use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use thiserror::Error;
use std::fs;
use std::path::Path;
use uuid::Uuid;
use base64::{Engine as _, engine::general_purpose::STANDARD};

// Export the QR code module
pub mod qr;
pub use qr::{QrCodeError, QrFormat, encode_credential_for_qr, decode_credential_from_qr, generate_credential_qr};

pub mod zkp_utils;

// Re-export key types and functions
pub use zkp_utils::{
    ZkProofType, 
    ZkSelectiveDisclosure, 
    SelectiveDisclosureParams,
    create_selective_disclosure,
    verify_selective_disclosure,
    ZkProofPresentation,
    create_zkp_presentation,
};

/// Errors that can occur in verifiable credential operations
#[derive(Debug, Error)]
pub enum CredentialError {
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    
    #[error("Serialization error: {0}")]
    SerializationError(#[from] serde_json::Error),
    
    #[error("Identity error: {0}")]
    IdentityError(#[from] IdentityError),
    
    #[error("Credential error: {0}")]
    CredentialError(String),
}

/// Verification method for a credential
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationMethod {
    pub id: String,
    #[serde(rename = "type")]
    pub type_: String,
    pub controller: String,
    pub publicKeyBase64: String,
}

/// Proof for a verifiable credential
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Proof {
    #[serde(rename = "type")]
    pub type_: String,
    pub created: DateTime<Utc>,
    pub verificationMethod: String,
    pub proofPurpose: String,
    pub proofValue: String,
}

/// Subject of a federation member credential
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationMemberSubject {
    pub id: String,
    pub federationMember: FederationMember,
}

/// Federation member information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationMember {
    pub scope: String,
    pub username: String,
    pub role: String,
}

/// A verifiable credential for federation membership
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiableCredential {
    #[serde(rename = "@context")]
    pub context: Vec<String>,
    pub id: String,
    #[serde(rename = "type")]
    pub types: Vec<String>,
    pub issuer: String,
    pub issuanceDate: DateTime<Utc>,
    pub credentialSubject: FederationMemberSubject,
    pub proof: Option<Proof>,
}

/// Generator for verifiable credentials
pub struct CredentialGenerator;

impl CredentialGenerator {
    /// Create a new credential generator
    pub fn new() -> Self {
        Self
    }
    
    /// Generate a federation member credential
    pub fn generate_federation_member(
        &self,
        identity: &Identity,
        role: Option<&str>,
    ) -> Result<VerifiableCredential, CredentialError> {
        let now = Utc::now();
        let id = format!("urn:uuid:{}", Uuid::new_v4());
        
        // Create the credential
        let credential = VerifiableCredential {
            context: vec![
                "https://www.w3.org/2018/credentials/v1".to_string(),
                "https://www.w3.org/2018/credentials/examples/v1".to_string(),
            ],
            id,
            types: vec![
                "VerifiableCredential".to_string(),
                "FederationMemberCredential".to_string(),
            ],
            issuer: identity.did().to_string(),
            issuanceDate: now,
            credentialSubject: FederationMemberSubject {
                id: identity.did().to_string(),
                federationMember: FederationMember {
                    scope: identity.scope().to_string(),
                    username: identity.username().to_string(),
                    role: role.unwrap_or("member").to_string(),
                },
            },
            proof: None, // Proof will be added later
        };
        
        Ok(credential)
    }
    
    /// Sign a credential
    pub fn sign_credential(
        &self,
        credential: &mut VerifiableCredential,
        identity: &Identity,
    ) -> Result<(), CredentialError> {
        // Serialize credential without proof
        let temp_credential = VerifiableCredential {
            context: credential.context.clone(),
            id: credential.id.clone(),
            types: credential.types.clone(),
            issuer: credential.issuer.clone(),
            issuanceDate: credential.issuanceDate,
            credentialSubject: credential.credentialSubject.clone(),
            proof: None,
        };
        
        let credential_json = serde_json::to_string(&temp_credential)?;
        
        // Sign the credential
        let signature = identity.sign(credential_json.as_bytes())
            .map_err(CredentialError::IdentityError)?;
        
        // Encode the signature
        let proof_value = STANDARD.encode(signature);
        
        // Create the proof - assuming Ed25519 for simplicity
        let proof = Proof {
            type_: "Ed25519Signature2020".to_string(),
            created: Utc::now(),
            verificationMethod: format!("{}#keys-1", identity.did()),
            proofPurpose: "assertionMethod".to_string(),
            proofValue: proof_value,
        };
        
        // Add the proof to the credential
        credential.proof = Some(proof);
        
        Ok(())
    }
    
    /// Verify a credential
    pub fn verify_credential(
        &self,
        credential: &VerifiableCredential,
    ) -> Result<bool, CredentialError> {
        // Extract the proof
        let proof = match &credential.proof {
            Some(p) => p,
            None => return Err(CredentialError::CredentialError("No proof found".to_string())),
        };
        
        // Create a temporary credential without the proof for verification
        let temp_credential = VerifiableCredential {
            context: credential.context.clone(),
            id: credential.id.clone(),
            types: credential.types.clone(),
            issuer: credential.issuer.clone(),
            issuanceDate: credential.issuanceDate,
            credentialSubject: credential.credentialSubject.clone(),
            proof: None,
        };
        
        // Serialize the credential without the proof
        let credential_json = serde_json::to_string(&temp_credential)?;
        
        // Decode the signature
        let signature = STANDARD.decode(&proof.proofValue)
            .map_err(|e| CredentialError::CredentialError(format!("Failed to decode signature: {}", e)))?;
        
        // In a real implementation, we would need to:
        // 1. Resolve the DID to get the verification method
        // 2. Use the verification method to verify the signature
        
        // For now, we'll just print a message
        println!("Verification of credential would happen here with DID resolution");
        
        // Return success as a placeholder
        Ok(true)
    }
    
    /// Export a credential to a file
    pub fn export_credential(
        &self,
        credential: &VerifiableCredential,
        path: &Path,
    ) -> Result<(), CredentialError> {
        let credential_json = serde_json::to_string_pretty(credential)?;
        fs::write(path, credential_json)?;
        Ok(())
    }
    
    /// Import a credential from a file
    pub fn import_credential(
        &self,
        path: &Path,
    ) -> Result<VerifiableCredential, CredentialError> {
        let credential_json = fs::read_to_string(path)?;
        let credential: VerifiableCredential = serde_json::from_str(&credential_json)?;
        Ok(credential)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    // TODO: Add tests for credential functionality
} 