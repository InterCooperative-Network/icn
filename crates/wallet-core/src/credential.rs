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

pub struct CredentialSigner {
    wallet: IdentityWallet,
}

impl CredentialSigner {
    pub fn new(wallet: IdentityWallet) -> Self {
        Self { wallet }
    }
    
    pub fn issue_credential(&self, subject_data: Value, credential_types: Vec<String>) -> WalletResult<VerifiableCredential> {
        let issuance_date = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        
        let mut credential = VerifiableCredential {
            context: vec![
                "https://www.w3.org/2018/credentials/v1".to_string(),
                "https://www.w3.org/2018/credentials/examples/v1".to_string(),
            ],
            credential_type: vec!["VerifiableCredential".to_string()],
            issuer: self.wallet.did.to_string(),
            issuance_date: issuance_date.clone(),
            credential_subject: subject_data,
            proof: None,
        };
        
        // Add additional credential types
        credential.credential_type.extend(credential_types);
        
        // Sign the credential
        let unsigned_json = serde_json::to_string(&credential)
            .map_err(|e| WalletError::SerializationError(format!("Failed to serialize credential: {}", e)))?;
            
        let signature = self.wallet.sign_message(unsigned_json.as_bytes());
        
        // Create JWS
        let jws = format!("eyJhbGciOiJFZERTQSJ9..{}",
            BASE64.encode(signature));
            
        // Add proof
        credential.proof = Some(CredentialProof {
            proof_type: "Ed25519Signature2020".to_string(),
            created: issuance_date,
            verification_method: format!("{}#keys-1", self.wallet.did),
            proof_purpose: "assertionMethod".to_string(),
            jws,
        });
        
        Ok(credential)
    }
    
    pub fn verify_credential(&self, credential: &VerifiableCredential) -> WalletResult<bool> {
        // Check if the credential has a proof
        let proof = match &credential.proof {
            Some(p) => p,
            None => return Ok(false),
        };
        
        // Create a copy without the proof for verification
        let credential_copy = VerifiableCredential {
            context: credential.context.clone(),
            credential_type: credential.credential_type.clone(),
            issuer: credential.issuer.clone(),
            issuance_date: credential.issuance_date.clone(),
            credential_subject: credential.credential_subject.clone(),
            proof: None,
        };
        
        let unsigned_json = serde_json::to_string(&credential_copy)
            .map_err(|e| WalletError::SerializationError(format!("Failed to serialize credential: {}", e)))?;
            
        // Extract JWS parts
        let jws_parts: Vec<&str> = proof.jws.split('.').collect();
        if jws_parts.len() != 3 {
            return Err(WalletError::CryptoError("Invalid JWS format".to_string()));
        }
        
        // Decode signature
        let signature_bytes = BASE64.decode(jws_parts[2])
            .map_err(|e| WalletError::CryptoError(format!("Invalid signature encoding: {}", e)))?;
            
        // Verify the signature
        self.wallet.verify_message(unsigned_json.as_bytes(), &signature_bytes)
    }
    
    pub fn create_selective_disclosure(&self, credential: &VerifiableCredential, fields_to_disclose: Vec<String>) -> WalletResult<VerifiableCredential> {
        // Create a copy of the credential
        let mut subject_data = credential.credential_subject.clone();
        
        // Filter out fields not in the disclosure list
        if let Value::Object(ref mut map) = subject_data {
            let keys: Vec<String> = map.keys().cloned().collect();
            for key in keys {
                if !fields_to_disclose.contains(&key) {
                    map.remove(&key);
                }
            }
        }
        
        // Create a new credential with only the disclosed fields
        self.issue_credential(subject_data, credential.credential_type.clone())
    }
} 