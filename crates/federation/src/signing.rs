//! Guardian Mandate signing helpers

use sha2::{Sha256, Digest};
use icn_identity::{
    IdentityId, IdentityScope, KeyPair, Signature, IdentityError,
    QuorumProof, QuorumConfig, TrustBundle
};
use icn_dag::DagNode;
use crate::{GuardianMandate, FederationResult, FederationError};

/// Calculate a consistent hash for mandate content.
///
/// This provides a standardized way to create a hash over the mandate content
/// for signing and verification purposes.
pub fn calculate_mandate_hash(
    action: &str,
    reason: &str,
    scope: &IdentityScope,
    scope_id: &IdentityId,
    guardian: &IdentityId,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    
    // Ensure we hash all elements in a consistent order
    hasher.update(action.as_bytes());
    hasher.update(reason.as_bytes());
    hasher.update(format!("{:?}", scope).as_bytes()); // Using Debug formatting for the enum
    hasher.update(scope_id.0.as_bytes());
    hasher.update(guardian.0.as_bytes());
    
    let result = hasher.finalize();
    let mut hash = [0u8; 32];
    hash.copy_from_slice(&result);
    
    hash
}

/// Sign a mandate hash with the provided keypair.
///
/// This is a wrapper around the identity signing function to make the
/// mandate signing process more ergonomic.
pub async fn sign_mandate_hash(
    hash: &[u8],
    keypair: &KeyPair,
) -> Result<Signature, IdentityError> {
    icn_identity::sign_message(hash, keypair)
}

/// Builder for creating signed guardian mandates
pub struct MandateBuilder {
    scope: IdentityScope,
    scope_id: IdentityId,
    action: String,
    reason: String,
    guardian: IdentityId,
    quorum_config: QuorumConfig,
    signing_guardians: Vec<(IdentityId, KeyPair)>,
    dag_node: Option<DagNode>,
}

impl MandateBuilder {
    /// Create a new mandate builder
    pub fn new(
        scope: IdentityScope,
        scope_id: IdentityId,
        action: String,
        reason: String,
        guardian: IdentityId,
    ) -> Self {
        Self {
            scope,
            scope_id,
            action,
            reason,
            guardian,
            quorum_config: QuorumConfig::Majority,
            signing_guardians: Vec::new(),
            dag_node: None,
        }
    }
    
    /// Set the quorum configuration
    pub fn with_quorum_config(mut self, config: QuorumConfig) -> Self {
        self.quorum_config = config;
        self
    }
    
    /// Add a signing guardian
    pub fn add_signer(mut self, id: IdentityId, keypair: KeyPair) -> Self {
        self.signing_guardians.push((id, keypair));
        self
    }
    
    /// Set the DAG node to store the mandate
    pub fn with_dag_node(mut self, dag_node: DagNode) -> Self {
        self.dag_node = Some(dag_node);
        self
    }
    
    /// Create the signed mandate
    pub async fn build(self) -> FederationResult<GuardianMandate> {
        let dag_node = self.dag_node.ok_or_else(|| 
            FederationError::InvalidMandate("DAG node is required".to_string())
        )?;
        
        create_signed_mandate(
            self.scope,
            self.scope_id,
            self.action,
            self.reason,
            self.guardian,
            self.quorum_config,
            &self.signing_guardians,
            dag_node,
        ).await
    }
}

/// Create a signed guardian mandate with the provided quorum of signatures.
///
/// This function simulates the process of collecting signatures from multiple
/// guardians to create a valid mandate. In a real world scenario, this would
/// involve a distributed process of collecting signatures from guardians.
/// 
/// **Note**: Consider using `MandateBuilder` for a more ergonomic API.
#[allow(clippy::too_many_arguments)]
pub async fn create_signed_mandate(
    // Mandate details
    scope: IdentityScope,
    scope_id: IdentityId,
    action: String,
    reason: String,
    guardian: IdentityId,
    // Quorum details
    quorum_config: QuorumConfig,
    // Signing guardians - each tuple represents a guardian's DID and their keypair
    signing_guardians: &[(IdentityId, KeyPair)],
    // DAG node to store the mandate content
    dag_node: DagNode,
) -> FederationResult<GuardianMandate> {
    // Calculate the mandate hash for signatures
    let mandate_hash = calculate_mandate_hash(&action, &reason, &scope, &scope_id, &guardian);
    
    // Collect signatures from guardians
    let mut votes = Vec::with_capacity(signing_guardians.len());
    
    for (guardian_id, keypair) in signing_guardians {
        match sign_mandate_hash(&mandate_hash, keypair).await {
            Ok(signature) => {
                votes.push((guardian_id.clone(), signature));
            },
            Err(e) => {
                return Err(FederationError::InvalidMandate(
                    format!("Failed to collect signature from guardian {}: {}", guardian_id.0, e)
                ));
            }
        }
    }
    
    // Create the quorum proof
    let quorum_proof = QuorumProof {
        votes,
        config: quorum_config,
    };
    
    // Create the mandate
    let mandate = GuardianMandate::new(
        scope,
        scope_id,
        action,
        reason,
        guardian,
        quorum_proof,
        dag_node,
    ).await;
    
    Ok(mandate)
}

/// Create a signed TrustBundle with a valid QuorumProof.
///
/// This function helps create a TrustBundle with a proper QuorumProof for testing
/// and future logic.
pub async fn create_signed_trust_bundle(
    // TrustBundle content
    bundle: &mut TrustBundle,
    // Quorum configuration
    quorum_config: QuorumConfig,
    // Signing guardians - each tuple represents a guardian's DID and their keypair
    signing_guardians: &[(IdentityId, &KeyPair)],
) -> FederationResult<()> {
    // Calculate the bundle hash for signatures
    let bundle_hash = bundle.calculate_hash();
    
    // Collect signatures from guardians
    let mut votes = Vec::with_capacity(signing_guardians.len());
    
    for (guardian_id, keypair) in signing_guardians {
        match icn_identity::sign_message(&bundle_hash, keypair) {
            Ok(signature) => {
                votes.push((guardian_id.clone(), signature));
            },
            Err(e) => {
                return Err(FederationError::InvalidMandate(
                    format!("Failed to collect signature from guardian {}: {}", guardian_id.0, e)
                ));
            }
        }
    }
    
    // Create the quorum proof
    let quorum_proof = QuorumProof {
        votes,
        config: quorum_config,
    };
    
    // Set the proof on the bundle
    bundle.proof = Some(quorum_proof);
    
    Ok(())
} 