/*!
 * Federation Synchronization Client for Wallet
 * 
 * This module implements the wallet-side client for federation synchronization,
 * including TrustBundle sync and verification.
 */

use crate::{SyncClient, error::SyncError, TrustBundle, DagNode};
use std::sync::Arc;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::{Mutex, broadcast};
use tokio::time::sleep;
use tracing::{debug, info, error, warn};
use reqwest::Url;
use serde::{Serialize, Deserialize};
use serde_json::Value;
use futures::Stream;
use std::pin::Pin;
use std::task::{Context, Poll};
use futures::stream::StreamExt;

/// Federation node address for connection
#[derive(Debug, Clone)]
pub struct FederationNodeAddress {
    /// Base URL for HTTP API calls
    pub http_url: String,
    
    /// libp2p multiaddress for direct P2P connection (optional)
    pub p2p_addr: Option<String>,
    
    /// Node identity (DID)
    pub node_id: Option<String>,
}

/// Federation sync client for wallet integration
pub struct FederationSyncClient {
    /// HTTP client
    client: reqwest::Client,
    
    /// Known federation nodes
    nodes: Vec<FederationNodeAddress>,
    
    /// Current trust bundle
    current_trust_bundle: Arc<Mutex<Option<TrustBundle>>>,
    
    /// Trust bundle update channel
    trust_bundle_tx: broadcast::Sender<TrustBundle>,
    
    /// Wallet identity
    identity: String,
}

/// Trust bundle subscription for receiving updates
pub struct TrustBundleSubscription {
    /// Broadcast receiver
    receiver: broadcast::Receiver<TrustBundle>,
}

impl Stream for TrustBundleSubscription {
    type Item = TrustBundle;
    
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match Pin::new(&mut self.receiver).poll_recv(cx) {
            Poll::Ready(Ok(bundle)) => Poll::Ready(Some(bundle)),
            Poll::Ready(Err(_)) => Poll::Ready(None), // Channel closed
            Poll::Pending => Poll::Pending,
        }
    }
}

/// Request format for trust bundle retrieval
#[derive(Serialize, Deserialize)]
struct TrustBundleRequest {
    epoch: u64,
}

/// Response format for trust bundle retrieval
#[derive(Serialize, Deserialize)]
struct TrustBundleResponse {
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    trust_bundle: Option<TrustBundle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    latest_epoch: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

impl FederationSyncClient {
    /// Create a new federation sync client
    pub fn new(identity: String) -> Self {
        let (tx, _) = broadcast::channel(16);
        
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            nodes: Vec::new(),
            current_trust_bundle: Arc::new(Mutex::new(None)),
            trust_bundle_tx: tx,
            identity,
        }
    }
    
    /// Add a federation node to connect to
    pub fn add_federation_node(&mut self, node: FederationNodeAddress) {
        self.nodes.push(node);
    }
    
    /// Retrieve the latest known TrustBundle
    pub async fn get_latest_trust_bundle(&self) -> Result<TrustBundle, SyncError> {
        // Check if we already have a trust bundle
        {
            let current = self.current_trust_bundle.lock().await;
            if let Some(bundle) = &*current {
                return Ok(bundle.clone());
            }
        }
        
        // Try to get the latest trust bundle from one of our known nodes
        let mut last_error = None;
        
        for node in &self.nodes {
            match self.fetch_trust_bundle_from_node(node, None).await {
                Ok(bundle) => {
                    // Update our current trust bundle
                    let mut current = self.current_trust_bundle.lock().await;
                    *current = Some(bundle.clone());
                    
                    // Notify subscribers
                    let _ = self.trust_bundle_tx.send(bundle.clone());
                    
                    return Ok(bundle);
                },
                Err(e) => {
                    last_error = Some(e);
                    continue;
                }
            }
        }
        
        if let Some(err) = last_error {
            Err(err)
        } else {
            Err(SyncError::Federation("No federation nodes available".to_string()))
        }
    }
    
    /// Retrieve a specific TrustBundle by epoch ID
    pub async fn get_trust_bundle(&self, epoch_id: u64) -> Result<TrustBundle, SyncError> {
        // Check if we already have this trust bundle
        {
            let current = self.current_trust_bundle.lock().await;
            if let Some(bundle) = &*current {
                if bundle.epoch == epoch_id {
                    return Ok(bundle.clone());
                }
            }
        }
        
        // Try to get the specified trust bundle from one of our known nodes
        let mut last_error = None;
        
        for node in &self.nodes {
            match self.fetch_trust_bundle_from_node(node, Some(epoch_id)).await {
                Ok(bundle) => {
                    // Update our current trust bundle if it's newer
                    let mut current = self.current_trust_bundle.lock().await;
                    if let Some(current_bundle) = &*current {
                        if bundle.epoch > current_bundle.epoch {
                            *current = Some(bundle.clone());
                            
                            // Notify subscribers
                            let _ = self.trust_bundle_tx.send(bundle.clone());
                        }
                    } else {
                        *current = Some(bundle.clone());
                        
                        // Notify subscribers
                        let _ = self.trust_bundle_tx.send(bundle.clone());
                    }
                    
                    return Ok(bundle);
                },
                Err(e) => {
                    last_error = Some(e);
                    continue;
                }
            }
        }
        
        if let Some(err) = last_error {
            Err(err)
        } else {
            Err(SyncError::Federation("No federation nodes available".to_string()))
        }
    }
    
    /// Subscribe to new TrustBundle announcements
    pub fn subscribe_to_trust_bundles(&self) -> TrustBundleSubscription {
        TrustBundleSubscription {
            receiver: self.trust_bundle_tx.subscribe(),
        }
    }
    
    /// Start a background task to periodically sync trust bundles
    pub fn start_periodic_sync(
        &self, 
        interval: Duration,
    ) -> tokio::task::JoinHandle<()> {
        let client = self.clone();
        
        tokio::spawn(async move {
            loop {
                // Attempt to sync the latest trust bundle
                match client.get_latest_trust_bundle().await {
                    Ok(bundle) => {
                        debug!(epoch = bundle.epoch, "Synchronized trust bundle");
                    },
                    Err(e) => {
                        warn!("Failed to sync trust bundle: {}", e);
                    }
                }
                
                // Wait for the next sync interval
                sleep(interval).await;
            }
        })
    }
    
    /// Verify a DAG node against the current trust bundle
    pub async fn verify_dag_node(&self, node: &DagNode) -> Result<bool, SyncError> {
        // Get the current trust bundle
        let current = self.current_trust_bundle.lock().await;
        
        // If no trust bundle is available, we can't verify
        let bundle = match &*current {
            Some(bundle) => bundle,
            None => return Err(SyncError::Validation("No trust bundle available for verification".to_string())),
        };
        
        // Check if the issuer is trusted
        if !bundle.trusted_dids.contains(&node.issuer) {
            return Err(SyncError::Validation(format!("Issuer {} is not trusted", node.issuer)));
        }
        
        // In a real implementation, we would verify the signature against the issuer's public key
        // For now, just return true if the issuer is trusted
        Ok(true)
    }
    
    /// Fetch a trust bundle from a node
    async fn fetch_trust_bundle_from_node(
        &self,
        node: &FederationNodeAddress,
        epoch_id: Option<u64>,
    ) -> Result<TrustBundle, SyncError> {
        // Construct URL for the trust bundle endpoint
        let url = format!(
            "{}/api/v1/federation/trust-bundle{}",
            node.http_url,
            epoch_id.map_or("/latest".to_string(), |id| format!("/{}", id))
        );
        
        // Make the request
        let response = self.client.get(&url).send().await?;
        
        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(SyncError::Federation(format!(
                "Failed to get trust bundle: HTTP {}: {}", 
                status, 
                error_text
            )));
        }
        
        // Parse the response
        let bundle_response = response.json::<TrustBundleResponse>().await?;
        
        if bundle_response.status != "success" {
            if let Some(error) = bundle_response.error {
                return Err(SyncError::Federation(format!("Trust bundle error: {}", error)));
            } else {
                return Err(SyncError::Federation("Trust bundle not available".to_string()));
            }
        }
        
        match bundle_response.trust_bundle {
            Some(bundle) => Ok(bundle),
            None => Err(SyncError::Federation("No trust bundle in response".to_string())),
        }
    }
}

impl Clone for FederationSyncClient {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            nodes: self.nodes.clone(),
            current_trust_bundle: self.current_trust_bundle.clone(),
            trust_bundle_tx: self.trust_bundle_tx.clone(),
            identity: self.identity.clone(),
        }
    }
}

/// Extension traits for SyncClient to support federation features
impl SyncClient {
    /// Create a federation sync client
    pub fn federation_client(&self, identity: String) -> FederationSyncClient {
        let mut client = FederationSyncClient::new(identity);
        
        // Add the SyncClient's node as a federation node
        client.add_federation_node(FederationNodeAddress {
            http_url: self.base_url.clone(),
            p2p_addr: None,
            node_id: None,
        });
        
        client
    }
} 