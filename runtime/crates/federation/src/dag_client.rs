use serde::{Deserialize, Serialize};
use chrono::{DateTime, Utc};
use crate::error::{FederationError, FederationResult};
use crate::recovery::{RecoveryEvent, RecoveryEventType, FederationKeyRotationEvent, GuardianSuccessionEvent, MetadataUpdateEvent, DisasterRecoveryAnchor};
use crate::dag_anchor::GenesisAnchor;
use crate::genesis::FederationMetadata;
use crate::guardian::{GuardianQuorumConfig, QuorumType};
use async_trait::async_trait;
use std::collections::HashMap;
use icn_identity::Signature;
use cid;

/// Represents an event that can be anchored in the DAG
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum FederationDagEvent {
    /// Federation genesis event
    Genesis(GenesisAnchor),
    /// Key rotation event
    KeyRotation(FederationKeyRotationEvent),
    /// Guardian succession event
    GuardianSuccession(GuardianSuccessionEvent),
    /// Metadata update event
    MetadataUpdate(MetadataUpdateEvent),
    /// Disaster recovery event
    DisasterRecovery(DisasterRecoveryAnchor),
}

impl FederationDagEvent {
    /// Get the federation DID for this event
    pub fn federation_did(&self) -> &str {
        match self {
            FederationDagEvent::Genesis(e) => &e.federation_did,
            FederationDagEvent::KeyRotation(e) => &e.base.federation_did,
            FederationDagEvent::GuardianSuccession(e) => &e.base.federation_did,
            FederationDagEvent::MetadataUpdate(e) => &e.base.federation_did,
            FederationDagEvent::DisasterRecovery(e) => &e.base.federation_did,
        }
    }
    
    /// Get the event timestamp
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            FederationDagEvent::Genesis(e) => e.issued_at,
            FederationDagEvent::KeyRotation(e) => e.base.timestamp,
            FederationDagEvent::GuardianSuccession(e) => e.base.timestamp,
            FederationDagEvent::MetadataUpdate(e) => e.base.timestamp,
            FederationDagEvent::DisasterRecovery(e) => e.base.timestamp,
        }
    }
    
    /// Get the previous event CID, if any
    pub fn previous_cid(&self) -> Option<&String> {
        match self {
            FederationDagEvent::Genesis(_) => None, // Genesis has no previous
            FederationDagEvent::KeyRotation(e) => e.base.previous_event_cid.as_ref(),
            FederationDagEvent::GuardianSuccession(e) => e.base.previous_event_cid.as_ref(),
            FederationDagEvent::MetadataUpdate(e) => e.base.previous_event_cid.as_ref(),
            FederationDagEvent::DisasterRecovery(e) => e.base.previous_event_cid.as_ref(),
        }
    }
    
    /// Get the event sequence number
    pub fn sequence_number(&self) -> u64 {
        match self {
            FederationDagEvent::Genesis(_) => 0, // Genesis is always sequence 0
            FederationDagEvent::KeyRotation(e) => e.base.sequence_number,
            FederationDagEvent::GuardianSuccession(e) => e.base.sequence_number,
            FederationDagEvent::MetadataUpdate(e) => e.base.sequence_number,
            FederationDagEvent::DisasterRecovery(e) => e.base.sequence_number,
        }
    }
    
    /// Get the event type
    pub fn event_type(&self) -> &'static str {
        match self {
            FederationDagEvent::Genesis(_) => "genesis",
            FederationDagEvent::KeyRotation(_) => "key_rotation",
            FederationDagEvent::GuardianSuccession(_) => "guardian_succession",
            FederationDagEvent::MetadataUpdate(_) => "metadata_update",
            FederationDagEvent::DisasterRecovery(_) => "disaster_recovery",
        }
    }
}

/// DAG node for federation events
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FederationDagNode {
    /// Content identifier for this node
    pub cid: String,
    /// The federation event
    pub event: FederationDagEvent,
    /// CID of the previous event in the chain, if any
    pub previous_cid: Option<String>,
    /// Timestamp when this node was created
    pub created_at: DateTime<Utc>,
}

/// Interface for interacting with the DAG
#[async_trait]
pub trait DagClient {
    /// Store a federation event in the DAG
    async fn store_event(&self, event: FederationDagEvent) -> FederationResult<String>;
    
    /// Retrieve a federation event by CID
    async fn get_event(&self, cid: &str) -> FederationResult<FederationDagNode>;
    
    /// List all events for a federation
    async fn list_federation_events(&self, federation_did: &str) -> FederationResult<Vec<String>>;
    
    /// Verify an event chain
    async fn verify_event_chain(&self, cid: &str) -> FederationResult<bool>;
    
    /// Generate a CID for an event
    fn generate_cid(&self, event: &FederationDagEvent) -> FederationResult<String>;
}

/// In-memory DAG client implementation for testing
#[derive(Clone)]
pub struct InMemoryDagClient {
    events: HashMap<String, FederationDagNode>,
    federation_events: HashMap<String, Vec<String>>,
}

impl Default for InMemoryDagClient {
    fn default() -> Self {
        Self {
            events: HashMap::new(),
            federation_events: HashMap::new(),
        }
    }
}

#[async_trait]
impl DagClient for InMemoryDagClient {
    async fn store_event(&self, event: FederationDagEvent) -> FederationResult<String> {
        let mut client = self.clone();
        let federation_did = event.federation_did().to_string();
        let cid = client.generate_cid(&event)?;
        
        let node = FederationDagNode {
            cid: cid.clone(),
            previous_cid: event.previous_cid().cloned(),
            event,
            created_at: Utc::now(),
        };
        
        client.events.insert(cid.clone(), node);
        
        let events = client.federation_events.entry(federation_did).or_insert_with(Vec::new);
        events.push(cid.clone());
        
        Ok(cid)
    }
    
    async fn get_event(&self, cid: &str) -> FederationResult<FederationDagNode> {
        self.events.get(cid)
            .cloned()
            .ok_or_else(|| FederationError::NotFound(format!("DAG node not found: {}", cid)))
    }
    
    async fn list_federation_events(&self, federation_did: &str) -> FederationResult<Vec<String>> {
        Ok(self.federation_events.get(federation_did)
            .cloned()
            .unwrap_or_default())
    }
    
    async fn verify_event_chain(&self, cid: &str) -> FederationResult<bool> {
        let node = self.get_event(cid).await?;
        
        // Check if this is a genesis event (which has no previous)
        if let FederationDagEvent::Genesis(_) = node.event {
            return Ok(true);
        }
        
        // For non-genesis events, verify there's a previous event
        if let Some(prev_cid) = &node.previous_cid {
            let prev_node = self.get_event(prev_cid).await?;
            
            // Verify sequence ordering
            if prev_node.event.sequence_number() + 1 != node.event.sequence_number() {
                return Err(FederationError::ValidationError(
                    format!("Sequence number mismatch: previous={}, current={}", 
                            prev_node.event.sequence_number(),
                            node.event.sequence_number())
                ));
            }
            
            // Verify federation DID consistency (except for disaster recovery which may change it)
            if !matches!(node.event, FederationDagEvent::DisasterRecovery(_)) 
                && prev_node.event.federation_did() != node.event.federation_did() {
                return Err(FederationError::ValidationError(
                    format!("Federation DID mismatch: previous={}, current={}", 
                            prev_node.event.federation_did(),
                            node.event.federation_did())
                ));
            }
            
            // Recursively verify the previous node's chain
            return self.verify_event_chain(prev_cid).await;
        }
        
        // Non-genesis events must have a previous event
        Err(FederationError::ValidationError(
            "Non-genesis event missing previous event link".to_string()
        ))
    }
    
    fn generate_cid(&self, event: &FederationDagEvent) -> FederationResult<String> {
        // In a real implementation, we would use a proper content addressing system
        // For this example, we'll just create a mock CID
        
        let prefix = match event {
            FederationDagEvent::Genesis(_) => "bafy_genesis",
            FederationDagEvent::KeyRotation(_) => "bafy_key_rotation",
            FederationDagEvent::GuardianSuccession(_) => "bafy_guardian",
            FederationDagEvent::MetadataUpdate(_) => "bafy_metadata",
            FederationDagEvent::DisasterRecovery(_) => "bafy_recovery",
        };
        
        let federation = event.federation_did().split(':').last().unwrap_or("unknown");
        let timestamp = event.timestamp().timestamp();
        let sequence = event.sequence_number();
        
        Ok(format!("{}_{}_{}_{}", prefix, federation, timestamp, sequence))
    }
}

/// Simple event replay engine
pub struct FederationReplayEngine<'a, T: DagClient> {
    dag_client: &'a T,
}

impl<'a, T: DagClient> FederationReplayEngine<'a, T> {
    /// Create a new replay engine with the given DAG client
    pub fn new(dag_client: &'a T) -> Self {
        Self { dag_client }
    }
    
    /// Replay all events for a federation
    pub async fn replay_federation(&self, federation_did: &str) -> FederationResult<Vec<FederationDagEvent>> {
        let cids = self.dag_client.list_federation_events(federation_did).await?;
        
        // Sort events by sequence number
        let mut events = Vec::new();
        for cid in cids {
            let node = self.dag_client.get_event(&cid).await?;
            events.push(node);
        }
        
        events.sort_by_key(|node| node.event.sequence_number());
        
        // Extract just the events
        let result = events.into_iter().map(|node| node.event).collect();
        Ok(result)
    }
    
    /// Replay events from a specific CID
    pub async fn replay_from(&self, cid: &str) -> FederationResult<Vec<FederationDagEvent>> {
        let mut result = Vec::new();
        let mut current_cid = cid.to_string();
        
        // Verify the event chain first
        self.dag_client.verify_event_chain(&current_cid).await?;
        
        // Collect all events in the chain
        loop {
            let node = self.dag_client.get_event(&current_cid).await?;
            result.push(node.event.clone());
            
            if let Some(prev_cid) = node.previous_cid {
                current_cid = prev_cid;
            } else {
                break;
            }
        }
        
        // Reverse to get chronological order
        result.reverse();
        Ok(result)
    }
}

/// Validation functions for federation DAG events
pub mod validation {
    use super::*;
    
    /// Validate a federation event chain
    pub async fn validate_event_chain<T: DagClient>(
        dag_client: &T,
        cid: &str,
    ) -> FederationResult<bool> {
        dag_client.verify_event_chain(cid).await
    }
    
    /// Validate a specific federation event
    pub async fn validate_event<T: DagClient>(
        dag_client: &T,
        event: &FederationDagEvent,
    ) -> FederationResult<bool> {
        // For now, just a placeholder validation
        // In a real implementation, we would validate signatures, quorum, etc.
        Ok(true)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::recovery::RecoveryEvent;
    use icn_identity::{KeyPair, Signature};
    
    #[tokio::test]
    async fn test_dag_client_store_and_retrieve() {
        // Set up an in-memory DAG client
        let client = InMemoryDagClient::default();
        
        // Create a mock genesis event
        let genesis = FederationDagEvent::Genesis(GenesisAnchor {
            federation_did: "did:key:z6MkTestFederation".to_string(),
            trust_bundle_cid: "bafy_bundle_123".to_string(),
            dag_root_cid: "bafy_dag_root_123".to_string(),
            issued_at: Utc::now(),
            anchor_signature: Signature(vec![1, 2, 3, 4]),
        });
        
        // Store the event
        let genesis_cid = client.store_event(genesis.clone()).await.unwrap();
        
        // Create a key rotation event
        let key_rotation = FederationDagEvent::KeyRotation(FederationKeyRotationEvent {
            base: RecoveryEvent {
                event_type: RecoveryEventType::FederationKeyRotation,
                federation_did: "did:key:z6MkTestFederation".to_string(),
                sequence_number: 1,
                previous_event_cid: Some(genesis_cid.clone()),
                timestamp: Utc::now(),
                signatures: vec![Signature(vec![5, 6, 7, 8])],
            },
            new_federation_did: "did:key:z6MkNewTestFederation".to_string(),
            key_proof: Signature(vec![9, 10, 11, 12]),
        });
        
        // Store the key rotation event
        let rotation_cid = client.store_event(key_rotation.clone()).await.unwrap();
        
        // Retrieve the events
        let genesis_node = client.get_event(&genesis_cid).await.unwrap();
        let rotation_node = client.get_event(&rotation_cid).await.unwrap();
        
        // Verify the events were stored correctly
        assert_eq!(genesis_node.event.federation_did(), "did:key:z6MkTestFederation");
        assert_eq!(rotation_node.event.federation_did(), "did:key:z6MkTestFederation");
        assert_eq!(rotation_node.previous_cid, Some(genesis_cid));
        
        // Verify the event chain
        let valid = client.verify_event_chain(&rotation_cid).await.unwrap();
        assert!(valid, "Event chain validation should succeed");
        
        // List all events for the federation
        let events = client.list_federation_events("did:key:z6MkTestFederation").await.unwrap();
        assert_eq!(events.len(), 2, "Should have 2 events for the federation");
    }
    
    #[tokio::test]
    async fn test_replay_engine() {
        // Set up an in-memory DAG client
        let client = InMemoryDagClient::default();
        
        // Create a federation event chain
        let federation_did = "did:key:z6MkTestFederation".to_string();
        
        // 1. Create a genesis event
        let genesis = FederationDagEvent::Genesis(GenesisAnchor {
            federation_did: federation_did.clone(),
            trust_bundle_cid: "bafy_bundle_123".to_string(),
            dag_root_cid: "bafy_dag_root_123".to_string(),
            issued_at: Utc::now(),
            anchor_signature: Signature(vec![1, 2, 3, 4]),
        });
        
        let genesis_cid = client.store_event(genesis.clone()).await.unwrap();
        
        // 2. Create a key rotation event
        let key_rotation = FederationDagEvent::KeyRotation(FederationKeyRotationEvent {
            base: RecoveryEvent {
                event_type: RecoveryEventType::FederationKeyRotation,
                federation_did: federation_did.clone(),
                sequence_number: 1,
                previous_event_cid: Some(genesis_cid.clone()),
                timestamp: Utc::now(),
                signatures: vec![Signature(vec![5, 6, 7, 8])],
            },
            new_federation_did: "did:key:z6MkNewTestFederation".to_string(),
            key_proof: Signature(vec![9, 10, 11, 12]),
        });
        
        let rotation_cid = client.store_event(key_rotation.clone()).await.unwrap();
        
        // 3. Create a metadata update event
        let metadata_update = FederationDagEvent::MetadataUpdate(MetadataUpdateEvent {
            base: RecoveryEvent {
                event_type: RecoveryEventType::MetadataUpdate,
                federation_did: federation_did.clone(),
                sequence_number: 2,
                previous_event_cid: Some(rotation_cid.clone()),
                timestamp: Utc::now(),
                signatures: vec![Signature(vec![13, 14, 15, 16])],
            },
            updated_metadata: FederationMetadata {
                federation_did: federation_did.clone(),
                name: "Updated Federation".to_string(),
                description: Some("Federation with updated metadata".to_string()),
                created_at: Utc::now(),
                initial_policies: vec![],
                initial_members: vec![],
                guardian_quorum: GuardianQuorumConfig {
                    quorum_type: QuorumType::Majority,
                    guardians: vec!["did:key:guardian1".to_string(), "did:key:guardian2".to_string()],
                    threshold: 1,
                },
                genesis_cid: cid::Cid::default(),
                additional_metadata: Some(serde_json::json!({"test": "metadata"})),
            },
        });
        
        let metadata_cid = client.store_event(metadata_update.clone()).await.unwrap();
        
        // Create a replay engine
        let replay_engine = FederationReplayEngine::new(&client);
        
        // Replay all events for the federation
        let events = replay_engine.replay_federation(&federation_did).await.unwrap();
        assert_eq!(events.len(), 3, "Should replay 3 events");
        
        // Verify event order
        assert_eq!(events[0].sequence_number(), 0);
        assert_eq!(events[1].sequence_number(), 1);
        assert_eq!(events[2].sequence_number(), 2);
        
        // Replay events from a specific point
        let partial_events = replay_engine.replay_from(&rotation_cid).await.unwrap();
        assert_eq!(partial_events.len(), 2, "Should replay 2 events when starting from rotation event");
        assert_eq!(partial_events[0].sequence_number(), 0);
        assert_eq!(partial_events[1].sequence_number(), 1);
    }
} 