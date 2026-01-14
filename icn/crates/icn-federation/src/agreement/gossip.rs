//! Agreement Gossip Integration
//!
//! Gossip message types and handlers for synchronizing agreement state
//! across the federation network.

use super::store::AgreementStoreOps;
use super::types::{Agreement, AgreementId, AgreementSignature, AgreementStatus, Amendment};
use crate::error::{FederationError, Result};
use crate::TOPIC_FEDERATION_AGREEMENTS;
use icn_identity::Did;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{debug, info, warn};

/// Messages for agreement synchronization over gossip
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum AgreementMessage {
    /// A new agreement has been proposed
    Proposed {
        /// The full agreement object (boxed to reduce enum size)
        agreement: Box<Agreement>,
    },

    /// A party has signed an agreement
    Signed {
        /// Agreement ID
        agreement_id: AgreementId,
        /// The signature
        signature: AgreementSignature,
    },

    /// Agreement status has changed
    StatusUpdate {
        /// Agreement ID
        agreement_id: AgreementId,
        /// New status
        new_status: AgreementStatus,
        /// Who initiated the change
        updated_by: Did,
    },

    /// An amendment has been proposed
    AmendmentProposed {
        /// The amendment
        amendment: Amendment,
    },

    /// An amendment has been signed
    AmendmentSigned {
        /// Agreement ID
        agreement_id: AgreementId,
        /// Amendment ID
        amendment_id: String,
        /// The signature
        signature: AgreementSignature,
    },

    /// Request for agreement sync
    SyncRequest {
        /// DIDs of agreements we want
        agreement_ids: Vec<AgreementId>,
        /// Requester DID
        requester: Did,
    },

    /// Response with requested agreements
    SyncResponse {
        /// Requested agreements
        agreements: Vec<Agreement>,
        /// Responder DID
        responder: Did,
    },
}

impl AgreementMessage {
    /// Get the message type name for metrics/logging
    pub fn message_type(&self) -> &'static str {
        match self {
            AgreementMessage::Proposed { .. } => "proposed",
            AgreementMessage::Signed { .. } => "signed",
            AgreementMessage::StatusUpdate { .. } => "status_update",
            AgreementMessage::AmendmentProposed { .. } => "amendment_proposed",
            AgreementMessage::AmendmentSigned { .. } => "amendment_signed",
            AgreementMessage::SyncRequest { .. } => "sync_request",
            AgreementMessage::SyncResponse { .. } => "sync_response",
        }
    }

    /// Serialize to bytes for gossip
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        serde_json::to_vec(self).map_err(|e| FederationError::SerializationError(e.to_string()))
    }

    /// Deserialize from bytes
    pub fn from_bytes(data: &[u8]) -> Result<Self> {
        serde_json::from_slice(data)
            .map_err(|e| FederationError::DeserializationError(e.to_string()))
    }
}

/// Callback for sending agreement messages via gossip
pub type AgreementGossipCallback = Arc<dyn Fn(&str, Vec<u8>) -> Result<()> + Send + Sync>;

/// Handler for agreement gossip messages
pub struct AgreementGossipHandler<S: AgreementStoreOps> {
    store: Arc<S>,
    own_did: Did,
    #[allow(dead_code)] // Reserved for future use in message signing
    own_coop_id: String,
    send_callback: Option<AgreementGossipCallback>,
}

impl<S: AgreementStoreOps> AgreementGossipHandler<S> {
    /// Create a new agreement gossip handler
    pub fn new(store: Arc<S>, own_did: Did, own_coop_id: String) -> Self {
        Self {
            store,
            own_did,
            own_coop_id,
            send_callback: None,
        }
    }

    /// Set the callback for sending gossip messages
    pub fn set_send_callback(&mut self, callback: AgreementGossipCallback) {
        self.send_callback = Some(callback);
    }

    /// Publish an agreement message to the gossip network
    pub fn publish(&self, message: AgreementMessage) -> Result<()> {
        if let Some(ref callback) = self.send_callback {
            let data = message.to_bytes()?;
            callback(TOPIC_FEDERATION_AGREEMENTS, data)?;
            debug!("Published agreement message: {}", message.message_type());
        } else {
            warn!("No send callback configured for agreement gossip");
        }
        Ok(())
    }

    /// Handle an incoming agreement gossip message
    pub fn handle_message(&self, data: &[u8]) -> Result<()> {
        let message = AgreementMessage::from_bytes(data)?;

        debug!(
            "Received agreement message: {} from network",
            message.message_type()
        );

        match message {
            AgreementMessage::Proposed { agreement } => self.handle_proposed(*agreement),
            AgreementMessage::Signed {
                agreement_id,
                signature,
            } => self.handle_signed(agreement_id, signature),
            AgreementMessage::StatusUpdate {
                agreement_id,
                new_status,
                updated_by,
            } => self.handle_status_update(agreement_id, new_status, updated_by),
            AgreementMessage::AmendmentProposed { amendment } => {
                self.handle_amendment_proposed(amendment)
            }
            AgreementMessage::AmendmentSigned {
                agreement_id,
                amendment_id,
                signature,
            } => self.handle_amendment_signed(agreement_id, amendment_id, signature),
            AgreementMessage::SyncRequest {
                agreement_ids,
                requester,
            } => self.handle_sync_request(agreement_ids, requester),
            AgreementMessage::SyncResponse {
                agreements,
                responder,
            } => self.handle_sync_response(agreements, responder),
        }
    }

    /// Check if this node is a party to an agreement
    fn is_party_to(&self, agreement: &Agreement) -> bool {
        agreement.parties.iter().any(|p| p.did == self.own_did)
    }

    /// Handle a proposed agreement
    fn handle_proposed(&self, agreement: Agreement) -> Result<()> {
        // Only store if we're a party to this agreement
        if !self.is_party_to(&agreement) {
            debug!(
                "Ignoring proposed agreement {} - not a party",
                agreement.id
            );
            return Ok(());
        }

        // Check if we already have this agreement
        if self.store.get_agreement(&agreement.id)?.is_some() {
            debug!("Already have agreement {}", agreement.id);
            return Ok(());
        }

        // Validate the agreement
        if agreement.parties.len() < 2 {
            warn!("Invalid agreement {} - insufficient parties", agreement.id);
            return Err(FederationError::InvalidState(
                "Agreement must have at least 2 parties".to_string(),
            ));
        }

        self.store.store_agreement(&agreement)?;
        info!(
            "Stored proposed agreement {} from gossip",
            agreement.id
        );
        Ok(())
    }

    /// Handle a signature message
    fn handle_signed(&self, agreement_id: AgreementId, signature: AgreementSignature) -> Result<()> {
        // Get the agreement
        let mut agreement = match self.store.get_agreement(&agreement_id)? {
            Some(a) => a,
            None => {
                debug!(
                    "Received signature for unknown agreement {}",
                    agreement_id
                );
                return Ok(());
            }
        };

        // Only process if we're a party
        if !self.is_party_to(&agreement) {
            return Ok(());
        }

        // Verify the signer is a party
        if !agreement.parties.iter().any(|p| p.did == signature.signer) {
            warn!(
                "Signature from non-party {} for agreement {}",
                signature.signer, agreement_id
            );
            return Err(FederationError::InvalidState(
                "Signer is not a party to the agreement".to_string(),
            ));
        }

        // Don't add duplicate signatures
        if agreement.has_signed(&signature.signer) {
            debug!(
                "Already have signature from {} for {}",
                signature.signer, agreement_id
            );
            return Ok(());
        }

        // Add the signature
        agreement.signatures.push(signature.clone());

        // Check if all signatures are collected - auto-activate
        if agreement.status.is_proposed()
            && agreement.is_fully_signed()
            && agreement.activate().is_ok()
        {
            info!("Agreement {} activated via gossip signatures", agreement_id);
        }

        self.store.store_agreement(&agreement)?;
        debug!(
            "Recorded signature from {} for agreement {}",
            signature.signer, agreement_id
        );
        Ok(())
    }

    /// Handle a status update message
    fn handle_status_update(
        &self,
        agreement_id: AgreementId,
        new_status: AgreementStatus,
        updated_by: Did,
    ) -> Result<()> {
        let mut agreement = match self.store.get_agreement(&agreement_id)? {
            Some(a) => a,
            None => return Ok(()),
        };

        if !self.is_party_to(&agreement) {
            return Ok(());
        }

        // Verify the updater is a party
        if !agreement.parties.iter().any(|p| p.did == updated_by) {
            warn!(
                "Status update from non-party {} for agreement {}",
                updated_by, agreement_id
            );
            return Err(FederationError::Unauthorized(
                "Updater is not a party".to_string(),
            ));
        }

        // Apply the status update
        agreement.status = new_status;
        self.store.store_agreement(&agreement)?;

        info!(
            "Updated status of agreement {} via gossip",
            agreement_id
        );
        Ok(())
    }

    /// Handle a proposed amendment
    fn handle_amendment_proposed(&self, amendment: Amendment) -> Result<()> {
        // Verify we have the agreement
        let agreement = match self.store.get_agreement(&amendment.agreement_id)? {
            Some(a) => a,
            None => {
                debug!(
                    "Amendment for unknown agreement {}",
                    amendment.agreement_id
                );
                return Ok(());
            }
        };

        if !self.is_party_to(&agreement) {
            return Ok(());
        }

        // Verify the proposer is a party
        if !agreement.parties.iter().any(|p| p.did == amendment.proposed_by) {
            warn!(
                "Amendment from non-party {} for agreement {}",
                amendment.proposed_by, amendment.agreement_id
            );
            return Err(FederationError::Unauthorized(
                "Amendment proposer is not a party".to_string(),
            ));
        }

        self.store.store_amendment(&amendment)?;
        info!(
            "Stored amendment {} for agreement {} from gossip",
            amendment.id, amendment.agreement_id
        );
        Ok(())
    }

    /// Handle an amendment signature
    fn handle_amendment_signed(
        &self,
        agreement_id: AgreementId,
        amendment_id: String,
        signature: AgreementSignature,
    ) -> Result<()> {
        let agreement = match self.store.get_agreement(&agreement_id)? {
            Some(a) => a,
            None => return Ok(()),
        };

        if !self.is_party_to(&agreement) {
            return Ok(());
        }

        // Get the amendment
        let mut amendments = self.store.get_amendments(&agreement_id)?;
        let amendment = match amendments.iter_mut().find(|a| a.id == amendment_id) {
            Some(a) => a,
            None => {
                debug!("Amendment {} not found for agreement {}", amendment_id, agreement_id);
                return Ok(());
            }
        };

        // Add signature if not duplicate
        if !amendment.signatures.iter().any(|s| s.signer == signature.signer) {
            amendment.add_signature(signature);
            self.store.store_amendment(amendment)?;
            debug!(
                "Recorded amendment signature from {} for {}",
                amendment.signatures.last().map(|s| s.signer.to_string()).unwrap_or_default(),
                amendment_id
            );
        }

        Ok(())
    }

    /// Handle a sync request
    fn handle_sync_request(&self, agreement_ids: Vec<AgreementId>, requester: Did) -> Result<()> {
        // Only respond if we have agreements to share
        let mut agreements = Vec::new();

        for id in agreement_ids {
            if let Some(agreement) = self.store.get_agreement(&id)? {
                // Only share if the requester is a party
                if agreement.parties.iter().any(|p| p.did == requester) {
                    agreements.push(agreement);
                }
            }
        }

        if !agreements.is_empty() {
            self.publish(AgreementMessage::SyncResponse {
                agreements,
                responder: self.own_did.clone(),
            })?;
        }

        Ok(())
    }

    /// Handle a sync response
    fn handle_sync_response(&self, agreements: Vec<Agreement>, _responder: Did) -> Result<()> {
        for agreement in agreements {
            if self.is_party_to(&agreement) {
                // Only update if we don't have it or ours is older
                let should_store = match self.store.get_agreement(&agreement.id)? {
                    Some(existing) => agreement.updated_at > existing.updated_at,
                    None => true,
                };

                if should_store {
                    self.store.store_agreement(&agreement)?;
                    debug!("Synced agreement {} from gossip", agreement.id);
                }
            }
        }
        Ok(())
    }

    /// Request sync for specific agreements
    pub fn request_sync(&self, agreement_ids: Vec<AgreementId>) -> Result<()> {
        if agreement_ids.is_empty() {
            return Ok(());
        }

        self.publish(AgreementMessage::SyncRequest {
            agreement_ids,
            requester: self.own_did.clone(),
        })
    }

    /// Broadcast a proposed agreement
    pub fn broadcast_proposed(&self, agreement: &Agreement) -> Result<()> {
        self.publish(AgreementMessage::Proposed {
            agreement: Box::new(agreement.clone()),
        })
    }

    /// Broadcast a signature
    pub fn broadcast_signature(
        &self,
        agreement_id: &AgreementId,
        signature: AgreementSignature,
    ) -> Result<()> {
        self.publish(AgreementMessage::Signed {
            agreement_id: agreement_id.clone(),
            signature,
        })
    }

    /// Broadcast a status update
    pub fn broadcast_status_update(
        &self,
        agreement_id: &AgreementId,
        new_status: AgreementStatus,
    ) -> Result<()> {
        self.publish(AgreementMessage::StatusUpdate {
            agreement_id: agreement_id.clone(),
            new_status,
            updated_by: self.own_did.clone(),
        })
    }

    /// Broadcast a proposed amendment
    pub fn broadcast_amendment(&self, amendment: &Amendment) -> Result<()> {
        self.publish(AgreementMessage::AmendmentProposed {
            amendment: amendment.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agreement::{AgreementType, InMemoryAgreementStore, PartyRole, TradeItem};
    use icn_identity::KeyPair;
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn create_test_agreement() -> Agreement {
        let proposer = KeyPair::generate().unwrap();
        let counterparty = KeyPair::generate().unwrap();

        Agreement::new(
            "Test Agreement",
            "For testing gossip",
            AgreementType::Trade {
                items: vec![TradeItem::new("item", 10, "units", 100, "USD")],
                currency: "USD".to_string(),
            },
        )
        .with_party(proposer.did().clone(), "coop-a", PartyRole::Proposer)
        .with_party(counterparty.did().clone(), "coop-b", PartyRole::Counterparty)
    }

    #[test]
    fn test_message_serialization() {
        let agreement = create_test_agreement();

        let message = AgreementMessage::Proposed {
            agreement: Box::new(agreement.clone()),
        };

        let bytes = message.to_bytes().unwrap();
        let decoded = AgreementMessage::from_bytes(&bytes).unwrap();

        match decoded {
            AgreementMessage::Proposed { agreement: decoded_agreement } => {
                assert_eq!(decoded_agreement.id, agreement.id);
            }
            _ => panic!("Wrong message type"),
        }
    }

    #[test]
    fn test_handler_filters_non_party_agreements() {
        let store = Arc::new(InMemoryAgreementStore::new());
        let own_kp = KeyPair::generate().unwrap();
        let handler = AgreementGossipHandler::new(
            store.clone(),
            own_kp.did().clone(),
            "our-coop".to_string(),
        );

        // Create an agreement where we're NOT a party
        let agreement = create_test_agreement();

        // Handle the proposed message
        let message = AgreementMessage::Proposed {
            agreement: Box::new(agreement.clone()),
        };
        handler.handle_message(&message.to_bytes().unwrap()).unwrap();

        // Should not be stored since we're not a party
        assert!(store.get_agreement(&agreement.id).unwrap().is_none());
    }

    #[test]
    fn test_handler_stores_party_agreements() {
        let store = Arc::new(InMemoryAgreementStore::new());
        let own_kp = KeyPair::generate().unwrap();
        let counterparty = KeyPair::generate().unwrap();

        let handler = AgreementGossipHandler::new(
            store.clone(),
            own_kp.did().clone(),
            "our-coop".to_string(),
        );

        // Create an agreement where we ARE a party
        let agreement = Agreement::new(
            "Test Agreement",
            "For testing",
            AgreementType::Credit {
                credit_limit: 5000,
                interest_rate_bps: 300,
                currency: "USD".to_string(),
            },
        )
        .with_party(own_kp.did().clone(), "our-coop", PartyRole::Proposer)
        .with_party(counterparty.did().clone(), "other-coop", PartyRole::Counterparty);

        let message = AgreementMessage::Proposed {
            agreement: Box::new(agreement.clone()),
        };
        handler.handle_message(&message.to_bytes().unwrap()).unwrap();

        // Should be stored since we're a party
        let stored = store.get_agreement(&agreement.id).unwrap();
        assert!(stored.is_some());
        assert_eq!(stored.unwrap().title, "Test Agreement");
    }

    #[test]
    fn test_publish_with_callback() {
        let store = Arc::new(InMemoryAgreementStore::new());
        let own_kp = KeyPair::generate().unwrap();
        let mut handler = AgreementGossipHandler::new(
            store,
            own_kp.did().clone(),
            "our-coop".to_string(),
        );

        let publish_count = Arc::new(AtomicUsize::new(0));
        let count_clone = publish_count.clone();

        handler.set_send_callback(Arc::new(move |_topic, _data| {
            count_clone.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }));

        let agreement = create_test_agreement();
        handler.broadcast_proposed(&agreement).unwrap();

        assert_eq!(publish_count.load(Ordering::SeqCst), 1);
    }
}
