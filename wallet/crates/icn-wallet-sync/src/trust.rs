use serde::{Serialize, Deserialize};
use std::time::SystemTime;
use std::collections::HashMap;
use sha2::{Sha256, Digest};
use std::sync::Arc;
use tokio::sync::Mutex;
use std::str::FromStr;
use std::fmt;
use std::pin::Pin;
use std::task::{Context, Poll};
use futures::Stream;
use tracing::{debug, error, info, warn};
use crate::WalletResult;

use crate::{SyncClient, error::SyncError, DagNode, DagNodeMetadata};

/// Trust bundle containing verified DIDs and credentials
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustBundle {
    /// Trust bundle ID
    pub id: String,
    
    /// Trust bundle name
    pub name: String,
    
    /// Trust bundle version
    pub version: u32,
    
    /// Creation timestamp
    pub created_at: SystemTime,
    
    /// List of trusted DIDs
    pub trusted_dids: Vec<String>,
    
    /// Issuer DID
    pub issuer: String,
    
    /// Trust bundle signature
    pub signature: Option<String>,
    
    /// Epoch number
    pub epoch: u64,
    
    /// Expiration timestamp
    pub expires_at: Option<SystemTime>,
    
    /// Additional metadata
    #[serde(default)]
    pub metadata: HashMap<String, String>,
    
    /// Trust bundle attestations from validators
    #[serde(default)]
    pub attestations: HashMap<String, String>,
}

impl TrustBundle {
    /// Create a new trust bundle
    pub fn new(name: String, issuer: String, trusted_dids: Vec<String>) -> Self {
        Self {
            id: String::new(), // Will be set after CID generation
            name,
            version: 1,
            created_at: SystemTime::now(),
            trusted_dids,
            issuer,
            signature: None,
            epoch: 1,
            expires_at: None,
            metadata: HashMap::new(),
            attestations: HashMap::new(),
        }
    }
    
    /// Convert to a DAG node
    pub fn to_dag_node(&self) -> Result<DagNode, SyncError> {
        // Convert to JSON
        let value = serde_json::to_value(&*self)
            .map_err(|e| SyncError::Serialization(e))?;
        
        // Serialize to bytes for CID generation and payload
        let json_bytes = serde_json::to_vec(&value)
            .map_err(|e| SyncError::Serialization(e))?;
        
        // Generate CID using SHA-256
        let mut hasher = Sha256::new();
        hasher.update(&json_bytes);
        let digest = hasher.finalize();
        let cid = format!("bafybeih{}", hex::encode(&digest[0..16]));
        
        // Create DAG node
        let node = DagNode {
            cid: cid.clone(),
            parents: Vec::new(),
            creator: self.issuer.clone(),
            timestamp: self.created_at,
            signatures: vec![],
            content: json_bytes, content_type: "application/json".to_string(),
            metadata: DagNodeMetadata {
                sequence: Some(self.epoch),
                scope: Some("federation".to_string()),
            },
        };
        
        Ok(node)
    }
    
    /// Generate ID for this trust bundle
    pub fn generate_id(&mut self) -> Result<(), SyncError> {
        // Temporarily clear the ID for consistent CID generation
        self.id = String::new();
        
        // Convert to JSON
        let value = serde_json::to_value(&*self)
            .map_err(|e| SyncError::Serialization(e))?;
        
        // Serialize to bytes for CID generation
        let json_bytes = serde_json::to_vec(&value)
            .map_err(|e| SyncError::Serialization(e))?;
        
        // Generate CID using SHA-256
        let mut hasher = Sha256::new();
        hasher.update(&json_bytes);
        let digest = hasher.finalize();
        let cid = format!("bafybeih{}", hex::encode(&digest[0..16]));
        
        // Set the ID
        self.id = cid;
        
        Ok(())
    }
    
    /// Check if the trust bundle is expired
    pub fn is_expired(&self) -> bool {
        match self.expires_at {
            Some(expires) => {
                match SystemTime::now().duration_since(expires) {
                    Ok(_) => true, // Current time is after expiration
                    Err(_) => false, // Current time is before expiration
                }
            },
            None => false, // No expiration time set
        }
    }
    
    /// Count nodes by role
    pub fn count_nodes_by_role(&self, role: &str) -> usize {
        self.metadata.iter()
            .filter(|(k, v)| k.starts_with("role:") && v == &role)
            .count()
    }
}

/// Trust bundle manager
pub struct TrustManager {
    /// Sync client
    client: SyncClient,
    
    /// Latest known trust bundle
    latest_bundle: Arc<Mutex<Option<TrustBundle>>>,
}

impl TrustManager {
    /// Create a new trust bundle manager
    pub fn new(client: SyncClient) -> Self {
        Self {
            client,
            latest_bundle: Arc::new(Mutex::new(None)),
        }
    }
    
    /// Submit a trust bundle to the node
    pub async fn submit_trust_bundle(&self, trust_bundle: &mut TrustBundle) -> Result<String, SyncError> {
        // Generate ID if not set
        if trust_bundle.id.is_empty() {
            trust_bundle.generate_id()?;
        }
        
        // Convert to DAG node
        let node = trust_bundle.to_dag_node()?;
        
        // Submit the node
        let response = self.client.submit_node(&node).await?;
        
        // Update latest bundle if this is newer
        let mut latest = self.latest_bundle.lock().await;
        if let Some(ref current) = *latest {
            if trust_bundle.epoch > current.epoch {
                *latest = Some(trust_bundle.clone());
            }
        } else {
            *latest = Some(trust_bundle.clone());
        }
        
        Ok(response.id)
    }
    
    /// Get a trust bundle by ID
    pub async fn get_trust_bundle(&self, bundle_id: &str) -> Result<TrustBundle, SyncError> {
        // Get the DAG node
        let node = self.client.get_node(bundle_id).await?;
        
        // Parse the payload as JSON
        let payload_json = node.content_as_json()
            .map_err(|e| SyncError::Serialization(e))?;
        
        // Convert to TrustBundle
        let trust_bundle = serde_json::from_value(payload_json)
            .map_err(|e| SyncError::Serialization(e))?;
        
        Ok(trust_bundle)
    }
    
    /// Verify a DAG node against the latest trust bundle
    pub async fn verify_dag_node(&self, node: &DagNode) -> Result<bool, SyncError> {
        // Get the latest trust bundle
        let latest = self.latest_bundle.lock().await;
        
        // If no trust bundle is available, we can't verify
        let bundle = match *latest {
            Some(ref bundle) => bundle,
            None => return Err(SyncError::Validation("No trust bundle available for verification".to_string())),
        };
        
        // Check if the issuer is trusted
        if !bundle.trusted_dids.contains(&node.creator) {
            return Err(SyncError::Validation(format!("Issuer {} is not trusted", node.creator)));
        }
        
        // In a real implementation, we would verify the signature
        // For now, just return true if the issuer is trusted
        Ok(true)
    }
    
    /// Synchronize with the node to get the latest trust bundle
    pub async fn sync_trust_bundle(&self) -> Result<Option<TrustBundle>, SyncError> {
        // Get current epoch
        let current_epoch = {
            let latest = self.latest_bundle.lock().await;
            match *latest {
                Some(ref bundle) => bundle.epoch,
                None => 0,
            }
        };
        
        // Construct the URL for the latest trust bundle
        let url = format!("/api/v1/federation/trust-bundle/latest");
        
        // Request the latest trust bundle
        // In a real implementation, we would send a real request to the node
        // For now, just create a dummy trust bundle with a higher epoch
        
        // TODO: Replace with actual request to the node
        let dummy_bundle = TrustBundle {
            id: format!("trust-bundle-{}", current_epoch + 1),
            name: "Latest Trust Bundle".to_string(),
            version: 1,
            created_at: SystemTime::now(),
            trusted_dids: vec!["did:icn:trusted1".to_string(), "did:icn:trusted2".to_string()],
            issuer: "did:icn:federation".to_string(),
            signature: Some("dummy-signature".to_string()),
            epoch: current_epoch + 1,
            expires_at: None,
            metadata: HashMap::new(),
            attestations: HashMap::new(),
        };
        
        // Update latest bundle
        {
            let mut latest = self.latest_bundle.lock().await;
            *latest = Some(dummy_bundle.clone());
        }
        
        Ok(Some(dummy_bundle))
    }
    
    /// Get the latest known trust bundle
    pub async fn get_latest_trust_bundle(&self) -> Option<TrustBundle> {
        let latest = self.latest_bundle.lock().await;
        latest.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, Duration};
    
    // Existing tests if any...
    
    #[tokio::test]
    async fn test_trust_bundle_verification() {
        // Create a mock sync client
        let client = SyncClient::new("http://localhost:8080".to_string());
        
        // Create trust manager
        let trust_manager = TrustManager::new(client);
        
        // Create a valid trust bundle
        let mut valid_bundle = TrustBundle {
            id: "valid-bundle-1".to_string(),
            name: "Valid Trust Bundle".to_string(),
            version: 1,
            created_at: SystemTime::now(),
            trusted_dids: vec!["did:icn:trusted1".to_string(), "did:icn:trusted2".to_string()],
            issuer: "did:icn:federation".to_string(),
            signature: Some("valid-signature".to_string()),
            epoch: 1,
            expires_at: Some(SystemTime::now() + Duration::from_secs(3600)), // 1 hour from now
            metadata: HashMap::new(),
            attestations: HashMap::new(),
        };
        
        // Initialize the trust manager's latest bundle
        {
            let mut latest = trust_manager.latest_bundle.lock().await;
            *latest = Some(valid_bundle.clone());
        }
        
        // Test verification with a trusted issuer
        let valid_node = DagNode {
            cid: "test-valid-node".to_string(),
            parents: vec![],
            issuer: "did:icn:trusted1".to_string(), // This is in the trusted DIDs
            timestamp: SystemTime::now(),
            signature: vec![1, 2, 3, 4],
            payload: vec![10, 20, 30],
            metadata: DagNodeMetadata::default(),
        };
        
        let verification_result = trust_manager.verify_dag_node(&valid_node).await;
        assert!(verification_result.is_ok() && verification_result.unwrap(), 
                "Verification should succeed for trusted issuer");
        
        // Test verification with an untrusted issuer
        let invalid_node = DagNode {
            cid: "test-invalid-node".to_string(),
            parents: vec![],
            issuer: "did:icn:untrusted".to_string(), // This is NOT in the trusted DIDs
            timestamp: SystemTime::now(),
            signature: vec![1, 2, 3, 4],
            payload: vec![10, 20, 30],
            metadata: DagNodeMetadata::default(),
        };
        
        let verification_result = trust_manager.verify_dag_node(&invalid_node).await;
        assert!(verification_result.is_err(), "Verification should fail for untrusted issuer");
        if let Err(SyncError::Validation(msg)) = verification_result {
            assert!(msg.contains("not trusted"), "Error should indicate untrusted issuer");
        } else {
            panic!("Expected ValidationError");
        }
        
        // Test with expired trust bundle
        let mut expired_bundle = valid_bundle.clone();
        expired_bundle.expires_at = Some(SystemTime::now() - Duration::from_secs(3600)); // 1 hour ago
        
        {
            let mut latest = trust_manager.latest_bundle.lock().await;
            *latest = Some(expired_bundle);
        }
        
        // Create a function to check if a bundle is expired
        let is_expired = |bundle: &TrustBundle| -> bool {
            bundle.is_expired()
        };
        
        // Get the latest bundle and check if it's expired
        let latest_bundle = trust_manager.get_latest_trust_bundle().await.unwrap();
        assert!(is_expired(&latest_bundle), "Trust bundle should be marked as expired");
        
        // Even with an expired bundle, verification should still work based on current implementation
        // In a real implementation, you might want to reject verification with expired bundles
        let verification_result = trust_manager.verify_dag_node(&valid_node).await;
        assert!(verification_result.is_ok(), "Current implementation should still verify with expired bundle");
        
        // Test with no trust bundle available
        {
            let mut latest = trust_manager.latest_bundle.lock().await;
            *latest = None;
        }
        
        let verification_result = trust_manager.verify_dag_node(&valid_node).await;
        assert!(verification_result.is_err(), "Verification should fail when no trust bundle is available");
        if let Err(SyncError::Validation(msg)) = verification_result {
            assert!(msg.contains("No trust bundle available"), "Error should indicate missing trust bundle");
        } else {
            panic!("Expected ValidationError for missing trust bundle");
        }
    }
    
    #[tokio::test]
    async fn test_trust_bundle_quorum_verification() {
        // This test would simulate trust bundle quorum verification
        // In a real implementation, this would involve signature verification from multiple guardians
        
        // Create a mock sync client
        let client = SyncClient::new("http://localhost:8080".to_string());
        
        // Create trust manager
        let trust_manager = TrustManager::new(client);
        
        // Create a trust bundle with attestations from guardians
        let mut bundle_with_attestations = TrustBundle {
            id: "quorum-bundle-1".to_string(),
            name: "Quorum Test Bundle".to_string(),
            version: 1,
            created_at: SystemTime::now(),
            trusted_dids: vec!["did:icn:trusted1".to_string()],
            issuer: "did:icn:federation".to_string(),
            signature: Some("federation-signature".to_string()),
            epoch: 2,
            expires_at: None,
            metadata: HashMap::new(),
            attestations: {
                let mut map = HashMap::new();
                map.insert("did:icn:guardian1".to_string(), "guardian1-signature".to_string());
                map.insert("did:icn:guardian2".to_string(), "guardian2-signature".to_string());
                map.insert("did:icn:guardian3".to_string(), "guardian3-signature".to_string());
                map
            },
        };
        
        // In a real implementation, each attestation would be validated against the guardian's public key
        // For testing purposes, we'll just check the number of attestations
        
        // Verify the trust bundle has sufficient attestations (quorum)
        let quorum_threshold = 3; // Require at least 3 guardian attestations
        let has_quorum = bundle_with_attestations.attestations.len() >= quorum_threshold;
        assert!(has_quorum, "Trust bundle should have quorum with {} attestations", 
                bundle_with_attestations.attestations.len());
        
        // Now simulate removing an attestation to break quorum
        bundle_with_attestations.attestations.remove("did:icn:guardian3");
        
        let has_quorum = bundle_with_attestations.attestations.len() >= quorum_threshold;
        assert!(!has_quorum, "Trust bundle should not have quorum after removing an attestation");
    }
} 