use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use icn_identity::{IdentityId, KeyPair, Signature, VerifiableCredential};
use crate::error::{FederationError, FederationResult};
use crate::quorum::SignerQuorumConfig;

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

/// Quorum type for decisions
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuorumType {
    /// Simple majority (>50%)
    Majority,
    /// Specified threshold percentage (0-100)
    Threshold(u8),
    /// Unanimous agreement
    Unanimous,
}

/// Functions for initializing signers
pub mod initialization {
    use super::*;
    use std::collections::HashSet;
    
    /// Generate a single signer with a new DID and keypair
    pub async fn generate_signer() -> FederationResult<(Signer, KeyPair)> {
        // Create a new keypair
        let keypair = KeyPair::new();
        
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
    pub async fn initialize_signer_set(count: usize, quorum_type: QuorumType) -> FederationResult<(Vec<Signer>, SignerQuorumConfig)> {
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
        // Create a basic verifiable credential
        let mut credential = VerifiableCredential {
            context: vec!["https://www.w3.org/2018/credentials/v1".to_string()],
            id: Some(format!("urn:uuid:{}", uuid::Uuid::new_v4())),
            type_: vec![
                "VerifiableCredential".to_string(),
                "SignerCredential".to_string(),
            ],
            issuer: issuer_did.to_string(),
            issuanceDate: Utc::now().to_rfc3339(),
            credentialSubject: serde_json::json!({
                "id": signer.did.0,
                "role": "federationSigner",
                "issuedAt": Utc::now().to_rfc3339(),
            }),
            proof: None,
        };
        
        // In a real implementation, we would sign the credential with the issuer's key
        // credential.proof = create_proof(credential, issuer_keypair);
        
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
        let quorum_type = QuorumType::Majority;
        
        let result = initialization::initialize_signer_set(count, quorum_type).await;
        assert!(result.is_ok());
        
        let (signers, quorum_config) = result.unwrap();
        assert_eq!(signers.len(), count);
        assert_eq!(quorum_config.signers.len(), count);
        assert_eq!(quorum_config.quorum_type, QuorumType::Majority);
    }
} 