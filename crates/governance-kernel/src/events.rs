/*!
# Governance Events

This module defines the event and credential structure for governance actions.
Events are emitted when governance actions occur, and credentials are generated
to provide verifiable proofs of these actions.
*/

use serde::{Serialize, Deserialize};
use icn_identity::{IdentityId, IdentityScope, VerifiableCredential};
use uuid::Uuid;
use std::time::{SystemTime, UNIX_EPOCH};
use async_trait;

/// Types of governance events that can be emitted
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase")]
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
    /// A trust bundle was updated
    TrustBundleUpdated,
    /// A custom event
    Custom(String),
}

/// Status of an event (for filtering)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum EventStatus {
    /// Event was successful
    Success,
    /// Event failed
    Failed,
    /// Event is pending
    Pending,
}

/// A governance event that can be emitted by the kernel
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GovernanceEvent {
    /// Unique ID for the event
    pub id: String,
    /// Type of event
    pub event_type: GovernanceEventType,
    /// Timestamp of the event (seconds since UNIX epoch)
    pub timestamp: u64,
    /// The identity that triggered the event
    pub issuer: IdentityId,
    /// The scope of the event (e.g., Federation, Community)
    pub scope: IdentityScope,
    /// The organization or federation this event belongs to (if any)
    pub organization: Option<IdentityId>,
    /// The proposal ID this event relates to (if any)
    pub proposal_cid: Option<String>,
    /// Status of the event
    pub status: EventStatus,
    /// Additional data specific to the event type (JSON-encoded)
    pub data: serde_json::Value,
}

impl GovernanceEvent {
    /// Create a new governance event
    pub fn new(
        event_type: GovernanceEventType,
        issuer: IdentityId,
        scope: IdentityScope,
        organization: Option<IdentityId>,
        proposal_cid: Option<String>,
        data: serde_json::Value,
    ) -> Self {
        let id = Uuid::new_v4().to_string();
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
            
        Self {
            id,
            event_type,
            timestamp,
            issuer,
            scope,
            organization,
            proposal_cid,
            status: EventStatus::Success,
            data,
        }
    }
    
    /// Convert the event to a VerifiableCredential
    pub fn to_credential(&self, issuer_did: &str) -> VerifiableCredential {
        let mut credential_types = vec!["GovernanceCredential".to_string()];
        
        // Add specific credential type based on event type
        match self.event_type {
            GovernanceEventType::ProposalCreated => credential_types.push("ProposalCreationCredential".to_string()),
            GovernanceEventType::VoteCast => credential_types.push("VoteCastCredential".to_string()),
            GovernanceEventType::ProposalFinalized => credential_types.push("ProposalFinalizationCredential".to_string()),
            GovernanceEventType::ProposalExecuted => credential_types.push("ProposalExecutionCredential".to_string()),
            GovernanceEventType::MandateIssued => credential_types.push("MandateIssuanceCredential".to_string()),
            GovernanceEventType::TrustBundleCreated => credential_types.push("TrustBundleCreationCredential".to_string()),
            GovernanceEventType::TrustBundleUpdated => credential_types.push("TrustBundleUpdateCredential".to_string()),
            GovernanceEventType::Custom(ref name) => credential_types.push(format!("{}Credential", name)),
        }
        
        // Create credential subject with event data
        let mut subject_map = serde_json::Map::new();
        
        // Add standard fields
        subject_map.insert("eventId".to_string(), serde_json::Value::String(self.id.clone()));
        subject_map.insert("eventType".to_string(), serde_json::Value::String(format!("{:?}", self.event_type)));
        subject_map.insert("timestamp".to_string(), serde_json::Value::Number(serde_json::Number::from(self.timestamp)));
        subject_map.insert("issuerId".to_string(), serde_json::Value::String(self.issuer.0.clone()));
        subject_map.insert("scope".to_string(), serde_json::Value::String(format!("{:?}", self.scope)));
        
        if let Some(org) = &self.organization {
            subject_map.insert("organizationId".to_string(), serde_json::Value::String(org.0.clone()));
        }
        
        if let Some(proposal_id) = &self.proposal_cid {
            subject_map.insert("proposalId".to_string(), serde_json::Value::String(proposal_id.clone()));
        }
        
        // Add event-specific data
        subject_map.insert("eventData".to_string(), self.data.clone());
        
        // Create the credential with the event emitter as the issuer
        let issuer_identity_id = IdentityId::new(issuer_did);
        let subject_identity_id = self.issuer.clone();
        
        VerifiableCredential::new(
            credential_types,
            &issuer_identity_id,
            &subject_identity_id,
            serde_json::Value::Object(subject_map)
        )
    }
}

/// Interface for components that can emit governance events
#[async_trait::async_trait]
pub trait EventEmitter {
    /// Emit a governance event
    async fn emit_event(&self, event: GovernanceEvent) -> Result<String, String>;
    
    /// Get events related to a specific proposal
    async fn get_events_for_proposal(&self, proposal_id: String) -> Result<Vec<GovernanceEvent>, String>;
    
    /// Get credentials related to a specific proposal
    async fn get_credentials_for_proposal(&self, proposal_id: String) -> Result<Vec<VerifiableCredential>, String>;
}

/// A simple in-memory event emitter for testing
#[derive(Clone)]
pub struct InMemoryEventEmitter(pub IdentityId);

#[async_trait::async_trait]
impl EventEmitter for InMemoryEventEmitter {
    async fn emit_event(&self, event: GovernanceEvent) -> Result<String, String> {
        println!("Emitted event: {:?}", event);
        
        // Return event ID
        Ok(event.id.clone())
    }
    
    async fn get_events_for_proposal(&self, _proposal_id: String) -> Result<Vec<GovernanceEvent>, String> {
        // This is just a stub for testing
        Ok(Vec::new())
    }
    
    async fn get_credentials_for_proposal(&self, _proposal_id: String) -> Result<Vec<VerifiableCredential>, String> {
        // This is just a stub for testing
        Ok(Vec::new())
    }
} 