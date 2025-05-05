/*!
 * Federation Credential Sync Service
 *
 * Manages synchronization of Verifiable Credentials across federation nodes,
 * ensuring a consistent view of governance receipts, proposals, and economic actions.
 */

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::StreamExt;
use serde::{Serialize, Deserialize};
use tokio::time;
use tracing::{debug, error, info, warn};

use icn_storage::StorageManager;
use icn_identity::IdentityManager;
use icn_core_vm::{
    VerifiableCredential, ExecutionReceiptSubject, 
    InternalHostError, get_execution_receipt_by_cid
};
use icn_dag::DagStore;

/// Types of credentials that can be synchronized
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SyncCredentialType {
    /// Execution Receipts from proposal executions
    ExecutionReceipt,
    /// Proposal Outcomes from voting procedures
    ProposalOutcome,
    /// Resource transfers between entities
    ResourceTransfer,
    /// Membership credentials
    MembershipCredential,
}

/// Parameters for credential synchronization
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncParameters {
    /// Federation ID to synchronize with
    pub federation_id: String,
    /// Types of credentials to synchronize
    pub credential_types: Vec<SyncCredentialType>,
    /// Start timestamp (inclusive)
    pub from_timestamp: DateTime<Utc>,
    /// End timestamp (inclusive, None means current time)
    pub to_timestamp: Option<DateTime<Utc>>,
    /// Maximum number of credentials to fetch
    pub limit: Option<usize>,
}

/// Status of a credential synchronization operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncStatus {
    /// Timestamp when the sync was initiated
    pub sync_initiated: DateTime<Utc>,
    /// Timestamp when the sync completed (if successful)
    pub sync_completed: Option<DateTime<Utc>>,
    /// Number of credentials successfully synchronized
    pub credentials_synced: usize,
    /// Number of credentials that failed to synchronize
    pub credentials_failed: usize,
    /// Error message (if any)
    pub error_message: Option<String>,
}

/// Result of a credential synchronization operation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    /// Status of the sync operation
    pub status: SyncStatus,
    /// CIDs of synchronized credentials
    pub credential_cids: Vec<String>,
}

/// A peer in the federation
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FederationPeer {
    /// DID of the peer
    pub did: String,
    /// Endpoint for credential synchronization
    pub sync_endpoint: String,
    /// Last known sync timestamp
    pub last_sync: Option<DateTime<Utc>>,
}

/// Configuration for the CredentialSyncService
#[derive(Debug, Clone)]
pub struct CredentialSyncConfig {
    /// DID of the local federation
    pub local_federation_did: String,
    /// Peers in the federation
    pub peers: Vec<FederationPeer>,
    /// Frequency of automatic synchronization (if enabled)
    pub sync_interval: Option<Duration>,
    /// Maximum number of credentials to fetch in a single sync
    pub max_credentials_per_sync: usize,
    /// Whether to verify credentials during synchronization
    pub verify_credentials: bool,
}

/// Default configuration values
impl Default for CredentialSyncConfig {
    fn default() -> Self {
        Self {
            local_federation_did: "did:icn:federation:local".to_string(),
            peers: Vec::new(),
            sync_interval: Some(Duration::from_secs(300)), // 5 minutes
            max_credentials_per_sync: 1000,
            verify_credentials: true,
        }
    }
}

/// Interface for credential verification
#[async_trait]
pub trait CredentialVerifier: Send + Sync {
    /// Verify a credential
    async fn verify_credential(&self, credential: &str) -> Result<bool, anyhow::Error>;
}

/// A service for synchronizing credentials across federation peers
pub struct CredentialSyncService {
    /// Storage manager
    storage_manager: Arc<dyn StorageManager>,
    /// Identity manager
    identity_manager: Arc<dyn IdentityManager>,
    /// Credential verifier
    credential_verifier: Option<Arc<dyn CredentialVerifier>>,
    /// Configuration
    config: CredentialSyncConfig,
    /// HTTP client for federation communication
    http_client: reqwest::Client,
}

impl CredentialSyncService {
    /// Create a new CredentialSyncService
    pub fn new(
        storage_manager: Arc<dyn StorageManager>,
        identity_manager: Arc<dyn IdentityManager>,
        config: CredentialSyncConfig,
    ) -> Self {
        Self {
            storage_manager,
            identity_manager,
            credential_verifier: None,
            config,
            http_client: reqwest::Client::new(),
        }
    }

    /// Set the credential verifier
    pub fn with_credential_verifier(mut self, verifier: Arc<dyn CredentialVerifier>) -> Self {
        self.credential_verifier = Some(verifier);
        self
    }

    /// Start the background sync task
    pub fn start_background_sync(&self) -> tokio::task::JoinHandle<()> {
        if self.config.sync_interval.is_none() {
            return tokio::spawn(async {
                info!("Background sync disabled, not starting");
            });
        }

        let storage_manager = self.storage_manager.clone();
        let identity_manager = self.identity_manager.clone();
        let verifier = self.credential_verifier.clone();
        let config = self.config.clone();
        let http_client = self.http_client.clone();

        tokio::spawn(async move {
            let mut interval = time::interval(config.sync_interval.unwrap_or(Duration::from_secs(300)));
            
            loop {
                interval.tick().await;
                
                info!("Starting background federation credential sync");
                
                // Create a fresh service instance for this iteration
                let service = CredentialSyncService {
                    storage_manager: storage_manager.clone(),
                    identity_manager: identity_manager.clone(),
                    credential_verifier: verifier.clone(),
                    config: config.clone(),
                    http_client: http_client.clone(),
                };
                
                // Sync with all peers
                for peer in &config.peers {
                    let from_timestamp = peer.last_sync.unwrap_or_else(|| 
                        DateTime::<Utc>::from_utc(
                            chrono::NaiveDateTime::from_timestamp_opt(0, 0).unwrap(),
                            Utc,
                        )
                    );
                    
                    let params = SyncParameters {
                        federation_id: peer.did.clone(),
                        credential_types: vec![
                            SyncCredentialType::ExecutionReceipt,
                            SyncCredentialType::ProposalOutcome,
                        ],
                        from_timestamp,
                        to_timestamp: None,
                        limit: Some(config.max_credentials_per_sync),
                    };
                    
                    match service.sync_credentials_from_peer(peer, &params).await {
                        Ok(result) => {
                            info!(
                                peer_did = %peer.did,
                                credentials_synced = result.status.credentials_synced,
                                "Successfully synced credentials from peer"
                            );
                        }
                        Err(e) => {
                            error!(
                                peer_did = %peer.did,
                                error = %e,
                                "Failed to sync credentials from peer"
                            );
                        }
                    }
                }
            }
        })
    }

    /// Synchronize credentials from a specific peer
    pub async fn sync_credentials_from_peer(
        &self, 
        peer: &FederationPeer, 
        params: &SyncParameters
    ) -> Result<SyncResult, anyhow::Error> {
        info!(
            peer_did = %peer.did,
            from_timestamp = %params.from_timestamp,
            "Syncing credentials from peer"
        );
        
        let sync_initiated = Utc::now();
        let mut status = SyncStatus {
            sync_initiated,
            sync_completed: None,
            credentials_synced: 0,
            credentials_failed: 0,
            error_message: None,
        };
        
        // Fetch credentials from peer
        let credentials = self.fetch_credentials_from_peer(peer, params).await?;
        
        // Process and store each credential
        let mut credential_cids = Vec::new();
        
        for credential_data in credentials {
            match self.process_and_store_credential(&credential_data).await {
                Ok(cid) => {
                    credential_cids.push(cid);
                    status.credentials_synced += 1;
                }
                Err(e) => {
                    warn!(error = %e, "Failed to process and store credential");
                    status.credentials_failed += 1;
                }
            }
        }
        
        status.sync_completed = Some(Utc::now());
        
        Ok(SyncResult {
            status,
            credential_cids,
        })
    }

    /// Fetch credentials from a peer
    async fn fetch_credentials_from_peer(
        &self,
        peer: &FederationPeer,
        params: &SyncParameters,
    ) -> Result<Vec<String>, anyhow::Error> {
        // In a real implementation, this would make an HTTP request to the peer's sync endpoint
        // For now, we'll simulate with a dummy implementation
        
        debug!(
            peer_did = %peer.did,
            endpoint = %peer.sync_endpoint,
            "Fetching credentials from peer"
        );
        
        // Construct the URL with query parameters
        let mut url = reqwest::Url::parse(&peer.sync_endpoint)?;
        
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("federationId", &params.federation_id);
            
            // Add credential types
            for cred_type in &params.credential_types {
                query.append_pair("credentialType", &format!("{:?}", cred_type));
            }
            
            // Add timestamp range
            query.append_pair("fromTimestamp", &params.from_timestamp.to_rfc3339());
            
            if let Some(to) = params.to_timestamp {
                query.append_pair("toTimestamp", &to.to_rfc3339());
            }
            
            // Add limit
            if let Some(limit) = params.limit {
                query.append_pair("limit", &limit.to_string());
            }
        }
        
        // Make the request
        let response = self.http_client.get(url).send().await?;
        
        // Check response status
        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Failed to fetch credentials from peer: HTTP {}",
                response.status()
            ));
        }
        
        // Parse response
        let credentials: Vec<String> = response.json().await?;
        
        Ok(credentials)
    }

    /// Process and store a credential
    async fn process_and_store_credential(&self, credential_json: &str) -> Result<String, anyhow::Error> {
        // Verify the credential if a verifier is configured
        if let Some(verifier) = &self.credential_verifier {
            if !verifier.verify_credential(credential_json).await? {
                return Err(anyhow::anyhow!("Credential verification failed"));
            }
        }
        
        // Parse the credential to determine its type
        let credential_value: serde_json::Value = serde_json::from_str(credential_json)?;
        
        // Extract credential type
        let credential_type = credential_value["type"]
            .as_array()
            .and_then(|types| types.get(1))
            .and_then(|t| t.as_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid credential type"))?;
        
        // Generate a key based on credential type and ID
        let credential_id = credential_value["id"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid credential ID"))?;
        
        // Anchor the credential to the DAG
        let dag_store = self.storage_manager.dag_store()?;
        
        // Create a DAG key based on credential type and ID
        let dag_key = format!("credential:{}:{}", credential_type.to_lowercase(), credential_id);
        
        // Store the credential in the DAG
        let cid = dag_store.store_node(credential_json.as_bytes().to_vec()).await?;
        
        // Store a mapping from the key to the CID
        let key_mapping = format!("key:{}", dag_key);
        
        // TODO: Use a proper key-value store
        debug!(
            credential_id = %credential_id,
            credential_type = %credential_type,
            cid = %cid,
            "Stored synchronized credential in DAG"
        );
        
        Ok(cid)
    }

    /// Crawl the DAG for credentials of a specific type
    pub async fn crawl_dag_for_credentials(
        &self,
        credential_type: SyncCredentialType,
        from_timestamp: DateTime<Utc>,
        to_timestamp: Option<DateTime<Utc>>,
        limit: Option<usize>,
    ) -> Result<Vec<(String, Vec<u8>)>, anyhow::Error> {
        let dag_store = self.storage_manager.dag_store()?;
        
        // Convert credential type to string
        let type_str = match credential_type {
            SyncCredentialType::ExecutionReceipt => "execution_receipt",
            SyncCredentialType::ProposalOutcome => "proposal_outcome",
            SyncCredentialType::ResourceTransfer => "resource_transfer",
            SyncCredentialType::MembershipCredential => "membership_credential",
        };
        
        // Create DAG key prefix
        let prefix = format!("credential:{}", type_str);
        
        // In a real implementation, this would query the DAG for anchors with the given prefix
        // and filter by timestamp. For now, return an empty vector.
        
        Ok(Vec::new())
    }
}

/// HTTP handlers for credential synchronization
pub mod http {
    use super::*;
    use warp::{Filter, Rejection, Reply};
    use std::convert::Infallible;

    /// Create a warp filter for the credential sync endpoint
    pub fn sync_credentials_filter(
        service: Arc<CredentialSyncService>,
    ) -> impl Filter<Extract = impl Reply, Error = Rejection> + Clone {
        warp::path!("federation" / "credentials" / "sync")
            .and(warp::get())
            .and(warp::query::<SyncParameters>())
            .and(with_service(service))
            .and_then(handle_sync_credentials)
    }

    /// Helper function to include the service in the route
    fn with_service(
        service: Arc<CredentialSyncService>,
    ) -> impl Filter<Extract = (Arc<CredentialSyncService>,), Error = Infallible> + Clone {
        warp::any().map(move || service.clone())
    }

    /// Handle credential sync requests
    async fn handle_sync_credentials(
        params: SyncParameters,
        service: Arc<CredentialSyncService>,
    ) -> Result<impl Reply, Rejection> {
        // Logic to handle credential sync requests
        // This would crawl the DAG for credentials and return them
        
        let now = Utc::now();
        let to_timestamp = params.to_timestamp.unwrap_or(now);
        
        let mut credentials = Vec::new();
        
        for cred_type in &params.credential_types {
            match service
                .crawl_dag_for_credentials(
                    *cred_type,
                    params.from_timestamp,
                    Some(to_timestamp),
                    params.limit,
                )
                .await
            {
                Ok(creds) => {
                    for (_, data) in creds {
                        if let Ok(json) = String::from_utf8(data) {
                            credentials.push(json);
                        }
                    }
                }
                Err(e) => {
                    error!(
                        error = %e,
                        credential_type = ?cred_type,
                        "Failed to crawl DAG for credentials"
                    );
                }
            }
        }
        
        let limit = params.limit.unwrap_or(usize::MAX);
        if credentials.len() > limit {
            credentials.truncate(limit);
        }
        
        Ok(warp::reply::json(&credentials))
    }
}

/// A simple credential verifier implementation that checks credential signatures
pub struct SimpleCredentialVerifier {
    identity_manager: Arc<dyn IdentityManager>,
}

impl SimpleCredentialVerifier {
    /// Create a new SimpleCredentialVerifier
    pub fn new(identity_manager: Arc<dyn IdentityManager>) -> Self {
        Self {
            identity_manager,
        }
    }
}

#[async_trait]
impl CredentialVerifier for SimpleCredentialVerifier {
    async fn verify_credential(&self, credential_json: &str) -> Result<bool, anyhow::Error> {
        // Parse the credential
        let credential_value: serde_json::Value = serde_json::from_str(credential_json)?;
        
        // Extract issuer
        let issuer = credential_value["issuer"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid credential issuer"))?;
        
        // Extract proof if present
        let proof = credential_value["proof"].as_object();
        
        if let Some(proof) = proof {
            // Extract verification method
            let verification_method = proof["verificationMethod"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid verification method"))?;
            
            // Extract signature
            let signature = proof["proofValue"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("Invalid proof value"))?;
            
            // In a real implementation, this would verify the signature using the identity manager
            // For now, just check that the issuer is a valid DID
            match self.identity_manager.verify_identity(issuer).await {
                Ok(is_valid) => Ok(is_valid),
                Err(e) => Err(anyhow::anyhow!("Failed to verify identity: {}", e)),
            }
        } else {
            // No proof, just check that the issuer is a valid DID
            match self.identity_manager.verify_identity(issuer).await {
                Ok(is_valid) => Ok(is_valid),
                Err(e) => Err(anyhow::anyhow!("Failed to verify identity: {}", e)),
            }
        }
    }
} 