use serde::{Serialize, Deserialize};
use serde_json::Value;
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use crate::error::{WalletResult, WalletError};
use crate::identity::IdentityWallet;
use crate::vc::{VerifiableCredential, CredentialProof, VerifiablePresentation};

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

pub struct CredentialSigner {
    wallet: IdentityWallet,
}

impl CredentialSigner {
    pub fn new(wallet: IdentityWallet) -> Self {
        Self { wallet }
    }
    
    pub fn issue_credential(&self, subject_data: Value, credential_types: Vec<String>) -> WalletResult<VerifiableCredential> {
        let issuance_date = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        
        // Create credential without proof
        let credential = VerifiableCredential::new(
            self.wallet.did.to_string(),
            subject_data,
            credential_types,
        );
        
        // Sign the credential
        let unsigned_json = credential.to_json()?;
        let signature = self.wallet.sign_message(unsigned_json.as_bytes());
        
        // Create JWS
        let jws = format!("eyJhbGciOiJFZERTQSJ9..{}",
            BASE64.encode(signature));
            
        // Add proof
        let proof = CredentialProof {
            proof_type: "Ed25519Signature2020".to_string(),
            created: issuance_date,
            verification_method: format!("{}#keys-1", self.wallet.did),
            proof_purpose: "assertionMethod".to_string(),
            jws,
        };
        
        Ok(credential.with_proof(proof))
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
        
        let unsigned_json = credential_copy.to_json()?;
            
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
    
    pub fn create_presentation(&self, credentials: Vec<VerifiableCredential>) -> WalletResult<VerifiablePresentation> {
        let presentation = VerifiablePresentation::new(
            self.wallet.did.to_string(),
            credentials,
        );
        
        // Sign the presentation
        let unsigned_json = presentation.to_json()?;
        let signature = self.wallet.sign_message(unsigned_json.as_bytes());
        
        // Create JWS
        let jws = format!("eyJhbGciOiJFZERTQSJ9..{}",
            BASE64.encode(signature));
            
        // Create proof
        let issuance_date = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let proof = CredentialProof {
            proof_type: "Ed25519Signature2020".to_string(),
            created: issuance_date,
            verification_method: format!("{}#keys-1", self.wallet.did),
            proof_purpose: "authentication".to_string(),
            jws,
        };
        
        Ok(presentation.with_proof(proof))
    }
} 