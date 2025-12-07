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
use icn_identity::{Did, KeyPair};
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

    /// Keypair for signing outgoing messages
    keypair: RwLock<Option<KeyPair>>,

    /// Callback to send messages via gossip
    send_callback: RwLock<Option<GossipSendCallback>>,
}

impl FederationGossipHandler {
    /// Create a new federation gossip handler
    pub fn new(registry: Arc<CooperativeRegistry>) -> Self {
        Self {
            registry,
            own_coop: RwLock::new(None),
            keypair: RwLock::new(None),
            send_callback: RwLock::new(None),
        }
    }

    /// Set the send callback for gossip messages
    pub fn set_send_callback(&self, callback: GossipSendCallback) {
        *self.send_callback.write().unwrap() = Some(callback);
    }

    /// Set the keypair for signing outgoing messages
    pub fn set_keypair(&self, keypair: KeyPair) {
        *self.keypair.write().unwrap() = Some(keypair);
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
                signature,
            } => self.handle_federation_accept(&accepter_coop_id, &requester_coop_id, &signature),
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
        if let Err(e) = coop_info.verify_signature() {
            warn!(
                "Rejected announcement from {} - invalid signature: {}",
                coop_info.coop_id, e
            );
            metrics::registry::signature_verification_failed_inc("coop_announce");
            return Ok(()); // Don't propagate error, just reject silently
        }
        debug!(
            "Verified signature for cooperative announcement: {}",
            coop_info.coop_id
        );

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
                    debug!(
                        "Registered cooperative from response: {}",
                        coop_info.coop_id
                    );
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

        // Verify vouch signature using voucher's DID
        if let Err(e) = vouch.verify_signature() {
            warn!(
                "Rejected vouch from {} - invalid signature: {}",
                vouch.voucher_coop_id, e
            );
            metrics::registry::signature_verification_failed_inc("vouch");
            return Ok(()); // Don't propagate error, just reject silently
        }
        debug!(
            "Verified signature for vouch from {} for {}",
            vouch.voucher_coop_id, vouch.target_coop_id
        );

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
        signature: &[u8],
    ) -> Result<()> {
        // Verify signature before accepting
        // Look up accepter's DID from registry
        let accepter_coop = self.registry.get(accepter_coop_id)?;
        let accepter_did = match accepter_coop {
            Some(coop) => coop.public_did.clone(),
            None => {
                warn!(
                    "Rejected federation accept from unknown cooperative: {}",
                    accepter_coop_id
                );
                return Ok(()); // Silently reject unknown accepters
            }
        };

        // Verify signature
        if signature.is_empty() {
            warn!(
                "Rejected federation accept from {} - missing signature",
                accepter_coop_id
            );
            metrics::registry::signature_verification_failed_inc("federation_accept");
            return Ok(());
        }

        // Recreate signing bytes
        let signing_bytes = {
            let mut bytes = Vec::new();
            bytes.extend(b"federation_accept:");
            bytes.extend(accepter_coop_id.as_bytes());
            bytes.extend(b":");
            bytes.extend(requester_coop_id.as_bytes());
            bytes
        };

        // Verify signature using accepter's DID
        let verifying_key = match accepter_did.to_verifying_key() {
            Ok(key) => key,
            Err(e) => {
                warn!(
                    "Rejected federation accept from {} - invalid DID: {}",
                    accepter_coop_id, e
                );
                metrics::registry::signature_verification_failed_inc("federation_accept");
                return Ok(());
            }
        };

        let sig = match ed25519_dalek::Signature::from_slice(signature) {
            Ok(s) => s,
            Err(e) => {
                warn!(
                    "Rejected federation accept from {} - invalid signature format: {}",
                    accepter_coop_id, e
                );
                metrics::registry::signature_verification_failed_inc("federation_accept");
                return Ok(());
            }
        };

        use ed25519_dalek::Verifier;
        if let Err(e) = verifying_key.verify(&signing_bytes, &sig) {
            warn!(
                "Rejected federation accept from {} - signature verification failed: {}",
                accepter_coop_id, e
            );
            metrics::registry::signature_verification_failed_inc("federation_accept");
            return Ok(());
        }

        debug!(
            "Verified signature for federation accept from {}",
            accepter_coop_id
        );

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
        let keypair = self.keypair.read().unwrap();

        if let Some(coop_info) = coop {
            // Sign the announcement if keypair is available
            let signed_coop = if let Some(ref kp) = *keypair {
                coop_info.sign(kp)
            } else {
                warn!("No keypair set - sending unsigned announcement");
                coop_info
            };

            let message = FederationMessage::CoopAnnounce(signed_coop);
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
    pub fn send_vouch(
        &self,
        target_coop_id: &str,
        voucher_did: Did,
        trust_score: f64,
    ) -> Result<()> {
        let own_coop_id = self
            .own_coop_id()
            .ok_or_else(|| FederationError::NotInitialized("Own coop not set".to_string()))?;

        let keypair = self.keypair.read().unwrap();

        let vouch = Vouch::new(
            own_coop_id.clone(),
            voucher_did,
            target_coop_id.to_string(),
            trust_score,
        );

        // Sign the vouch if keypair is available
        let signed_vouch = if let Some(ref kp) = *keypair {
            vouch.sign(kp)
        } else {
            warn!("No keypair set - sending unsigned vouch");
            vouch
        };

        let message = FederationMessage::Vouch(signed_vouch);
        self.send_message(TOPIC_FEDERATION_REGISTRY, &message)?;
        metrics::registry::vouches_sent_inc(target_coop_id);

        info!(
            "Sent vouch for {} from {} with trust score {:.2}",
            target_coop_id, own_coop_id, trust_score
        );
        Ok(())
    }

    /// Request to federate with another cooperative
    pub fn request_federation(&self) -> Result<()> {
        let coop = self.own_coop.read().unwrap().clone();

        if let Some(coop_info) = coop {
            let message = FederationMessage::FederationRequest {
                requester: coop_info,
            };
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

        let keypair = self.keypair.read().unwrap();

        // Create signing bytes for the acceptance
        let signing_bytes = {
            let mut bytes = Vec::new();
            bytes.extend(b"federation_accept:");
            bytes.extend(own_coop_id.as_bytes());
            bytes.extend(b":");
            bytes.extend(requester_coop_id.as_bytes());
            bytes
        };

        // Sign the acceptance if keypair is available
        let signature = if let Some(ref kp) = *keypair {
            kp.sign(&signing_bytes).to_vec()
        } else {
            warn!("No keypair set - sending unsigned acceptance");
            Vec::new()
        };

        let message = FederationMessage::FederationAccept {
            accepter_coop_id: own_coop_id,
            requester_coop_id: requester_coop_id.to_string(),
            signature,
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

    fn test_keypair() -> KeyPair {
        KeyPair::generate().unwrap()
    }

    /// Create a test coop with an associated keypair for signing
    fn create_test_coop_with_keypair(id: &str) -> (CooperativeInfo, KeyPair) {
        let keypair = test_keypair();
        let coop = CooperativeInfo::new(
            id.to_string(),
            format!("{id} Cooperative"),
            keypair.did().clone(),
            FederationPolicy::Open,
        );
        (coop, keypair)
    }

    /// Create a test coop without keypair (for non-signing tests)
    fn create_test_coop(id: &str) -> CooperativeInfo {
        let (coop, _) = create_test_coop_with_keypair(id);
        coop
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

        // Set up our own coop with keypair for signing
        let (my_coop, keypair) = create_test_coop_with_keypair("my-coop");
        handler.set_own_coop(my_coop);
        handler.set_keypair(keypair);

        // Set up send callback
        let send_count = Arc::new(AtomicUsize::new(0));
        let count_clone = send_count.clone();
        handler.set_send_callback(Arc::new(move |_topic, _data| {
            count_clone.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }));

        // Announce (should be signed now)
        handler.announce().unwrap();
        assert_eq!(send_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_handler_receive_announcement() {
        let registry = create_test_registry();
        let handler = FederationGossipHandler::new(registry.clone());

        // Set up our own coop
        handler.set_own_coop(create_test_coop("my-coop"));

        // Create and sign announcement from another coop
        let (other_coop, other_keypair) = create_test_coop_with_keypair("other-coop");
        let signed_coop = other_coop.sign(&other_keypair);
        let message = FederationMessage::CoopAnnounce(signed_coop);
        let data = serde_json::to_vec(&message).unwrap();

        handler
            .handle_message("federation:registry", &data)
            .unwrap();

        // Should be registered (signature is valid)
        let registered = registry.get("other-coop").unwrap();
        assert!(registered.is_some());
    }

    #[test]
    fn test_handler_rejects_unsigned_announcement() {
        let registry = create_test_registry();
        let handler = FederationGossipHandler::new(registry.clone());

        // Set up our own coop
        handler.set_own_coop(create_test_coop("my-coop"));

        // Send unsigned announcement from another coop
        let other_coop = create_test_coop("unsigned-coop");
        let message = FederationMessage::CoopAnnounce(other_coop);
        let data = serde_json::to_vec(&message).unwrap();

        handler
            .handle_message("federation:registry", &data)
            .unwrap();

        // Should NOT be registered (signature is missing)
        let registered = registry.get("unsigned-coop").unwrap();
        assert!(registered.is_none());
    }

    #[test]
    fn test_handler_ignores_own_announcement() {
        let registry = create_test_registry();
        let handler = FederationGossipHandler::new(registry.clone());

        // Set up our own coop with keypair
        let (my_coop, my_keypair) = create_test_coop_with_keypair("my-coop");
        handler.set_own_coop(my_coop.clone());

        // Receive our own (signed) announcement - shouldn't register
        let signed_coop = my_coop.sign(&my_keypair);
        let message = FederationMessage::CoopAnnounce(signed_coop);
        let data = serde_json::to_vec(&message).unwrap();

        handler
            .handle_message("federation:registry", &data)
            .unwrap();

        // Should NOT be registered (it's our own)
        let registered = registry.get("my-coop").unwrap();
        assert!(registered.is_none());
    }

    #[test]
    fn test_coop_info_signature_roundtrip() {
        let (coop, keypair) = create_test_coop_with_keypair("test-coop");

        // Sign the coop info
        let signed_coop = coop.sign(&keypair);
        assert!(!signed_coop.signature.is_empty());

        // Verify signature
        assert!(signed_coop.verify_signature().is_ok());
    }

    #[test]
    fn test_vouch_signature_roundtrip() {
        let keypair = test_keypair();
        let vouch = Vouch::new(
            "voucher-coop".to_string(),
            keypair.did().clone(),
            "target-coop".to_string(),
            0.7,
        );

        // Sign the vouch
        let signed_vouch = vouch.sign(&keypair);
        assert!(!signed_vouch.signature.is_empty());

        // Verify signature
        assert!(signed_vouch.verify_signature().is_ok());
    }

    #[test]
    fn test_handler_federation_accept_requires_signature() {
        let registry = create_test_registry();
        let handler = FederationGossipHandler::new(registry.clone());

        // Set up our own coop as the requester
        let (my_coop, my_keypair) = create_test_coop_with_keypair("requester-coop");
        handler.set_own_coop(my_coop);
        handler.set_keypair(my_keypair);

        // Register an accepter coop first (so we can look up their DID)
        let (accepter_coop, accepter_keypair) = create_test_coop_with_keypair("accepter-coop");
        let signed_accepter = accepter_coop.clone().sign(&accepter_keypair);
        let announce_msg = FederationMessage::CoopAnnounce(signed_accepter);
        let announce_data = serde_json::to_vec(&announce_msg).unwrap();
        handler
            .handle_message("federation:registry", &announce_data)
            .unwrap();

        // Verify accepter is registered
        assert!(registry.get("accepter-coop").unwrap().is_some());

        // Now send an UNSIGNED federation accept - should be rejected
        let unsigned_accept = FederationMessage::FederationAccept {
            accepter_coop_id: "accepter-coop".to_string(),
            requester_coop_id: "requester-coop".to_string(),
            signature: vec![], // Empty signature
        };
        let unsigned_data = serde_json::to_vec(&unsigned_accept).unwrap();
        handler
            .handle_message("federation:registry", &unsigned_data)
            .unwrap();
        // The unsigned accept should be silently rejected (no error, but no action)

        // Now send a SIGNED federation accept - should be accepted
        let signing_bytes = {
            let mut bytes = Vec::new();
            bytes.extend(b"federation_accept:");
            bytes.extend(b"accepter-coop");
            bytes.extend(b":");
            bytes.extend(b"requester-coop");
            bytes
        };
        let signature = accepter_keypair.sign(&signing_bytes).to_vec();

        let signed_accept = FederationMessage::FederationAccept {
            accepter_coop_id: "accepter-coop".to_string(),
            requester_coop_id: "requester-coop".to_string(),
            signature,
        };
        let signed_data = serde_json::to_vec(&signed_accept).unwrap();

        // Should succeed without error
        handler
            .handle_message("federation:registry", &signed_data)
            .unwrap();
    }

    #[test]
    fn test_handler_rejects_forged_federation_accept() {
        let registry = create_test_registry();
        let handler = FederationGossipHandler::new(registry.clone());

        // Set up our own coop as the requester
        let (my_coop, my_keypair) = create_test_coop_with_keypair("requester-coop");
        handler.set_own_coop(my_coop);
        handler.set_keypair(my_keypair);

        // Register the real accepter coop
        let (accepter_coop, _accepter_keypair) = create_test_coop_with_keypair("accepter-coop");
        let (_, attacker_keypair) = create_test_coop_with_keypair("attacker-coop");

        // Register accepter so we can look up their DID
        let signed_accepter = accepter_coop.clone().sign(&_accepter_keypair);
        let announce_msg = FederationMessage::CoopAnnounce(signed_accepter);
        let announce_data = serde_json::to_vec(&announce_msg).unwrap();
        handler
            .handle_message("federation:registry", &announce_data)
            .unwrap();

        // Attacker tries to forge an accept message claiming to be from accepter-coop
        // but signs with their own key
        let signing_bytes = {
            let mut bytes = Vec::new();
            bytes.extend(b"federation_accept:");
            bytes.extend(b"accepter-coop");
            bytes.extend(b":");
            bytes.extend(b"requester-coop");
            bytes
        };
        let forged_signature = attacker_keypair.sign(&signing_bytes).to_vec();

        let forged_accept = FederationMessage::FederationAccept {
            accepter_coop_id: "accepter-coop".to_string(), // Claims to be accepter
            requester_coop_id: "requester-coop".to_string(),
            signature: forged_signature, // But signed by attacker
        };
        let forged_data = serde_json::to_vec(&forged_accept).unwrap();

        // Should be silently rejected (signature won't verify against accepter's DID)
        handler
            .handle_message("federation:registry", &forged_data)
            .unwrap();
    }
}
