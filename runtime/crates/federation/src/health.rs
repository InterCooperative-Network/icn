/*!
 * Federation Health and Diagnostics
 * 
 * This module implements health checks and diagnostic endpoints for federation nodes,
 * enabling monitoring of federation status, quorum, and replication.
 */

use crate::{
    FederationResult, 
    FederationError,
    debug_api::{FederationStatusResponse, DagNodeResponse, ProposalStatusResponse},
    sync,
};

use std::sync::Arc;
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use tokio::sync::Mutex;
use tracing::{debug, info, error, warn};
use serde::{Serialize, Deserialize};

use libp2p::{PeerId, Swarm};
use cid::Cid;
use icn_identity::TrustBundle;
use icn_storage::StorageBackend;

/// Federation health metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationHealth {
    /// Overall status (ok, degraded, error)
    pub status: String,
    
    /// Current epoch 
    pub epoch: u64,
    
    /// Connected peers count
    pub connected_peers: usize,
    
    /// Blob replication status
    pub replication_status: ReplicationStatus,
    
    /// Time since last sync (seconds)
    pub time_since_sync: u64,
    
    /// Federation quorum health
    pub quorum_health: QuorumHealth,
    
    /// Last error message (if any)
    pub last_error: Option<String>,
}

/// Blob replication status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplicationStatus {
    /// Total blobs tracked
    pub total_blobs: usize,
    
    /// Fully replicated blobs count
    pub fully_replicated: usize,
    
    /// Blobs with replication in progress
    pub in_progress: usize,
    
    /// Blobs with failed replication
    pub failed: usize,
}

/// Quorum health metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuorumHealth {
    /// Enough validators for quorum? 
    pub has_validator_quorum: bool,
    
    /// Enough guardians for quorum?
    pub has_guardian_quorum: bool,
    
    /// Validator node count
    pub validator_count: usize,
    
    /// Guardian node count
    pub guardian_count: usize,
    
    /// Observer node count
    pub observer_count: usize,
    
    /// Required quorum size
    pub required_quorum: usize,
}

/// Federation diagnostic report with detailed status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationDiagnostic {
    /// Base health metrics
    pub health: FederationHealth,
    
    /// Peer list with connection details
    pub peers: Vec<PeerDiagnostic>,
    
    /// Epoch details including DAG roots
    pub epoch_details: Option<EpochDiagnostic>,
    
    /// Detected inconsistencies
    pub inconsistencies: Vec<String>,
}

/// Peer diagnostic information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerDiagnostic {
    /// Peer ID
    pub peer_id: String,
    
    /// Connection status
    pub connected: bool,
    
    /// Peer role
    pub role: String,
    
    /// Peer addresses
    pub addresses: Vec<String>,
    
    /// Ping latency in ms
    pub latency_ms: Option<u64>,
    
    /// Protocol versions supported
    pub protocols: Vec<String>,
}

/// Epoch diagnostic details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EpochDiagnostic {
    /// Epoch number
    pub epoch: u64,
    
    /// Creation timestamp
    pub created_at: SystemTime,
    
    /// DAG root CIDs
    pub dag_roots: Vec<String>,
    
    /// Guardians who signed
    pub signers: Vec<String>,
    
    /// Credential attestations
    pub attestation_count: usize,
}

/// Get current federation health
pub async fn get_federation_health(
    storage: &Arc<Mutex<dyn StorageBackend + Send + Sync>>,
    swarm: &Swarm<crate::network::IcnFederationBehaviour>,
    last_sync_time: Option<SystemTime>,
) -> FederationResult<FederationHealth> {
    // Get current epoch
    let epoch = sync::get_latest_known_epoch(storage).await?;
    
    // Get connected peers
    let connected_peers = swarm.connected_peers().count();
    
    // Get trust bundle to calculate quorum health
    let trust_bundle = match sync::get_trust_bundle(epoch, storage).await? {
        Some(bundle) => bundle,
        None => {
            return Ok(FederationHealth {
                status: "degraded".to_string(),
                epoch,
                connected_peers,
                replication_status: ReplicationStatus {
                    total_blobs: 0,
                    fully_replicated: 0,
                    in_progress: 0,
                    failed: 0,
                },
                time_since_sync: 0,
                quorum_health: QuorumHealth {
                    has_validator_quorum: false,
                    has_guardian_quorum: false,
                    validator_count: 0,
                    guardian_count: 0,
                    observer_count: 0,
                    required_quorum: 0,
                },
                last_error: Some("No trust bundle available".to_string()),
            });
        }
    };
    
    // Calculate roles from trust bundle
    // In a real implementation, this would analyze the trust bundle for roles
    // For now, we'll use placeholder logic
    let validator_count = 3; // Placeholder
    let guardian_count = 2;  // Placeholder
    let observer_count = 2;  // Placeholder
    let required_quorum = (validator_count * 2) / 3 + 1;
    
    // Calculate time since last sync
    let time_since_sync = match last_sync_time {
        Some(last_sync) => {
            SystemTime::now()
                .duration_since(last_sync)
                .unwrap_or_else(|_| Duration::from_secs(0))
                .as_secs()
        },
        None => 0,
    };
    
    // Create health response
    let health = FederationHealth {
        status: if validator_count >= required_quorum { "ok" } else { "degraded" }.to_string(),
        epoch,
        connected_peers,
        replication_status: ReplicationStatus {
            total_blobs: 100, // Placeholder
            fully_replicated: 90, // Placeholder
            in_progress: 8,   // Placeholder
            failed: 2,        // Placeholder
        },
        time_since_sync,
        quorum_health: QuorumHealth {
            has_validator_quorum: validator_count >= required_quorum,
            has_guardian_quorum: guardian_count >= 2, // Assuming 2 is the required guardian quorum
            validator_count,
            guardian_count,
            observer_count,
            required_quorum,
        },
        last_error: None,
    };
    
    Ok(health)
}

/// Get detailed federation diagnostic information
pub async fn get_federation_diagnostic(
    storage: &Arc<Mutex<dyn StorageBackend + Send + Sync>>,
    swarm: &Swarm<crate::network::IcnFederationBehaviour>,
    last_sync_time: Option<SystemTime>,
) -> FederationResult<FederationDiagnostic> {
    // Get base health metrics
    let health = get_federation_health(storage, swarm, last_sync_time).await?;
    
    // Get peer information
    let mut peers = Vec::new();
    for peer_id in swarm.connected_peers() {
        let peer_info = PeerDiagnostic {
            peer_id: peer_id.to_string(),
            connected: true,
            role: "unknown".to_string(), // Would be populated from trust bundle in real implementation
            addresses: Vec::new(), // Would be populated from swarm in real implementation
            latency_ms: None,      // Would be measured in real implementation
            protocols: Vec::new(), // Would be populated from swarm in real implementation
        };
        peers.push(peer_info);
    }
    
    // Get epoch details if available
    let epoch_details = if let Some(bundle) = sync::get_trust_bundle(health.epoch, storage).await? {
        Some(EpochDiagnostic {
            epoch: bundle.epoch_id,
            created_at: SystemTime::now(), // This would come from the bundle in real implementation
            dag_roots: bundle.dag_roots.iter().map(|cid| cid.to_string()).collect(),
            signers: match &bundle.proof {
                Some(proof) => proof.signers.clone(),
                None => Vec::new(),
            },
            attestation_count: bundle.attestations.len(),
        })
    } else {
        None
    };
    
    // Check for inconsistencies
    let mut inconsistencies = Vec::new();
    
    // Example inconsistency checks:
    if health.connected_peers < health.quorum_health.required_quorum {
        inconsistencies.push("Connected peers below required quorum".to_string());
    }
    
    if health.replication_status.failed > 0 {
        inconsistencies.push(format!(
            "{} blobs failed to replicate", 
            health.replication_status.failed
        ));
    }
    
    Ok(FederationDiagnostic {
        health,
        peers,
        epoch_details,
        inconsistencies,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use icn_storage::AsyncInMemoryStorage;
    
    #[tokio::test]
    async fn test_health_response_structure() {
        let health = FederationHealth {
            status: "ok".to_string(),
            epoch: 42,
            connected_peers: 5,
            replication_status: ReplicationStatus {
                total_blobs: 100,
                fully_replicated: 90,
                in_progress: 8,
                failed: 2,
            },
            time_since_sync: 60,
            quorum_health: QuorumHealth {
                has_validator_quorum: true,
                has_guardian_quorum: true,
                validator_count: 3,
                guardian_count: 2,
                observer_count: 2,
                required_quorum: 2,
            },
            last_error: None,
        };
        
        // Serialize to JSON to verify structure
        let json = serde_json::to_string_pretty(&health).unwrap();
        println!("{}", json);
        
        // Deserialize back
        let health2: FederationHealth = serde_json::from_str(&json).unwrap();
        
        // Verify fields
        assert_eq!(health.status, health2.status);
        assert_eq!(health.epoch, health2.epoch);
        assert_eq!(health.connected_peers, health2.connected_peers);
    }
} 