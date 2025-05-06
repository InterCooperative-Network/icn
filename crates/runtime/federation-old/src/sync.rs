/*!
 * TrustBundle Synchronization
 * 
 * This module implements the federation protocol for TrustBundle synchronization
 * between peers, ensuring that all nodes maintain a consistent view of the federation.
 */

use crate::{
    network::{self, TRUST_BUNDLE_PROTOCOL_ID, TRUST_BUNDLE_TIMEOUT},
    FederationError,
    FederationResult,
    FederationResultExt,
};

use std::collections::HashSet;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, error, warn};

use libp2p::{
    request_response::{self, OutboundRequestId, ResponseChannel, RequestId},
    PeerId,
    swarm::Swarm,
};

use icn_identity::TrustBundle;
use icn_storage::{StorageBackend, StorageError};
use cid::Cid;

/// Creates a trust bundle storage key for the specified epoch
pub fn create_trust_bundle_key(epoch: u64) -> Cid {
    let key_str = format!("trustbundle::{}", epoch);
    let key_hash = crate::create_sha256_multihash(key_str.as_bytes());
    cid::Cid::new_v1(0x71, key_hash)
}

/// Creates the latest epoch metadata key
pub fn create_latest_epoch_key() -> Cid {
    let meta_key = "federation::latest_epoch";
    let meta_hash = crate::create_sha256_multihash(meta_key.as_bytes());
    cid::Cid::new_v1(0x71, meta_hash)
}

/// Handle TrustBundle requests from peers
pub async fn handle_trust_bundle_request(
    request: network::TrustBundleRequest,
    channel: request_response::ResponseChannel<network::TrustBundleResponse>,
    storage: &Arc<Mutex<dyn StorageBackend + Send + Sync>>,
    swarm: &mut Swarm<network::IcnFederationBehaviour>,
) -> FederationResult<()> {
    let epoch = request.epoch;
    debug!(epoch, "Received TrustBundleRequest");
    
    // Create key for trust bundle storage
    let key = create_trust_bundle_key(epoch);
    
    // Get the trust bundle from storage
    let mut storage_guard = storage.lock().await;
    let result = storage_guard.get_kv(&key).await;
    drop(storage_guard);
    
    match result {
        Ok(Some(bytes)) => {
            // Try to deserialize
            match serde_json::from_slice::<TrustBundle>(&bytes) {
                Ok(bundle) => {
                    debug!(epoch, "Found trust bundle, sending response");
                    // Send the trust bundle
                    let response = network::TrustBundleResponse { 
                        bundle: Some(bundle) 
                    };
                    swarm.behaviour_mut().trust_bundle.send_response(channel, response);
                    Ok(())
                },
                Err(e) => {
                    error!(epoch, "Failed to deserialize trust bundle: {}", e);
                    // Send error response
                    let response = network::TrustBundleResponse { bundle: None };
                    swarm.behaviour_mut().trust_bundle.send_response(channel, response);
                    Err(FederationError::StorageError(format!("Deserialization error: {}", e)))
                }
            }
        },
        Ok(None) => {
            debug!(epoch, "Trust bundle not found");
            // Trust bundle not found
            let response = network::TrustBundleResponse { bundle: None };
            swarm.behaviour_mut().trust_bundle.send_response(channel, response);
            Ok(())
        },
        Err(e) => {
            error!(epoch, "Failed to retrieve trust bundle: {}", e);
            // Storage error
            let response = network::TrustBundleResponse { bundle: None };
            swarm.behaviour_mut().trust_bundle.send_response(channel, response);
            Err(FederationError::StorageError(format!("Storage error: {}", e)))
        }
    }
}

/// Handle TrustBundle responses from peers
pub async fn handle_trust_bundle_response(
    request_id: OutboundRequestId,
    response: network::TrustBundleResponse,
    storage: &Arc<Mutex<dyn StorageBackend + Send + Sync>>,
    pending_trust_bundle_requests: &mut std::collections::HashMap<OutboundRequestId, (u64, PeerId)>,
) -> FederationResult<Option<TrustBundle>> {
    // Check if this is a response to a pending request
    if let Some((epoch, peer_id)) = pending_trust_bundle_requests.remove(&request_id) {
        match response.bundle {
            Some(bundle) => {
                info!(
                    epoch, 
                    peer=%peer_id,
                    "Received trust bundle from peer"
                );
                
                // Validate the trust bundle
                // TODO: Implement proper verification including:
                // - Quorum signature verification
                // - Expiry check
                // - Signer authorization check
                if bundle.epoch_id != epoch {
                    warn!(
                        received_epoch=bundle.epoch_id,
                        expected_epoch=epoch,
                        peer=%peer_id,
                        "Received trust bundle with unexpected epoch"
                    );
                    return Err(FederationError::TrustBundleError { 
                        kind: crate::TrustBundleErrorKind::ValidationFailed,
                        message: format!("Epoch mismatch: expected {}, got {}", epoch, bundle.epoch_id)
                    });
                }
                
                // Store the bundle
                let key = create_trust_bundle_key(epoch);
                let bundle_bytes = serde_json::to_vec(&bundle)
                    .map_err(|e| FederationError::StorageError(format!("Failed to serialize trust bundle: {}", e)))?;
                
                let mut storage_guard = storage.lock().await;
                
                // Store trust bundle
                storage_guard.put_kv(key, bundle_bytes).await
                    .map_err(|e| FederationError::StorageError(format!("Failed to store trust bundle: {}", e)))?;
                
                // Update latest epoch if this is newer
                let latest_epoch_key = create_latest_epoch_key();
                let current_epoch_opt = storage_guard.get_kv(&latest_epoch_key).await
                    .map_err(|e| FederationError::StorageError(format!("Failed to get latest epoch: {}", e)))?;
                
                let update_needed = match current_epoch_opt {
                    Some(current_bytes) => {
                        let current_str = String::from_utf8_lossy(&current_bytes);
                        if let Ok(cur_val) = current_str.parse::<u64>() {
                            epoch > cur_val
                        } else { true }
                    },
                    None => true,
                };
                
                if update_needed {
                    storage_guard.put_kv(latest_epoch_key, epoch.to_string().into_bytes()).await
                        .map_err(|e| FederationError::StorageError(format!("Failed to update latest epoch: {}", e)))?;
                    info!(epoch, "Updated latest known epoch");
                }
                
                drop(storage_guard);
                
                Ok(Some(bundle))
            },
            None => {
                warn!(epoch, peer=%peer_id, "Peer does not have requested trust bundle");
                Ok(None)
            }
        }
    } else {
        warn!("Received trust bundle response for unknown request");
        Ok(None)
    }
}

/// Request a TrustBundle from a peer
pub async fn request_trust_bundle_from_peer(
    peer_id: &PeerId,
    epoch: u64,
    swarm: &mut Swarm<network::IcnFederationBehaviour>,
    pending_trust_bundle_requests: &mut std::collections::HashMap<OutboundRequestId, (u64, PeerId)>,
) -> FederationResult<OutboundRequestId> {
    debug!(epoch, peer=%peer_id, "Requesting trust bundle from peer");
    
    // Create request
    let request = network::TrustBundleRequest { epoch };
    
    // Send request
    let request_id = swarm.behaviour_mut().trust_bundle.send_request(peer_id, request);
    
    // Track the request
    pending_trust_bundle_requests.insert(request_id, (epoch, *peer_id));
    
    Ok(request_id)
}

/// Get the latest known epoch from storage
pub async fn get_latest_known_epoch(
    storage: &Arc<Mutex<dyn StorageBackend + Send + Sync>>,
) -> FederationResult<u64> {
    let key = create_latest_epoch_key();
    let storage_guard = storage.lock().await;
    
    match storage_guard.get_kv(&key).await {
        Ok(Some(bytes)) => {
            let epoch_str = String::from_utf8_lossy(&bytes);
            match epoch_str.parse::<u64>() {
                Ok(epoch) => Ok(epoch),
                Err(_) => Ok(0), // If we can't parse, assume 0
            }
        },
        Ok(None) => Ok(0), // No epoch stored, assume 0
        Err(e) => Err(FederationError::StorageError(format!("Failed to get latest epoch: {}", e)))
    }
}

/// Get a TrustBundle by epoch from storage
pub async fn get_trust_bundle(
    epoch: u64,
    storage: &Arc<Mutex<dyn StorageBackend + Send + Sync>>,
) -> FederationResult<Option<TrustBundle>> {
    let key = create_trust_bundle_key(epoch);
    let storage_guard = storage.lock().await;
    
    match storage_guard.get_kv(&key).await {
        Ok(Some(bytes)) => {
            match serde_json::from_slice::<TrustBundle>(&bytes) {
                Ok(bundle) => Ok(Some(bundle)),
                Err(e) => Err(FederationError::StorageError(format!("Failed to deserialize trust bundle: {}", e)))
            }
        },
        Ok(None) => Ok(None),
        Err(e) => Err(FederationError::StorageError(format!("Failed to get trust bundle: {}", e)))
    }
}

/// Store a TrustBundle
pub async fn store_trust_bundle(
    bundle: &TrustBundle,
    storage: &Arc<Mutex<dyn StorageBackend + Send + Sync>>,
) -> FederationResult<()> {
    let epoch = bundle.epoch_id;
    let key = create_trust_bundle_key(epoch);
    
    // Serialize the bundle
    let bundle_bytes = serde_json::to_vec(bundle)
        .map_err(|e| FederationError::StorageError(format!("Failed to serialize trust bundle: {}", e)))?;
    
    let mut storage_guard = storage.lock().await;
    
    // Store trust bundle
    storage_guard.put_kv(key, bundle_bytes).await
        .map_err(|e| FederationError::StorageError(format!("Failed to store trust bundle: {}", e)))?;
    
    // Update latest epoch if this is newer
    let latest_epoch_key = create_latest_epoch_key();
    let current_epoch_opt = storage_guard.get_kv(&latest_epoch_key).await
        .map_err(|e| FederationError::StorageError(format!("Failed to get latest epoch: {}", e)))?;
    
    let update_needed = match current_epoch_opt {
        Some(current_bytes) => {
            let current_str = String::from_utf8_lossy(&current_bytes);
            if let Ok(cur_val) = current_str.parse::<u64>() {
                epoch > cur_val
            } else { true }
        },
        None => true,
    };
    
    if update_needed {
        storage_guard.put_kv(latest_epoch_key, epoch.to_string().into_bytes()).await
            .map_err(|e| FederationError::StorageError(format!("Failed to update latest epoch: {}", e)))?;
        info!(epoch, "Updated latest known epoch");
    }
    
    Ok(())
}

/// Initiate epoch discovery
pub async fn discover_latest_epoch(
    known_peers: &[PeerId],
    swarm: &mut Swarm<network::IcnFederationBehaviour>,
    pending_trust_bundle_requests: &mut std::collections::HashMap<OutboundRequestId, (u64, PeerId)>,
) -> FederationResult<()> {
    if known_peers.is_empty() {
        debug!("No known peers for epoch discovery");
        return Ok(());
    }
    
    // Select a random peer for discovery
    use rand::seq::SliceRandom;
    let mut rng = rand::thread_rng();
    
    if let Some(peer) = known_peers.choose(&mut rng) {
        // Request epoch 0 to get the latest available
        request_trust_bundle_from_peer(peer, 0, swarm, pending_trust_bundle_requests).await?;
    }
    
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_storage::AsyncInMemoryStorage;
    
    #[tokio::test]
    async fn test_epoch_storage_and_retrieval() {
        let storage_impl = AsyncInMemoryStorage::new();
        let storage: Arc<Mutex<dyn StorageBackend + Send + Sync>> = Arc::new(Mutex::new(storage_impl));
        
        // Test with no epoch stored
        let epoch = get_latest_known_epoch(&storage).await.unwrap();
        assert_eq!(epoch, 0, "Initial epoch should be 0");
        
        // Create a dummy trust bundle
        let bundle = TrustBundle::new(
            42, // epoch_id
            "test-federation".to_string(), // federation_id
            vec![], // dag_roots
            vec![], // attestations
        );
        
        // Store the bundle
        store_trust_bundle(&bundle, &storage).await.unwrap();
        
        // Check that the latest epoch was updated
        let latest_epoch = get_latest_known_epoch(&storage).await.unwrap();
        assert_eq!(latest_epoch, 42, "Latest epoch should be updated to 42");
        
        // Retrieve the bundle
        let retrieved_bundle = get_trust_bundle(42, &storage).await.unwrap();
        assert!(retrieved_bundle.is_some(), "Bundle should be retrieved");
        
        let retrieved_bundle = retrieved_bundle.unwrap();
        assert_eq!(retrieved_bundle.epoch_id, 42, "Retrieved bundle should have epoch 42");
        assert_eq!(retrieved_bundle.federation_id, "test-federation", "Federation ID should match");
    }
} 