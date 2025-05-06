use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use icn_identity::{IdentityId, KeyPair, Signature, VerifiableCredential};
use crate::error::{FederationError, FederationResult};
use crate::quorum::SignerQuorumConfig;
use crate::quorum::QuorumType as QuorumTypeConfig;

/// Represents a federation signer with their ID and key information
#[derive(Debug, Clone)]
pub struct Signer {
    /// The DID of the signer
    pub did: IdentityId,
    /// The keypair for the signer
    pub keypair: KeyPair,
    /// Optional credential for the signer
    pub credential: Option<VerifiableCredential>,
}

/// Serializable version of Signer for recovery events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerializableSigner {
    /// Signer DID
    pub did: String,
    /// Signer Public Key (base64 encoded)
    pub public_key: String,
}

impl From<&Signer> for SerializableSigner {
    fn from(signer: &Signer) -> Self {
        Self {
            did: signer.did.0.clone(),
            // In a real implementation, we would extract the public key
            public_key: "placeholder_key".to_string(),
        }
    }
}

/// Functions for initializing signers
pub mod initialization {
    use super::*;
    use std::collections::HashSet;
    
    /// Generate a single signer with a new DID and keypair
    pub async fn generate_signer() -> FederationResult<(Signer, KeyPair)> {
        // Create a new keypair
        let keypair = KeyPair::generate_random();
        
        // Generate a DID for the signer
        let signer_did = format!("did:icn:signer:{}", Utc::now().timestamp_millis());
        
        // Create the signer
        let signer = Signer {
            did: IdentityId(signer_did),
            keypair: keypair.clone(),
            credential: None,
        };
        
        Ok((signer, keypair))
    }
    
    /// Initialize a set of signers with the specified quorum configuration
    pub async fn initialize_signer_set(count: usize, quorum_type: QuorumTypeConfig) -> FederationResult<(Vec<Signer>, SignerQuorumConfig)> {
        // Generate the specified number of signers
        let mut signers = Vec::with_capacity(count);
        let mut signer_dids = HashSet::new();
        
        for _ in 0..count {
            let (signer, _) = generate_signer().await?;
            
            // Ensure DIDs are unique
            if signer_dids.contains(&signer.did.0) {
                return Err(FederationError::BootstrapError(
                    format!("Duplicate signer DID generated: {}", signer.did.0)
                ));
            }
            
            signer_dids.insert(signer.did.0.clone());
            signers.push(signer);
        }
        
        // Create quorum configuration with all signer DIDs
        let signer_did_strings: Vec<String> = signers.iter()
            .map(|s| s.did.0.clone())
            .collect();
        
        let quorum_config = SignerQuorumConfig::new(
            quorum_type,
            signer_did_strings,
        );
        
        Ok((signers, quorum_config))
    }
    
    /// Create a verifiable credential for a signer
    pub async fn create_signer_credential(
        signer: &Signer,
        issuer_did: &str,
        issuer_keypair: &KeyPair,
    ) -> FederationResult<VerifiableCredential> {
        // Create credential using the VerifiableCredential::new method
        let issuer_id = IdentityId(issuer_did.to_string());
        let subject_id = signer.did.clone();
        
        let claims = serde_json::json!({
            "role": "federationSigner",
            "issuedAt": Utc::now().to_rfc3339(),
        });
        
        let credential = VerifiableCredential::new(
            vec!["VerifiableCredential".to_string(), "SignerCredential".to_string()],
            &issuer_id,
            &subject_id,
            claims
        );
        
        // In a real implementation, we would sign the credential with the issuer's key
        // credential = sign_credential(credential, issuer_did, issuer_keypair).await?;
        
        Ok(credential)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_signer_generation() {
        let result = initialization::generate_signer().await;
        assert!(result.is_ok());
        
        let (signer, keypair) = result.unwrap();
        assert!(signer.did.0.starts_with("did:icn:signer:"));
    }
    
    #[tokio::test]
    async fn test_signer_set_initialization() {
        let count = 3;
        let quorum_type = QuorumTypeConfig::Majority;
        
        let result = initialization::initialize_signer_set(count, quorum_type).await;
        assert!(result.is_ok());
        
        let (signers, quorum_config) = result.unwrap();
        assert_eq!(signers.len(), count);
        assert_eq!(quorum_config.signers.len(), count);
        assert_eq!(quorum_config.quorum_type, QuorumTypeConfig::Majority);
    }
} 