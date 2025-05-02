use cid::Cid;
use serde::{Serialize, Deserialize};
use icn_identity::{IdentityId, IdentityScope, VerifiableCredential};
use uuid::Uuid;
use std::time::{SystemTime, UNIX_EPOCH};

/// Types of governance events that can be emitted
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GovernanceEventType {
    /// A new governance proposal was created
    ProposalCreated,
    /// A vote was cast on a proposal
    VoteCast,
    /// A proposal was finalized
    ProposalFinalized,
    /// A proposal was executed
    ProposalExecuted,
    /// A mandate was issued
    MandateIssued,
    /// A trust bundle was created 
    TrustBundleCreated,
    /// A custom event
    Custom(String),
}

/// A governance event that can be emitted by the kernel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernanceEvent {
    /// Unique ID for this event
    pub id: Uuid,
    
    /// The type of event
    pub event_type: GovernanceEventType,
    
    /// The DID of the entity that generated this event
    pub emitter: IdentityId,
    
    /// Timestamp (Unix timestamp in seconds)
    pub timestamp: u64,
    
    /// Related proposal CID (if relevant)
    pub proposal_cid: Option<Cid>,
    
    /// Related scope (e.g., federation ID or other scope identifier)
    pub scope_id: Option<IdentityId>,
    
    /// Scope type (e.g., Federation, DAO, etc.)
    pub scope_type: Option<IdentityScope>,
    
    /// Additional event-specific data
    pub data: serde_json::Value,
}

impl GovernanceEvent {
    /// Create a new governance event
    pub fn new(
        event_type: GovernanceEventType,
        emitter: IdentityId,
        proposal_cid: Option<Cid>,
        scope_id: Option<IdentityId>,
        scope_type: Option<IdentityScope>,
        data: serde_json::Value,
    ) -> Self {
        // Get current timestamp
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();
        
        Self {
            id: Uuid::new_v4(),
            event_type,
            emitter,
            timestamp,
            proposal_cid,
            scope_id,
            scope_type,
            data,
        }
    }
    
    /// Convert this event to a Verifiable Credential
    pub async fn to_verifiable_credential(&self) -> VerifiableCredential {
        // Create a new VC with appropriate type
        let mut credential_types = vec!["VerifiableCredential".to_string()];
        
        // Add specific type based on event type
        match self.event_type {
            GovernanceEventType::ProposalCreated => {
                credential_types.push("ProposalCreationCredential".to_string());
            },
            GovernanceEventType::VoteCast => {
                credential_types.push("VoteCastCredential".to_string());
            },
            GovernanceEventType::ProposalFinalized => {
                credential_types.push("ProposalFinalizationCredential".to_string());
            },
            GovernanceEventType::ProposalExecuted => {
                credential_types.push("ProposalExecutionCredential".to_string());
            },
            GovernanceEventType::MandateIssued => {
                credential_types.push("MandateCredential".to_string());
            },
            GovernanceEventType::TrustBundleCreated => {
                credential_types.push("TrustBundleCredential".to_string());
            },
            GovernanceEventType::Custom(ref custom_type) => {
                credential_types.push(format!("{}Credential", custom_type));
            }
        }
        
        // Create credential subject
        let mut subject_map = serde_json::Map::new();
        
        // Add event data
        subject_map.insert("eventId".to_string(), serde_json::Value::String(self.id.to_string()));
        subject_map.insert("eventType".to_string(), serde_json::to_value(&self.event_type).unwrap());
        subject_map.insert("timestamp".to_string(), serde_json::Value::Number(self.timestamp.into()));
        
        // Add proposal CID if present
        if let Some(cid) = &self.proposal_cid {
            subject_map.insert("proposalCid".to_string(), serde_json::Value::String(cid.to_string()));
        }
        
        // Add scope information if present
        if let Some(scope_id) = &self.scope_id {
            subject_map.insert("scopeId".to_string(), serde_json::Value::String(scope_id.0.clone()));
        }
        
        if let Some(scope_type) = &self.scope_type {
            subject_map.insert("scopeType".to_string(), serde_json::to_value(scope_type).unwrap());
        }
        
        // Add event-specific data
        subject_map.insert("eventData".to_string(), self.data.clone());
        
        // Create the credential with the event emitter as the issuer
        VerifiableCredential::new(
            credential_types,
            self.emitter.0.clone(),
            self.emitter.0.clone(),
            serde_json::Value::Object(subject_map)
        )
    }
}

/// EventEmitter trait for components that need to emit governance events
#[async_trait::async_trait]
pub trait EventEmitter {
    /// Emit a governance event
    async fn emit_event(&self, event: GovernanceEvent) -> Result<Cid, String>;
    
    /// Emit a governance event and return it as a Verifiable Credential
    async fn emit_event_with_vc(&self, event: GovernanceEvent) -> Result<(Cid, VerifiableCredential), String> {
        // Convert the event to a VC
        let vc = event.to_verifiable_credential().await;
        
        // Emit the event
        let event_cid = self.emit_event(event).await?;
        
        Ok((event_cid, vc))
    }
} 