//! Topic subscription management
//!
//! This module handles topic subscription lifecycle with policy-based authorization.
//! It enforces per-peer and per-topic limits to prevent resource exhaustion.
//!
//! # Authorization Layers
//!
//! 1. **Trust threshold** - Fine-grained trust score check (if configured via PolicyOracle)
//! 2. **Access control** - Coarse-grained ACL check (temporary TrustClass bridge)
//! 3. **Rate limits** - Per-peer subscription limits (oracle-determined)
//! 4. **Capacity limits** - Per-topic subscriber limits
//!
//! # Key Functions
//!
//! - [`GossipActor::subscribe`] - Subscribe a DID to a topic (with authorization)
//! - [`GossipActor::unsubscribe`] - Remove a subscription
//! - [`GossipActor::get_subscribers`] - List subscribers for a topic
//! - [`GossipActor::get_subscriptions`] - List topics a DID is subscribed to
//! - [`GossipActor::is_subscribed`] - Check if a DID is subscribed to a topic

use crate::gossip::{
    spawn_violation_recording, topics_per_peer_limit, GossipActor, MAX_SUBSCRIBERS_PER_TOPIC,
};
use crate::types::Subscription;
use anyhow::{bail, Context as _, Result};
use icn_identity::Did;
use icn_kernel_api::authz::{ActionKind, Domain, PolicyRequest};
use std::sync::Arc;
use tracing::{info, instrument, warn};

impl GossipActor {
    /// Subscribe to a topic
    #[instrument(skip(self), fields(topic = %topic, subscriber = %subscriber))]
    pub async fn subscribe(&mut self, topic: &str, subscriber: Did) -> Result<Subscription> {
        let topic_obj = self.topics.get(topic).context("Topic not found")?;

        // Use policy oracle for trust decision
        let request = PolicyRequest::new(
            subscriber.as_str().to_string(),
            ActionKind::Subscribe,
            Domain::trust(),
        );
        let decision = self.policy_oracle.evaluate(&request);

        // Extract trust score from oracle decision (if credit_multiplier is set)
        // The trust app sets credit_multiplier = trust_score
        let trust_score = decision
            .constraints()
            .and_then(|c| c.credit_multiplier)
            .unwrap_or(0.0);

        // Priority 1: Check fine-grained trust threshold (if configured)
        if let Some(threshold) = topic_obj.min_trust_threshold {
            // Enforce trust threshold
            if trust_score < threshold {
                warn!(
                    "🔒 Subscription rejected: DID {} to topic {} (trust score: {:.3} < threshold: {:.3})",
                    subscriber, topic, trust_score, threshold
                );

                // Track rejection metrics
                icn_obs::metrics::gossip::subscriptions_rejected_inc(topic, trust_score);

                // Record misbehavior violation (fire-and-forget, non-blocking)
                if let Some(ref detector) = self.misbehavior_detector {
                    use sha2::{Digest, Sha256};
                    let mut hasher = Sha256::new();
                    hasher.update(subscriber.as_str().as_bytes());
                    hasher.update(topic.as_bytes());
                    hasher.update(b"unauthorized_subscription");
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

                bail!("Insufficient trust: score {trust_score:.3} < required {threshold:.3}");
            }

            info!(
                "✅ Subscription authorized: DID {} to topic {} (trust score: {:.3})",
                subscriber, topic, trust_score
            );
        }

        // Priority 2: Check AccessControl-based ACL (coarse-grained)
        // Map oracle constraints back to TrustClass (temporary bridge until AccessControl is refactored)
        let trust_class = if let Some(constraints) = decision.constraints() {
            if let Some(max_topics) = constraints.max_topics {
                // Map max_topics back to TrustClass (temporary bridge)
                if max_topics >= 500 {
                    Some(icn_trust::TrustClass::Federated)
                } else if max_topics >= 100 {
                    Some(icn_trust::TrustClass::Partner)
                } else if max_topics >= 25 {
                    Some(icn_trust::TrustClass::Known)
                } else {
                    Some(icn_trust::TrustClass::Isolated)
                }
            } else {
                None
            }
        } else {
            None
        };

        if !topic_obj.can_subscribe(&subscriber, trust_class) {
            // Record misbehavior violation (fire-and-forget, non-blocking)
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

        // Check per-peer subscription limit (oracle-determined)
        // This prevents a single peer from subscribing to too many topics
        let peer_topics = self.get_subscriptions(&subscriber);
        let peer_limit = topics_per_peer_limit(&self.policy_oracle, &subscriber);
        if peer_topics.len() >= peer_limit {
            // Record misbehavior violation (fire-and-forget, non-blocking)
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
            // Check per-topic subscriber limit to prevent unbounded growth
            if subscribers.len() >= MAX_SUBSCRIBERS_PER_TOPIC {
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

        Ok(Subscription {
            topic: topic.to_string(),
            subscriber,
        })
    }

    /// Unsubscribe from a topic
    pub fn unsubscribe(&mut self, topic: &str, subscriber: &Did) -> Result<()> {
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

        Ok(())
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
