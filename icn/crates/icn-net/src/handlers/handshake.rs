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
