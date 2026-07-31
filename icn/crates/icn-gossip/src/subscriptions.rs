//! Topic subscription management
//!
//! This module handles topic subscription lifecycle with trust-based authorization.
//! It enforces per-peer and per-topic limits to prevent resource exhaustion.
//!
//! # Authorization Layers
//!
//! 1. **Trust threshold** - Fine-grained trust score check (if configured)
//! 2. **Access control** - Coarse-grained ACL check based on trust class
//! 3. **Rate limits** - Per-peer subscription limits (trust-weighted)
//! 4. **Capacity limits** - Per-topic subscriber limits
//!
//! # Local vs network entry points (#2471)
//!
//! [`GossipActor::subscribe`] and [`GossipActor::unsubscribe`] are the **local** API:
//! they act on whatever DID the caller supplies and are how this node subscribes itself.
//!
//! [`GossipActor::subscribe_from_network`] and [`GossipActor::unsubscribe_from_network`]
//! are the **only** entry points a network message handler may use. They refuse any
//! request claiming this node's own DID, because `NetworkMessage.from` is self-declared
//! and no peer has a legitimate reason to alter our subscriptions.
//!
//! Neither pair authenticates anything. The DIDs here are claims, not proofs.
//!
//! # Key Functions
//!
//! - [`GossipActor::subscribe`] - Subscribe a DID to a topic (with authorization)
//! - [`GossipActor::unsubscribe`] - Remove a subscription
//! - [`GossipActor::subscribe_from_network`] - Network-originated subscribe (own-DID guarded)
//! - [`GossipActor::unsubscribe_from_network`] - Network-originated unsubscribe (own-DID guarded)
//! - [`GossipActor::get_subscribers`] - List subscribers for a topic
//! - [`GossipActor::get_subscriptions`] - List topics a DID is subscribed to
//! - [`GossipActor::is_subscribed`] - Check if a DID is subscribed to a topic
//! - [`GossipActor::is_locally_subscribed`] - Check this node's own, non-peer-mutable subscription

use crate::error::GossipError;
use crate::gossip::{spawn_violation_recording, GossipActor, MAX_SUBSCRIBERS_PER_TOPIC};
use crate::types::{AccessControl, ResourceLimits, Subscription};
use anyhow::{bail, Context as _, Result};
use icn_identity::Did;
use icn_kernel_api::authz::{
    ActionKind, Domain, PolicyContext, PolicyDecision, PolicyRequest, PolicyRequestCore,
};
use std::sync::Arc;
use tracing::{debug, info, instrument, warn};

impl GossipActor {
    /// Subscribe to a topic
    #[instrument(skip(self), fields(topic = %topic, subscriber = %subscriber))]
    pub async fn subscribe(&mut self, topic: &str, subscriber: Did) -> Result<Subscription> {
        let topic_obj = self.topics.get(topic).context("Topic not found")?;

        // 1. Get Constraints from Oracle
        let mut limits = ResourceLimits::default();

        if let Some(oracle) = &self.oracle {
            let mut context = PolicyContext::new()
                .with_resource(topic)
                .with_metadata("topic", topic);
            if let Some(min_threshold) = topic_obj.min_trust_threshold {
                context = context.with_metadata("min_trust_threshold", min_threshold.to_string());
            }
            if let AccessControl::MinTrustScore(min_score) = &topic_obj.acl {
                context = context.with_metadata("acl_min_trust_score", min_score.to_string());
            }
            let req = PolicyRequest::with_context(
                PolicyRequestCore::new(
                    subscriber.to_string(),
                    ActionKind::Subscribe,
                    Domain::trust(),
                ),
                context,
            );

            match oracle.evaluate(&req) {
                PolicyDecision::Allow { constraints, .. } => {
                    limits = ResourceLimits::from_constraints(&constraints);
                }
                PolicyDecision::Deny { reason, .. } => {
                    warn!(
                        "PolicyOracle denied subscription for {}: {}",
                        subscriber, reason
                    );

                    // Record misbehavior violation for policy denial
                    if let Some(ref detector) = self.misbehavior_detector {
                        use sha2::{Digest, Sha256};
                        let mut hasher = Sha256::new();
                        hasher.update(subscriber.as_str().as_bytes());
                        hasher.update(topic.as_bytes());
                        hasher.update(b"policy_denial");
                        let evidence = hasher.finalize().to_vec();

                        let violation = icn_security::Violation::ExcessiveResourceUse {
                            metric: format!("unauthorized_subscription:{topic}"),
                            observed: 1,
                            limit: 0,
                        };

                        spawn_violation_recording(
                            Arc::clone(detector),
                            subscriber.clone(),
                            violation,
                            evidence,
                        );
                    }

                    bail!("Subscription denied by policy: {reason}");
                }
            }
        }

        // Priority 1: Check AccessControl-based ACL (non-trust semantics)
        if !topic_obj.can_subscribe(&subscriber) {
            // Record misbehavior violation
            if let Some(ref detector) = self.misbehavior_detector {
                use sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                hasher.update(subscriber.as_str().as_bytes());
                hasher.update(topic.as_bytes());
                hasher.update(b"acl_violation");
                let evidence = hasher.finalize().to_vec();

                let violation = icn_security::Violation::ExcessiveResourceUse {
                    metric: format!("acl_violation:{topic}"),
                    observed: 1,
                    limit: 0,
                };

                spawn_violation_recording(
                    Arc::clone(detector),
                    subscriber.clone(),
                    violation,
                    evidence,
                );
            }

            bail!("Not authorized to subscribe to topic: {topic}");
        }

        // Check per-peer subscription limit (blindly enforced from policy constraints).
        // The local node's own DID is exempt: it must be able to subscribe to all
        // topics it creates regardless of its trust score at startup.
        let peer_topics = self.get_subscriptions(&subscriber);
        let peer_limit = limits.max_subscriptions;
        let is_own_did = subscriber == self.own_did;

        if !is_own_did && peer_topics.len() as u32 >= peer_limit {
            // Record misbehavior violation
            if let Some(ref detector) = self.misbehavior_detector {
                use sha2::{Digest, Sha256};
                let mut hasher = Sha256::new();
                hasher.update(subscriber.as_str().as_bytes());
                hasher.update(b"peer_subscription_limit");
                let evidence = hasher.finalize().to_vec();

                let violation = icn_security::Violation::ExcessiveResourceUse {
                    metric: "peer_subscriptions".to_string(),
                    observed: (peer_topics.len() + 1) as u64,
                    limit: peer_limit as u64,
                };

                spawn_violation_recording(
                    Arc::clone(detector),
                    subscriber.clone(),
                    violation,
                    evidence,
                );
            }

            warn!(
                "Per-peer subscription limit reached: {} has {} subscriptions (max {})",
                subscriber,
                peer_topics.len(),
                peer_limit
            );
            icn_obs::metrics::gossip::subscriptions_rejected_inc(topic, 0.0);
            bail!(
                "Peer subscription limit reached: {} subscriptions (max {})",
                peer_topics.len(),
                peer_limit
            );
        }

        // Add subscriber
        let subscribers = self.subscriptions.entry(topic.to_string()).or_default();

        if !subscribers.contains(&subscriber) {
            // Check per-topic subscriber limit to prevent unbounded growth.
            //
            // The local node's own DID is exempt, matching the per-peer limit above
            // (#2471). This list is fillable by unauthenticated peers: `Subscribe` accepts
            // any claimed DID, so a peer can push it to MAX_SUBSCRIBERS_PER_TOPIC with
            // distinct claimed DIDs. Without the exemption, that peer could make the
            // node's own later `subscribe` — during subsystem startup, or for a topic
            // created at runtime — fail, leaving `local_topic_subscriptions` unset and
            // silencing local delivery. That is the same remote suppression this change
            // exists to close, arriving through the capacity check instead of through
            // `Unsubscribe`. The exemption costs at most one entry over the cap, for our
            // own DID only.
            if !is_own_did && subscribers.len() >= MAX_SUBSCRIBERS_PER_TOPIC {
                // Record misbehavior violation (fire-and-forget, non-blocking)
                if let Some(ref detector) = self.misbehavior_detector {
                    use sha2::{Digest, Sha256};
                    let mut hasher = Sha256::new();
                    hasher.update(subscriber.as_str().as_bytes());
                    hasher.update(topic.as_bytes());
                    hasher.update(b"subscriber_limit");
                    let evidence = hasher.finalize().to_vec();

                    let violation = icn_security::Violation::ExcessiveResourceUse {
                        metric: format!("topic_subscribers:{topic}"),
                        observed: (subscribers.len() + 1) as u64,
                        limit: MAX_SUBSCRIBERS_PER_TOPIC as u64,
                    };

                    spawn_violation_recording(
                        Arc::clone(detector),
                        subscriber.clone(),
                        violation,
                        evidence,
                    );
                }

                bail!(
                    "Topic subscription limit reached: {} (max {})",
                    subscribers.len(),
                    MAX_SUBSCRIBERS_PER_TOPIC
                );
            }

            subscribers.push(subscriber.clone());
            info!(
                subscriber_did = %subscriber,
                topic = %topic,
                subscriber_count = subscribers.len(),
                "Subscribed to topic"
            );

            // Update metrics
            self.update_gauge_metrics();
        }

        // Record the locally-owned delivery gate (#2471). `is_own_did` was computed from
        // `self.own_did`, so this set can only ever be reached by a caller that passed our
        // own DID — and `subscribe_from_network` refuses exactly that from the wire.
        if is_own_did {
            self.local_topic_subscriptions.insert(topic.to_string());
        }

        Ok(Subscription {
            topic: topic.to_string(),
            subscriber,
        })
    }

    /// Unsubscribe from a topic
    pub fn unsubscribe(&mut self, topic: &str, subscriber: &Did) -> Result<()> {
        let is_own_did = *subscriber == self.own_did;

        let subscribers = self
            .subscriptions
            .get_mut(topic)
            .context("Topic not found")?;

        if let Some(pos) = subscribers.iter().position(|did| did == subscriber) {
            subscribers.remove(pos);
            info!(
                subscriber_did = %subscriber,
                topic = %topic,
                subscriber_count = subscribers.len(),
                "Unsubscribed from topic"
            );

            // Update metrics
            self.update_gauge_metrics();
        }

        if is_own_did {
            self.local_topic_subscriptions.remove(topic);
        }

        Ok(())
    }

    /// Handle a `Subscribe` request that arrived over the network.
    ///
    /// **This is the only entry point network message handlers may use.** The claimed
    /// subscriber comes from `NetworkMessage.from`, which is self-declared: TLS is TOFU
    /// with `client_auth_mandatory() = false`, and nothing rebinds a per-message `from`
    /// to the Hello identity. A syntactically valid DID is not proof of key possession.
    ///
    /// A peer subscribing **itself** remains supported — that is the intended protocol.
    /// What is refused is a request claiming *this node's own DID*, for which no peer has
    /// any legitimate reason. Enforced here rather than in the handler because the same
    /// handler shape is reimplemented in several places (`icn-core`'s supervisor,
    /// `icn-testkit`, integration harnesses); a per-caller guard would be one forgotten
    /// copy away from reopening the hole.
    ///
    /// This is containment, not authentication: it does not verify that the claimed peer
    /// DID belongs to the sender. Binding `NetworkMessage.from` to the authenticated
    /// identity of the delivering connection remains unresolved (#2480). The separate
    /// question of authenticating `GossipEntry.author` on the apply path is #2469.
    pub async fn subscribe_from_network(
        &mut self,
        topic: &str,
        claimed_subscriber: Did,
    ) -> Result<Subscription> {
        self.reject_own_did_claim(topic, &claimed_subscriber, "subscribe")?;
        self.subscribe(topic, claimed_subscriber).await
    }

    /// Handle an `Unsubscribe` request that arrived over the network.
    ///
    /// See [`GossipActor::subscribe_from_network`] for why the guard lives here.
    pub fn unsubscribe_from_network(
        &mut self,
        topic: &str,
        claimed_subscriber: &Did,
    ) -> Result<()> {
        self.reject_own_did_claim(topic, claimed_subscriber, "unsubscribe")?;
        self.unsubscribe(topic, claimed_subscriber)
    }

    /// Refuse a network-originated subscription-control request that claims this node's DID.
    ///
    /// Observability is a counter plus a `debug!` line: this is remotely triggerable and
    /// repeatable, so it must not be able to drive log volume. The counter is the durable
    /// signal. The returned error is the typed
    /// [`GossipError::SubscriptionControlSpoofRejected`] rather than an opaque one,
    /// specifically so callers can tell a spoof rejection apart from an operational
    /// subscribe/unsubscribe failure and avoid re-logging it at `warn!`.
    fn reject_own_did_claim(&self, topic: &str, claimed: &Did, action: &'static str) -> Result<()> {
        if *claimed != self.own_did {
            return Ok(());
        }

        icn_obs::metrics::gossip::subscription_control_spoof_rejected_inc();
        debug!(
            topic = %topic,
            action = %action,
            "Refused network subscription-control request claiming this node's own DID"
        );
        Err(GossipError::SubscriptionControlSpoofRejected {
            topic: topic.to_string(),
            action,
        }
        .into())
    }

    /// Whether `err` is the spoof rejection from [`GossipActor::subscribe_from_network`] or
    /// [`GossipActor::unsubscribe_from_network`].
    ///
    /// Network message handlers use this to keep a remotely-triggerable, expected rejection
    /// out of their operational `warn!` paths — otherwise a peer can batch many topics per
    /// forged request and drive warning-level log volume.
    pub fn is_subscription_control_spoof(err: &anyhow::Error) -> bool {
        matches!(
            err.downcast_ref::<GossipError>(),
            Some(GossipError::SubscriptionControlSpoofRejected { .. })
        )
    }

    /// Whether this node has subscribed **itself** to `topic`.
    ///
    /// This is the gate on local notification delivery. Unlike
    /// [`GossipActor::is_subscribed`], it cannot be influenced by any network message.
    pub fn is_locally_subscribed(&self, topic: &str) -> bool {
        self.local_topic_subscriptions.contains(topic)
    }

    /// Get all subscribers for a topic
    pub fn get_subscribers(&self, topic: &str) -> Vec<Did> {
        self.subscriptions.get(topic).cloned().unwrap_or_default()
    }

    /// Get all topics a DID is subscribed to
    pub fn get_subscriptions(&self, did: &Did) -> Vec<String> {
        self.subscriptions
            .iter()
            .filter_map(|(topic, subscribers)| {
                if subscribers.contains(did) {
                    Some(topic.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Check if a DID is subscribed to a topic
    pub fn is_subscribed(&self, topic: &str, did: &Did) -> bool {
        self.subscriptions
            .get(topic)
            .map(|subs| subs.contains(did))
            .unwrap_or(false)
    }
}
