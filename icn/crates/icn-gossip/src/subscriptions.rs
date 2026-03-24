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
//! # Key Functions
//!
//! - [`GossipActor::subscribe`] - Subscribe a DID to a topic (with authorization)
//! - [`GossipActor::unsubscribe`] - Remove a subscription
//! - [`GossipActor::get_subscribers`] - List subscribers for a topic
//! - [`GossipActor::get_subscriptions`] - List topics a DID is subscribed to
//! - [`GossipActor::is_subscribed`] - Check if a DID is subscribed to a topic

use crate::gossip::{spawn_violation_recording, GossipActor, MAX_SUBSCRIBERS_PER_TOPIC};
use crate::types::{AccessControl, ResourceLimits, Subscription};
use anyhow::{bail, Context as _, Result};
use icn_identity::Did;
use icn_kernel_api::authz::{
    ActionKind, Domain, PolicyContext, PolicyDecision, PolicyRequest, PolicyRequestCore,
};
use std::sync::Arc;
use tracing::{info, instrument, warn};

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
