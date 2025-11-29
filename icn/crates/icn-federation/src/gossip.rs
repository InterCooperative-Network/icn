//! Federation Gossip Handler (Phase F1)
//!
//! Handles gossip messages for federation coordination, including:
//! - Cooperative announcements and discovery
//! - Vouch messages for policy enforcement
//! - Federation requests and responses

use crate::error::{FederationError, Result};
use crate::metrics;
use crate::registry::CooperativeRegistry;
use crate::types::{current_timestamp, CooperativeInfo, FederationMessage, Vouch};
use crate::{TOPIC_FEDERATION_CLEARING, TOPIC_FEDERATION_REGISTRY, TOPIC_FEDERATION_TRUST};
use icn_identity::Did;
use std::sync::Arc;
use std::sync::RwLock;
use tracing::{debug, info, warn};

/// Callback for sending gossip messages to the network
pub type GossipSendCallback = Arc<dyn Fn(&str, Vec<u8>) -> Result<()> + Send + Sync>;

/// Handler for federation-related gossip messages
pub struct FederationGossipHandler {
    /// The cooperative registry
    registry: Arc<CooperativeRegistry>,

    /// Our cooperative info
    own_coop: RwLock<Option<CooperativeInfo>>,

    /// Callback to send messages via gossip
    send_callback: RwLock<Option<GossipSendCallback>>,
}

impl FederationGossipHandler {
    /// Create a new federation gossip handler
    pub fn new(registry: Arc<CooperativeRegistry>) -> Self {
        Self {
            registry,
            own_coop: RwLock::new(None),
            send_callback: RwLock::new(None),
        }
    }

    /// Set the send callback for gossip messages
    pub fn set_send_callback(&self, callback: GossipSendCallback) {
        *self.send_callback.write().unwrap() = Some(callback);
    }

    /// Set our own cooperative info
    pub fn set_own_coop(&self, coop: CooperativeInfo) {
        *self.own_coop.write().unwrap() = Some(coop);
    }

    /// Get our cooperative ID
    pub fn own_coop_id(&self) -> Option<String> {
        self.own_coop
            .read()
            .unwrap()
            .as_ref()
            .map(|c| c.coop_id.clone())
    }

    /// Handle an incoming federation message
    pub fn handle_message(&self, topic: &str, data: &[u8]) -> Result<()> {
        let message: FederationMessage = serde_json::from_slice(data)
            .map_err(|e| FederationError::DeserializationError(e.to_string()))?;

        match topic {
            t if t == TOPIC_FEDERATION_REGISTRY => self.handle_registry_message(message),
            t if t == TOPIC_FEDERATION_TRUST => self.handle_trust_message(message),
            t if t == TOPIC_FEDERATION_CLEARING => self.handle_clearing_message(message),
            _ => {
                debug!("Ignoring message on unknown federation topic: {}", topic);
                Ok(())
            }
        }
    }

    /// Handle registry-related messages
    fn handle_registry_message(&self, message: FederationMessage) -> Result<()> {
        match message {
            FederationMessage::CoopAnnounce(coop_info) => self.handle_coop_announce(coop_info),
            FederationMessage::CoopQuery { coop_id } => self.handle_coop_query(coop_id),
            FederationMessage::CoopResponse { cooperatives } => {
                self.handle_coop_response(cooperatives)
            }
            FederationMessage::Vouch(vouch) => self.handle_vouch(vouch),
            FederationMessage::FederationRequest { requester } => {
                self.handle_federation_request(requester)
            }
            FederationMessage::FederationAccept {
                accepter_coop_id,
                requester_coop_id,
                ..
            } => self.handle_federation_accept(&accepter_coop_id, &requester_coop_id),
            FederationMessage::FederationReject {
                rejecter_coop_id,
                requester_coop_id,
                reason,
            } => self.handle_federation_reject(&rejecter_coop_id, &requester_coop_id, &reason),
        }
    }

    /// Handle trust-related messages (attestations)
    fn handle_trust_message(&self, _message: FederationMessage) -> Result<()> {
        // Trust messages are handled by AttestationStore
        // This is a passthrough - actual handling delegated to attestation layer
        Ok(())
    }

    /// Handle clearing-related messages
    fn handle_clearing_message(&self, _message: FederationMessage) -> Result<()> {
        // Clearing messages are handled by ClearingManager
        // This is a passthrough - actual handling delegated to clearing layer
        Ok(())
    }

    /// Handle a cooperative announcement
    fn handle_coop_announce(&self, coop_info: CooperativeInfo) -> Result<()> {
        metrics::registry::announcements_received_inc();

        // Don't process our own announcements
        {
            let own_coop = self.own_coop.read().unwrap();
            if let Some(ref own) = *own_coop {
                if own.coop_id == coop_info.coop_id {
                    return Ok(());
                }
            }
        }

        // Verify signature before accepting
        // TODO: Verify signature using coop_info.public_did

        // Check if already registered
        if self.registry.get(&coop_info.coop_id)?.is_some() {
            // Update last_seen timestamp
            self.registry
                .update_last_seen(&coop_info.coop_id, current_timestamp())?;
            debug!("Updated last_seen for cooperative: {}", coop_info.coop_id);
        } else {
            // Check policy before registering
            let policy_result = self.registry.check_policy(&coop_info)?;
            if policy_result.is_allowed() {
                self.registry.register(coop_info.clone())?;
                info!(
                    "Registered new cooperative from announcement: {}",
                    coop_info.coop_id
                );
            } else {
                debug!(
                    "Cooperative {} not registered due to policy: {:?}",
                    coop_info.coop_id, policy_result
                );
            }
        }

        Ok(())
    }

    /// Handle a cooperative query
    fn handle_coop_query(&self, coop_id: Option<String>) -> Result<()> {
        let cooperatives = if let Some(id) = coop_id {
            match self.registry.get(&id)? {
                Some(coop) => vec![coop],
                None => vec![],
            }
        } else {
            self.registry.list()?
        };

        // Send response
        let response = FederationMessage::CoopResponse { cooperatives };
        self.send_message(TOPIC_FEDERATION_REGISTRY, &response)?;

        Ok(())
    }

    /// Handle a cooperative response
    fn handle_coop_response(&self, cooperatives: Vec<CooperativeInfo>) -> Result<()> {
        for coop_info in cooperatives {
            // Skip our own info
            {
                let own_coop = self.own_coop.read().unwrap();
                if let Some(ref own) = *own_coop {
                    if own.coop_id == coop_info.coop_id {
                        continue;
                    }
                }
            }

            // Check if we should register
            if self.registry.get(&coop_info.coop_id)?.is_none() {
                let policy_result = self.registry.check_policy(&coop_info)?;
                if policy_result.is_allowed() {
                    self.registry.register(coop_info.clone())?;
                    debug!("Registered cooperative from response: {}", coop_info.coop_id);
                }
            }
        }

        Ok(())
    }

    /// Handle a vouch message
    fn handle_vouch(&self, vouch: Vouch) -> Result<()> {
        metrics::registry::vouches_received_inc(&vouch.voucher_coop_id);

        // Verify the vouch hasn't expired
        if vouch.is_expired() {
            warn!("Received expired vouch from {}", vouch.voucher_coop_id);
            return Ok(());
        }

        // Verify the voucher is a known cooperative
        if self.registry.get(&vouch.voucher_coop_id)?.is_none() {
            warn!(
                "Received vouch from unknown cooperative: {}",
                vouch.voucher_coop_id
            );
            return Ok(());
        }

        // TODO: Verify vouch signature using voucher's DID

        // Store the vouch
        let target = vouch.target_coop_id.clone();
        let voucher = vouch.voucher_coop_id.clone();
        self.registry.add_vouch(&vouch)?;
        debug!("Stored vouch from {} for {}", voucher, target);

        Ok(())
    }

    /// Handle a federation request
    fn handle_federation_request(&self, requester: CooperativeInfo) -> Result<()> {
        info!(
            "Received federation request from: {} ({})",
            requester.name, requester.coop_id
        );

        // Check policy
        let policy_result = self.registry.check_policy(&requester)?;

        if policy_result.is_allowed() {
            // Auto-accept if policy allows
            self.accept_federation(&requester.coop_id)?;
        } else {
            debug!(
                "Federation request from {} pending policy check: {:?}",
                requester.coop_id, policy_result
            );
            // Request stays pending until policy is satisfied
        }

        Ok(())
    }

    /// Handle federation acceptance
    fn handle_federation_accept(
        &self,
        accepter_coop_id: &str,
        requester_coop_id: &str,
    ) -> Result<()> {
        info!(
            "Federation accepted: {} accepted {}",
            accepter_coop_id, requester_coop_id
        );

        // If we're the requester, mark the accepter as federated
        {
            let own_coop = self.own_coop.read().unwrap();
            if let Some(ref own) = *own_coop {
                if own.coop_id == requester_coop_id {
                    // The accepter is now our federation partner
                    self.registry
                        .update_last_seen(accepter_coop_id, current_timestamp())?;
                }
            }
        }

        Ok(())
    }

    /// Handle federation rejection
    fn handle_federation_reject(
        &self,
        rejecter_coop_id: &str,
        requester_coop_id: &str,
        reason: &str,
    ) -> Result<()> {
        info!(
            "Federation rejected: {} rejected {} - {}",
            rejecter_coop_id, requester_coop_id, reason
        );

        Ok(())
    }

    /// Announce our cooperative to the network
    pub fn announce(&self) -> Result<()> {
        let coop = self.own_coop.read().unwrap().clone();

        if let Some(coop_info) = coop {
            let message = FederationMessage::CoopAnnounce(coop_info);
            self.send_message(TOPIC_FEDERATION_REGISTRY, &message)?;
            metrics::registry::announcements_sent_inc();
            debug!("Announced cooperative to federation");
        } else {
            return Err(FederationError::NotInitialized(
                "Own cooperative info not set".to_string(),
            ));
        }

        Ok(())
    }

    /// Query for cooperatives
    pub fn query_cooperatives(&self, coop_id: Option<String>) -> Result<()> {
        let message = FederationMessage::CoopQuery { coop_id };
        self.send_message(TOPIC_FEDERATION_REGISTRY, &message)?;
        Ok(())
    }

    /// Send a vouch for another cooperative
    pub fn send_vouch(&self, target_coop_id: &str, voucher_did: Did, trust_score: f64) -> Result<()> {
        let own_coop_id = self
            .own_coop_id()
            .ok_or_else(|| FederationError::NotInitialized("Own coop not set".to_string()))?;

        let vouch = Vouch::new(own_coop_id.clone(), voucher_did, target_coop_id.to_string(), trust_score);

        // TODO: Sign the vouch

        let message = FederationMessage::Vouch(vouch);
        self.send_message(TOPIC_FEDERATION_REGISTRY, &message)?;
        metrics::registry::vouches_sent_inc(target_coop_id);

        info!("Sent vouch for {} from {} with trust score {:.2}", target_coop_id, own_coop_id, trust_score);
        Ok(())
    }

    /// Request to federate with another cooperative
    pub fn request_federation(&self) -> Result<()> {
        let coop = self.own_coop.read().unwrap().clone();

        if let Some(coop_info) = coop {
            let message = FederationMessage::FederationRequest { requester: coop_info };
            self.send_message(TOPIC_FEDERATION_REGISTRY, &message)?;
            debug!("Sent federation request");
        } else {
            return Err(FederationError::NotInitialized(
                "Own cooperative info not set".to_string(),
            ));
        }

        Ok(())
    }

    /// Accept a federation request
    pub fn accept_federation(&self, requester_coop_id: &str) -> Result<()> {
        let own_coop_id = self
            .own_coop_id()
            .ok_or_else(|| FederationError::NotInitialized("Own coop not set".to_string()))?;

        // TODO: Sign the acceptance

        let message = FederationMessage::FederationAccept {
            accepter_coop_id: own_coop_id,
            requester_coop_id: requester_coop_id.to_string(),
            signature: Vec::new(),
        };
        self.send_message(TOPIC_FEDERATION_REGISTRY, &message)?;

        info!("Accepted federation request from {}", requester_coop_id);
        Ok(())
    }

    /// Reject a federation request
    pub fn reject_federation(&self, requester_coop_id: &str, reason: &str) -> Result<()> {
        let own_coop_id = self
            .own_coop_id()
            .ok_or_else(|| FederationError::NotInitialized("Own coop not set".to_string()))?;

        let message = FederationMessage::FederationReject {
            rejecter_coop_id: own_coop_id,
            requester_coop_id: requester_coop_id.to_string(),
            reason: reason.to_string(),
        };
        self.send_message(TOPIC_FEDERATION_REGISTRY, &message)?;

        info!(
            "Rejected federation request from {}: {}",
            requester_coop_id, reason
        );
        Ok(())
    }

    /// Send a message via the gossip callback
    fn send_message(&self, topic: &str, message: &FederationMessage) -> Result<()> {
        let callback = self.send_callback.read().unwrap();

        if let Some(ref cb) = *callback {
            let data = serde_json::to_vec(message)?;
            cb(topic, data)?;
            Ok(())
        } else {
            Err(FederationError::NotInitialized(
                "Send callback not set".to_string(),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::FederationPolicy;
    use icn_identity::KeyPair;
    use icn_store::{SledStore, Store};
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn test_did() -> Did {
        KeyPair::generate().unwrap().did().clone()
    }

    fn create_test_coop(id: &str) -> CooperativeInfo {
        CooperativeInfo::new(
            id.to_string(),
            format!("{} Cooperative", id),
            test_did(),
            FederationPolicy::Open,
        )
    }

    fn create_test_registry() -> Arc<CooperativeRegistry> {
        let store = Arc::new(SledStore::temporary().unwrap()) as Arc<dyn Store>;
        let own_info = create_test_coop("test-coop");
        Arc::new(CooperativeRegistry::new(store, own_info).unwrap())
    }

    #[test]
    fn test_handler_announcement() {
        let registry = create_test_registry();
        let handler = FederationGossipHandler::new(registry.clone());

        // Set up our own coop
        handler.set_own_coop(create_test_coop("my-coop"));

        // Set up send callback
        let send_count = Arc::new(AtomicUsize::new(0));
        let count_clone = send_count.clone();
        handler.set_send_callback(Arc::new(move |_topic, _data| {
            count_clone.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }));

        // Announce
        handler.announce().unwrap();
        assert_eq!(send_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_handler_receive_announcement() {
        let registry = create_test_registry();
        let handler = FederationGossipHandler::new(registry.clone());

        // Set up our own coop
        handler.set_own_coop(create_test_coop("my-coop"));

        // Receive announcement from another coop
        let other_coop = create_test_coop("other-coop");
        let message = FederationMessage::CoopAnnounce(other_coop);
        let data = serde_json::to_vec(&message).unwrap();

        handler.handle_message("federation:registry", &data).unwrap();

        // Should be registered
        let registered = registry.get("other-coop").unwrap();
        assert!(registered.is_some());
    }

    #[test]
    fn test_handler_ignores_own_announcement() {
        let registry = create_test_registry();
        let handler = FederationGossipHandler::new(registry.clone());

        // Set up our own coop
        let my_coop = create_test_coop("my-coop");
        handler.set_own_coop(my_coop.clone());

        // Receive our own announcement (shouldn't register)
        let message = FederationMessage::CoopAnnounce(my_coop);
        let data = serde_json::to_vec(&message).unwrap();

        handler.handle_message("federation:registry", &data).unwrap();

        // Should NOT be registered (it's our own)
        let registered = registry.get("my-coop").unwrap();
        assert!(registered.is_none());
    }
}
