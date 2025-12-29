//! Handshake protocol handlers - topology exchange and RTT measurement
//!
//! Handles:
//! - Handshake: Topology info exchange for neighbor set management
//! - HandshakeAck: Simple acknowledgement
//! - Ping/Pong: Round-trip time measurement

use super::ConnectionContext;
use crate::protocol::{write_message, NetworkMessage};
use crate::topology::{NeighborLimitsConfig, NodeRole, PeerId, TopologyInfo};
use icn_identity::Did;
use tracing::{info, warn};

impl ConnectionContext {
    /// Handle a Handshake message with topology information
    pub async fn handle_handshake(
        &self,
        connection: &quinn::Connection,
        from: &Did,
        region: &str,
        cluster_id: &str,
        role: &str,
    ) {
        info!(
            "Received handshake from {} (region={}, cluster={})",
            from, region, cluster_id
        );

        // Add peer to neighbor sets if topology is enabled
        if let Some(ref sets) = self.neighbor_sets {
            let peer_topology = TopologyInfo {
                region: region.to_string(),
                cluster_id: cluster_id.to_string(),
                role: match role {
                    "Edge" => NodeRole::Edge,
                    "Rendezvous" => NodeRole::Rendezvous,
                    "Archive" => NodeRole::Archive,
                    _ => NodeRole::Edge,
                },
            };

            let trust_score = if let Some(ref tg) = self.trust_graph {
                tg.read().await.compute_trust_score(from).unwrap_or(0.0) as f32
            } else {
                0.5
            };

            let limits = self
                .topology_config
                .as_ref()
                .map(|cfg| cfg.neighbor_limits.clone())
                .unwrap_or_else(|| NeighborLimitsConfig {
                    max_local_cluster: 50,
                    max_regional: 30,
                    max_backbone: 20,
                    max_trusted: 10,
                });

            sets.write().await.add_neighbor(
                PeerId(from.clone()),
                peer_topology,
                None,
                trust_score,
                &limits,
            );

            let sets_read = sets.read().await;
            icn_obs::metrics::topology::neighbors_by_set_update(
                sets_read.local_cluster.len(),
                sets_read.regional.len(),
                sets_read.backbone.len(),
                sets_read.trusted.len(),
            );
        }

        // Send handshake response
        self.send_handshake_response(connection, from).await;
    }

    /// Send handshake response
    async fn send_handshake_response(&self, connection: &quinn::Connection, to: &Did) {
        let connection_clone = connection.clone();
        let from_did = to.clone();
        let own_did_clone = self.own_did.clone();
        let topo_cfg_clone = self.topology_config.clone();

        tokio::spawn(async move {
            match connection_clone.open_bi().await {
                Ok((mut new_send, _new_recv)) => {
                    let response_msg = if let Some(ref topo_cfg) = topo_cfg_clone {
                        NetworkMessage::handshake(
                            own_did_clone,
                            from_did.clone(),
                            topo_cfg.region.clone(),
                            topo_cfg.cluster_id.clone(),
                            format!("{:?}", topo_cfg.role),
                        )
                    } else {
                        NetworkMessage::handshake_ack(own_did_clone, from_did.clone())
                    };

                    if let Err(e) = write_message(&mut new_send, &response_msg).await {
                        warn!("Failed to send handshake response to {}: {}", from_did, e);
                    } else if let Err(e) = new_send.finish() {
                        warn!(
                            "Failed to finish handshake response stream to {}: {}",
                            from_did, e
                        );
                    } else {
                        info!("Sent handshake response to {}", from_did);
                    }
                }
                Err(e) => {
                    warn!(
                        "Failed to open stream for handshake response to {}: {}",
                        from_did, e
                    );
                }
            }
        });
    }

    /// Handle HandshakeAck message
    pub fn handle_handshake_ack(&self, from: &Did) {
        info!("Received handshake ack from {}", from);
        // Nothing to do, just acknowledgement
    }

    /// Handle Ping message - respond with Pong
    pub async fn handle_ping(
        &self,
        connection: &quinn::Connection,
        message: NetworkMessage,
        from: &Did,
        sent_at: u64,
    ) {
        info!("Received Ping from {} (sent_at={}ms)", from, sent_at);

        // Send Pong response
        let pong_msg = NetworkMessage::pong(self.own_did.clone(), from.clone(), sent_at);

        let connection_clone = connection.clone();
        let from_did = from.clone();
        tokio::spawn(async move {
            match connection_clone.open_bi().await {
                Ok((mut new_send, _new_recv)) => {
                    if let Err(e) = write_message(&mut new_send, &pong_msg).await {
                        warn!("Failed to send Pong to {}: {}", from_did, e);
                    } else if let Err(e) = new_send.finish() {
                        warn!("Failed to finish Pong stream to {}: {}", from_did, e);
                    } else {
                        info!("Sent Pong to {}", from_did);
                    }
                }
                Err(e) => {
                    warn!("Failed to open stream for Pong to {}: {}", from_did, e);
                }
            }
        });

        // Forward Ping to external handler for observability
        self.forward_to_handler(message);
    }

    /// Handle Pong message - measure RTT
    pub async fn handle_pong(&self, from: &Did, ping_sent_at: u64, pong_sent_at: u64) {
        let now = icn_time::current_timestamp_millis();
        let rtt_ms = now.saturating_sub(ping_sent_at);

        info!(
            peer_did = %from,
            rtt_ms = rtt_ms,
            ping_sent_at = ping_sent_at,
            pong_sent_at = pong_sent_at,
            "Received Pong - RTT measured"
        );

        // Record RTT in neighbor sets if available
        if let Some(ref sets) = self.neighbor_sets {
            sets.write().await.record_rtt(&PeerId(from.clone()), rtt_ms);

            icn_obs::metrics::topology::rtt_observe(rtt_ms as f64);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::replay_guard::ReplayGuard;
    use crate::topology::{NeighborLimitsConfig, NeighborSets, TopologyConfig};
    use crate::{RateLimitConfig, RateLimiter, SessionManager};
    use icn_identity::{IdentityBundle, KeyPair};
    use std::collections::HashMap;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use tokio::sync::RwLock;

    fn create_test_context_with_topology() -> (ConnectionContext, Arc<RwLock<NeighborSets>>) {
        let keypair = KeyPair::generate().unwrap();
        let identity_bundle = IdentityBundle::from_keypair(keypair.clone()).unwrap();
        let own_did = keypair.did().clone();

        let forward_count = Arc::new(AtomicUsize::new(0));
        let forward_count_clone = Arc::clone(&forward_count);

        let handler: crate::IncomingMessageHandler = Arc::new(move |_msg| {
            forward_count_clone.fetch_add(1, Ordering::SeqCst);
        });

        let rate_limiter = Arc::new(RateLimiter::new(RateLimitConfig::default()));
        let replay_guard = Arc::new(RwLock::new(ReplayGuard::new(300, 3600)));
        let session_manager = Arc::new(RwLock::new(SessionManager::new()));
        let peer_connections = Arc::new(RwLock::new(HashMap::new()));

        let topology_config = TopologyConfig {
            region: "us-west".to_string(),
            cluster_id: "cluster-1".to_string(),
            role: NodeRole::Edge,
            neighbor_limits: NeighborLimitsConfig::default(),
            fanout: Default::default(),
        };

        let own_topology = TopologyInfo {
            region: "us-west".to_string(),
            cluster_id: "cluster-1".to_string(),
            role: NodeRole::Edge,
        };

        let neighbor_sets = Arc::new(RwLock::new(NeighborSets::new(own_topology)));

        let ctx = ConnectionContext {
            handler,
            rate_limiter,
            replay_guard,
            neighbor_sets: Some(neighbor_sets.clone()),
            topology_config: Some(topology_config),
            trust_graph: None,
            session_manager,
            peer_connections,
            blob_registry: None,
            misbehavior_detector: None,
            identity_bundle,
            own_did,
        };

        (ctx, neighbor_sets)
    }

    fn create_test_context_no_topology() -> ConnectionContext {
        let keypair = KeyPair::generate().unwrap();
        let identity_bundle = IdentityBundle::from_keypair(keypair.clone()).unwrap();
        let own_did = keypair.did().clone();

        let handler: crate::IncomingMessageHandler = Arc::new(move |_msg| {});

        let rate_limiter = Arc::new(RateLimiter::new(RateLimitConfig::default()));
        let replay_guard = Arc::new(RwLock::new(ReplayGuard::new(300, 3600)));
        let session_manager = Arc::new(RwLock::new(SessionManager::new()));
        let peer_connections = Arc::new(RwLock::new(HashMap::new()));

        ConnectionContext {
            handler,
            rate_limiter,
            replay_guard,
            neighbor_sets: None,
            topology_config: None,
            trust_graph: None,
            session_manager,
            peer_connections,
            blob_registry: None,
            misbehavior_detector: None,
            identity_bundle,
            own_did,
        }
    }

    #[test]
    fn test_handle_handshake_ack() {
        // Simple test - just ensures it doesn't panic
        let ctx = create_test_context_no_topology();
        let peer = KeyPair::generate().unwrap();
        ctx.handle_handshake_ack(&peer.did());
        // If we get here without panic, test passes
    }

    #[tokio::test]
    async fn test_handle_pong_records_rtt() {
        let (ctx, neighbor_sets) = create_test_context_with_topology();
        let peer = KeyPair::generate().unwrap();
        let peer_did = peer.did().clone();

        // Add the peer to neighbor sets first
        {
            let mut sets = neighbor_sets.write().await;
            sets.add_neighbor(
                PeerId(peer_did.clone()),
                TopologyInfo {
                    region: "us-west".to_string(),
                    cluster_id: "cluster-1".to_string(),
                    role: NodeRole::Edge,
                },
                None,
                0.5,
                &NeighborLimitsConfig::default(),
            );
        }

        // Simulate a ping sent 50ms ago
        let ping_sent_at = icn_time::current_timestamp_millis() - 50;
        let pong_sent_at = ping_sent_at + 25; // Pong sent 25ms after ping received

        ctx.handle_pong(&peer_did, ping_sent_at, pong_sent_at).await;

        // Verify RTT was recorded (should be approximately 50ms)
        let sets = neighbor_sets.read().await;
        let rtt = sets.get_rtt(&PeerId(peer_did.clone()));
        assert!(rtt.is_some());
        // RTT should be close to 50ms (with some timing variance)
        let rtt = rtt.unwrap();
        assert!(
            rtt >= 40 && rtt <= 100,
            "RTT {} was not in expected range",
            rtt
        );
    }

    #[tokio::test]
    async fn test_handle_pong_without_topology() {
        // Should not panic even without topology enabled
        let ctx = create_test_context_no_topology();
        let peer = KeyPair::generate().unwrap();

        let ping_sent_at = icn_time::current_timestamp_millis() - 50;
        let pong_sent_at = ping_sent_at + 25;

        ctx.handle_pong(&peer.did(), ping_sent_at, pong_sent_at)
            .await;
        // Test passes if no panic
    }

    #[tokio::test]
    async fn test_handle_pong_unknown_peer() {
        let (ctx, neighbor_sets) = create_test_context_with_topology();
        let peer = KeyPair::generate().unwrap();

        // Don't add peer to neighbor sets - should still not panic
        let ping_sent_at = icn_time::current_timestamp_millis() - 50;
        let pong_sent_at = ping_sent_at + 25;

        ctx.handle_pong(&peer.did(), ping_sent_at, pong_sent_at)
            .await;

        // RTT should not be recorded for unknown peer
        let sets = neighbor_sets.read().await;
        let rtt = sets.get_rtt(&PeerId(peer.did().clone()));
        assert!(rtt.is_none());
    }
}
