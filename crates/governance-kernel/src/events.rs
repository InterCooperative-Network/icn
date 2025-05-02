/*!
# Governance Events

This module defines the event and credential structure for governance actions.
Events are emitted when governance actions occur, and credentials are generated
to provide verifiable proofs of these actions.
*/

use cid::Cid;
use serde::{Serialize, Deserialize};
use icn_identity::{IdentityId, IdentityScope, VerifiableCredential};
use uuid::Uuid;
use std::time::{SystemTime, UNIX_EPOCH};
use std::collections::HashMap;

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
    /// The proposal CID this event relates to (if any)
    pub proposal_cid: Option<Cid>,
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
        proposal_cid: Option<Cid>,
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
        subject_map.insert("issuerId".to_string(), serde_json::Value::String(self.issuer.to_string()));
        subject_map.insert("scope".to_string(), serde_json::Value::String(format!("{:?}", self.scope)));
        
        if let Some(org) = &self.organization {
            subject_map.insert("organizationId".to_string(), serde_json::Value::String(org.to_string()));
        }
        
        if let Some(cid) = &self.proposal_cid {
            subject_map.insert("proposalCid".to_string(), serde_json::Value::String(cid.to_string()));
        }
        
        // Add event-specific data
        subject_map.insert("eventData".to_string(), self.data.clone());
        
        // Create the credential with the event emitter as the issuer
        VerifiableCredential::new(
            credential_types,
            issuer_did.to_string(),
            // Add subject DID - using the event issuer as the subject
            self.issuer.to_string(),
            serde_json::Value::Object(subject_map)
        )
    }
}

/// Interface for components that can emit governance events
pub trait EventEmitter {
    /// Emit a governance event
    fn emit_event(&self, event: GovernanceEvent) -> Result<(), String>;
    
    /// Get events related to a specific proposal
    fn get_events_for_proposal(&self, proposal_cid: Cid) -> Result<Vec<GovernanceEvent>, String>;
    
    /// Get credentials related to a specific proposal
    fn get_credentials_for_proposal(&self, proposal_cid: Cid) -> Result<Vec<VerifiableCredential>, String>;
}

/// A simple in-memory event emitter for testing
#[derive(Clone)]
pub struct InMemoryEventEmitter(pub IdentityId);

impl EventEmitter for InMemoryEventEmitter {
    fn emit_event(&self, event: GovernanceEvent) -> Result<(), String> {
        println!("Emitted event: {:?}", event);
        Ok(())
    }
    
    fn get_events_for_proposal(&self, proposal_cid: Cid) -> Result<Vec<GovernanceEvent>, String> {
        // This is just a stub for testing
        Ok(Vec::new())
    }
    
    fn get_credentials_for_proposal(&self, proposal_cid: Cid) -> Result<Vec<VerifiableCredential>, String> {
        // This is just a stub for testing
        Ok(Vec::new())
    }
} 