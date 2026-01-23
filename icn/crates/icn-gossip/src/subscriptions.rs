//! Topic subscription management

use crate::gossip::{spawn_violation_recording, topics_per_peer_limit, GossipActor, MAX_SUBSCRIBERS_PER_TOPIC};
use crate::types::Subscription;
use anyhow::{bail, Context as _, Result};
use icn_identity::Did;
use tracing::{info, warn};

impl GossipActor {
    /// Subscribe to a topic
    pub async fn subscribe(&mut self, topic: &str, subscriber: Did) -> Result<Subscription> {
        let topic_obj = self.topics.get(topic).context("Topic not found")?;

        // Priority 1: Check fine-grained trust threshold (if configured)
        if let Some(threshold) = topic_obj.min_trust_threshold {
            if let Some(trust_graph) = &self.trust_graph {
                // Get trust score from cache or compute it async
                let trust_score = if let Some(cached) = self.trust_cache.get(&subscriber) {
                    cached
                } else {
                    // Compute async and cache
                    let graph = trust_graph.read().await;
                    let score = graph.compute_trust_score(&subscriber).unwrap_or(0.0);
                    self.trust_cache.insert(&subscriber, score);
                    score
                };

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
                            std::sync::Arc::clone(detector),
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
            } else {
                warn!(
                    "Topic {} has min_trust_threshold {:.3} but GossipActor has no trust_graph - falling back to ACL check",
                    topic, threshold
                );
            }
        }

        // Priority 2: Check AccessControl-based ACL (coarse-grained)
        let trust_class = (self.trust_lookup)(&subscriber);
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
                    std::sync::Arc::clone(detector),
                    subscriber.clone(),
                    violation,
                    evidence,
                );
            }

            bail!("Not authorized to subscribe to topic: {topic}");
        }

        // Check per-peer subscription limit (trust-weighted)
        // This prevents a single peer from subscribing to too many topics
        let peer_topics = self.get_subscriptions(&subscriber);
        let peer_limit = topics_per_peer_limit(trust_class);
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
                    std::sync::Arc::clone(detector),
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
                        std::sync::Arc::clone(detector),
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
