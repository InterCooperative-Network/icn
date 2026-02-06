//! Replica coordination handlers: ReplicaRequest, ReplicaOffer, ReplicaStatus
//!
//! These handlers implement Phase 17 replica coordination protocol for
//! tracking which nodes have copies of content and their health status.

use crate::gossip::GossipActor;
use crate::types::{ContentHash, GossipMessage, ReplicaHealth};
use anyhow::Result;
use icn_identity::Did;
use tracing::{debug, info, warn};

impl GossipActor {
    /// Handle a ReplicaRequest message - request to find replicas for content
    ///
    /// Checks if we have the requested content. If so, responds with a
    /// ReplicaOffer indicating we can serve as a replica.
    pub(crate) fn handle_replica_request(
        &mut self,
        _sender: &Did,
        content_hash: ContentHash,
        requesting_peer: Did,
    ) -> Result<()> {
        debug!(
            peer_did = %requesting_peer,
            content_hash = %hex::encode(content_hash),
            message_type = "ReplicaRequest",
            "Received replica request"
        );

        // Check if we have this content hash in any topic
        let mut have_content = false;
        for entries in self.entries.values() {
            if entries.contains_key(&content_hash) {
                have_content = true;
                break;
            }
        }

        if have_content {
            debug!(
                content_hash = %hex::encode(content_hash),
                "We have this content, responding with ReplicaOffer"
            );

            // Respond with ReplicaOffer
            self.send_message(
                Some(requesting_peer.clone()),
                GossipMessage::ReplicaOffer {
                    content_hash,
                    offering_peer: self.own_did.clone(),
                    health: ReplicaHealth::Healthy,
                },
            );

            // Record ourselves as a replica in the store (if available)
            if let Some(store) = &self.store {
                if let Err(e) = store.add_replica(
                    &content_hash,
                    self.own_did.to_string(),
                    icn_store::ReplicaHealth::Healthy,
                ) {
                    warn!(
                        content_hash = %hex::encode(content_hash),
                        error = %e,
                        "Failed to record replica metadata"
                    );
                }
            }
        } else {
            debug!(
                content_hash = %hex::encode(content_hash),
                "We don't have this content, ignoring request"
            );
        }

        Ok(())
    }

    /// Handle a ReplicaOffer message - offer to serve as a replica
    ///
    /// Records the offering peer as a replica for the content hash in our store.
    pub(crate) fn handle_replica_offer(
        &mut self,
        _sender: &Did,
        content_hash: ContentHash,
        offering_peer: Did,
        health: ReplicaHealth,
    ) -> Result<()> {
        debug!(
            peer_did = %offering_peer,
            content_hash = %hex::encode(content_hash),
            health = ?health,
            message_type = "ReplicaOffer",
            "Received replica offer"
        );

        // Record replica metadata in store (if available)
        if let Some(store) = &self.store {
            // Convert gossip ReplicaHealth to store ReplicaHealth
            let store_health = match health {
                ReplicaHealth::Healthy => icn_store::ReplicaHealth::Healthy,
                ReplicaHealth::Stale => icn_store::ReplicaHealth::Stale,
                ReplicaHealth::Unreachable => icn_store::ReplicaHealth::Unreachable,
            };

            match store.add_replica(&content_hash, offering_peer.to_string(), store_health) {
                Ok(_) => {
                    debug!(
                        content_hash = %hex::encode(content_hash),
                        peer = %offering_peer,
                        "Recorded replica metadata"
                    );

                    // Get updated replica count for logging
                    if let Ok(count) = store.get_replica_count(&content_hash) {
                        info!(
                            content_hash = %hex::encode(content_hash),
                            replica_count = count,
                            "Replica count updated"
                        );
                    }
                }
                Err(e) => {
                    warn!(
                        content_hash = %hex::encode(content_hash),
                        peer = %offering_peer,
                        error = %e,
                        "Failed to record replica metadata"
                    );
                }
            }
        } else {
            debug!("No store configured, replica metadata not persisted");
        }

        Ok(())
    }

    /// Handle a ReplicaStatus message - batch update of replica statuses
    ///
    /// Updates replica metadata for multiple peers at once and checks
    /// if content is under-replicated.
    pub(crate) fn handle_replica_status(
        &mut self,
        _sender: &Did,
        content_hash: ContentHash,
        replicas: Vec<(Did, ReplicaHealth)>,
    ) -> Result<()> {
        debug!(
            content_hash = %hex::encode(content_hash),
            replica_count = replicas.len(),
            message_type = "ReplicaStatus",
            "Received replica status update"
        );

        // Batch update replica metadata in store (if available)
        if let Some(store) = &self.store {
            let mut updated_count = 0;
            let mut failed_count = 0;

            for (did, health) in replicas {
                // Convert gossip ReplicaHealth to store ReplicaHealth
                let store_health = match health {
                    ReplicaHealth::Healthy => icn_store::ReplicaHealth::Healthy,
                    ReplicaHealth::Stale => icn_store::ReplicaHealth::Stale,
                    ReplicaHealth::Unreachable => icn_store::ReplicaHealth::Unreachable,
                };

                match store.add_replica(&content_hash, did.to_string(), store_health) {
                    Ok(_) => updated_count += 1,
                    Err(e) => {
                        warn!(
                            content_hash = %hex::encode(content_hash),
                            peer = %did,
                            error = %e,
                            "Failed to update replica status"
                        );
                        failed_count += 1;
                    }
                }
            }

            info!(
                content_hash = %hex::encode(content_hash),
                updated = updated_count,
                failed = failed_count,
                "Batch updated replica status"
            );

            // Phase 17 Week 3: Check if replica count below threshold
            const DEFAULT_REPLICA_TARGET: usize = 3;
            if let Ok(Some(metadata)) = store.get_replica_metadata(&content_hash) {
                let healthy_count = metadata
                    .replicas
                    .iter()
                    .filter(|r| r.health == icn_store::ReplicaHealth::Healthy)
                    .count();

                if healthy_count < DEFAULT_REPLICA_TARGET {
                    icn_obs::metrics::replication::content_under_replicated_detected_inc();
                    warn!(
                        content_hash = %hex::encode(content_hash),
                        healthy_replicas = healthy_count,
                        target_replicas = DEFAULT_REPLICA_TARGET,
                        "Content under-replicated - ReplicationManager will request additional replicas"
                    );
                }
            }
        } else {
            debug!("No store configured, replica status not persisted");
        }

        Ok(())
    }
}
