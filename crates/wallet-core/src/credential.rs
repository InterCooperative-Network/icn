use serde::{Serialize, Deserialize};
use serde_json::Value;
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use crate::error::{WalletResult, WalletError};
use crate::identity::IdentityWallet;
use crate::vc::{VerifiableCredential as VcCredential, CredentialProof, VerifiablePresentation};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletCredential {
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

impl WalletCredential {
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
    
    // Convert to vc::VerifiableCredential
    pub fn to_vc_credential(&self) -> VcCredential {
        let mut vc = VcCredential::new(
            self.issuer.clone(),
            self.credential_subject.clone(),
            self.credential_type.clone(),
        );
        
        if let Some(proof) = &self.proof {
            vc = vc.with_proof(proof.clone());
        }
        
        vc
    }
}

pub struct CredentialSigner {
    wallet: IdentityWallet,
}

impl CredentialSigner {
    pub fn new(wallet: IdentityWallet) -> Self {
        Self { wallet }
    }
    
    pub fn issue_credential(&self, subject_data: Value, credential_types: Vec<String>) -> WalletResult<WalletCredential> {
        let issuance_date = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        
        // Create credential without proof
        let credential = WalletCredential::new(
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
        let proof = CredentialProof::new(
            "Ed25519Signature2020".to_string(),
            issuance_date,
            format!("{}#keys-1", self.wallet.did),
            "assertionMethod".to_string(),
            jws
        );
        
        Ok(credential.with_proof(proof))
    }
    
    pub fn verify_credential(&self, credential: &WalletCredential) -> WalletResult<bool> {
        // Check if the credential has a proof
        let proof = match &credential.proof {
            Some(p) => p,
            None => return Ok(false),
        };
        
        // Create a copy without the proof for verification
        let credential_copy = WalletCredential {
            context: credential.context.clone(),
            credential_type: credential.credential_type.clone(),
            issuer: credential.issuer.clone(),
            issuance_date: credential.issuance_date.clone(),
            credential_subject: credential.credential_subject.clone(),
            proof: None,
        };
        
        let unsigned_json = credential_copy.to_json()?;
            
        // Extract the signature from proof
        // Use the accessor method to get the JWS
        let jws = proof.get_jws();
        let jws_parts: Vec<&str> = jws.split('.').collect();
        let signature_base64 = if jws_parts.len() >= 3 {
            jws_parts[2]
        } else {
            return Err(WalletError::CryptoError("Invalid JWS format".to_string()));
        };
            
        // Decode signature
        let signature_bytes = BASE64.decode(signature_base64)
            .map_err(|e| WalletError::CryptoError(format!("Invalid signature encoding: {}", e)))?;
            
        // Verify the signature
        self.wallet.verify_message(unsigned_json.as_bytes(), &signature_bytes)
    }
    
    pub fn create_selective_disclosure(&self, credential: &WalletCredential, fields_to_disclose: Vec<String>) -> WalletResult<WalletCredential> {
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
    
    pub fn create_presentation(&self, credentials: Vec<WalletCredential>) -> WalletResult<VerifiablePresentation> {
        // Convert WalletCredential to VcCredential
        let vc_credentials: Vec<VcCredential> = credentials.iter()
            .map(|cred| cred.to_vc_credential())
            .collect();
            
        let presentation = VerifiablePresentation::new(
            self.wallet.did.to_string(),
            vc_credentials,
        );
        
        // Sign the presentation
        let unsigned_json = presentation.to_json()?;
        let signature = self.wallet.sign_message(unsigned_json.as_bytes());
        
        // Create JWS
        let jws = format!("eyJhbGciOiJFZERTQSJ9..{}",
            BASE64.encode(signature));
            
        // Create proof
        let issuance_date = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let proof = CredentialProof::new(
            "Ed25519Signature2020".to_string(),
            issuance_date,
            format!("{}#keys-1", self.wallet.did),
            "authentication".to_string(),
            jws
        );
        
        Ok(presentation.with_proof(proof))
    }
} 