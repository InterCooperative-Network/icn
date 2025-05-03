use serde::{Serialize, Deserialize};
use serde_json::Value;
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use crate::error::{WalletResult, WalletError};
use crate::identity::IdentityWallet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiableCredential {
    #[serde(rename = "@context")]
    context: Vec<String>,
    #[serde(rename = "type")]
    credential_type: Vec<String>,
    issuer: String,
    #[serde(rename = "issuanceDate")]
    issuance_date: String,
    #[serde(rename = "credentialSubject")]
    credential_subject: Value,
    proof: Option<CredentialProof>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CredentialProof {
    #[serde(rename = "type")]
    proof_type: String,
    created: String,
    #[serde(rename = "verificationMethod")]
    verification_method: String,
    #[serde(rename = "proofPurpose")]
    proof_purpose: String,
    jws: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerifiablePresentation {
    #[serde(rename = "@context")]
    context: Vec<String>,
    #[serde(rename = "type")]
    presentation_type: Vec<String>,
    #[serde(rename = "verifiableCredential")]
    credentials: Vec<VerifiableCredential>,
    holder: String,
    proof: Option<CredentialProof>,
}

impl VerifiableCredential {
    pub fn new(
        issuer: String,
        credential_subject: Value,
        credential_types: Vec<String>,
    ) -> Self {
        let issuance_date = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        
        let mut credential_type = vec!["VerifiableCredential".to_string()];
        credential_type.extend(credential_types);
        
        Self {
            context: vec![
                "https://www.w3.org/2018/credentials/v1".to_string(),
                "https://www.w3.org/2018/credentials/examples/v1".to_string(),
            ],
            credential_type,
            issuer,
            issuance_date,
            credential_subject,
            proof: None,
        }
    }
    
    pub fn with_proof(mut self, proof: CredentialProof) -> Self {
        self.proof = Some(proof);
        self
    }
    
    pub fn to_json(&self) -> WalletResult<String> {
        serde_json::to_string(self)
            .map_err(|e| WalletError::SerializationError(format!("Failed to serialize credential: {}", e)))
    }
}

impl VerifiablePresentation {
    pub fn new(
        holder: String, 
        credentials: Vec<VerifiableCredential>
    ) -> Self {
        Self {
            context: vec![
                "https://www.w3.org/2018/credentials/v1".to_string(),
            ],
            presentation_type: vec!["VerifiablePresentation".to_string()],
            credentials,
            holder,
            proof: None,
        }
    }
    
    pub fn with_proof(mut self, proof: CredentialProof) -> Self {
        self.proof = Some(proof);
        self
    }
    
    pub fn to_json(&self) -> WalletResult<String> {
        serde_json::to_string(self)
            .map_err(|e| WalletError::SerializationError(format!("Failed to serialize presentation: {}", e)))
    }
} 