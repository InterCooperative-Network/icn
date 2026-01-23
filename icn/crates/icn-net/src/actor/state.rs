//! State persistence for the network actor

use anyhow::{Context, Result};
use icn_identity::Did;
use tracing::info;

use crate::CapabilityFlags;
use super::PeerConnectionInfo;

use super::NetworkActor;

impl NetworkActor {
    /// Export network actor state for persistence
    ///
    /// This exports:
    /// - Peer X25519 public keys (for end-to-end encryption)
    /// - Known peer addresses (last known SocketAddr for reconnection)
    ///
    /// Note: Active connections are NOT persisted - they will be re-established
    /// via discovery and dialing after restart.
    pub async fn export_state(&self) -> icn_snapshot::NetworkState {
        // Export peer connection info (version, capabilities, X25519 keys, PQ keys)
        let peer_connections: std::collections::HashMap<String, icn_snapshot::PeerConnectionInfo> =
            self.peer_connections
                .read()
                .await
                .iter()
                .map(|(did, info)| {
                    let snapshot_info = icn_snapshot::PeerConnectionInfo {
                        did: did.to_string(),
                        negotiated_version: info.negotiated_version,
                        peer_capabilities: info.peer_capabilities.bits(),
                        peer_software: info.peer_software.clone(),
                        x25519_key: info.x25519_key,
                        ml_dsa_public: info.ml_dsa_public.clone(),
                        ml_kem_public: info.ml_kem_public.clone(),
                    };
                    (did.to_string(), snapshot_info)
                })
                .collect();

        // Legacy formats (empty for new deployments)
        let peer_x25519_keys: std::collections::HashMap<String, [u8; 32]> =
            std::collections::HashMap::new();
        let peer_addresses: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();

        icn_snapshot::NetworkState {
            peer_connections,
            peer_x25519_keys,
            peer_addresses,
        }
    }

    /// Restore network actor state from persistence
    ///
    /// This restores:
    /// - Peer connection info (version, capabilities, X25519 keys)
    /// - Known peer addresses (as a hint for reconnection)
    ///
    /// Supports both modern (peer_connections) and legacy (peer_x25519_keys) formats.
    /// Note: Connections are NOT automatically re-established - that happens
    /// through normal discovery and connection management processes.
    pub async fn restore_state(&self, state: icn_snapshot::NetworkState) -> Result<()> {
        info!(
            "Restoring network state: {} peer connections, {} legacy keys, {} peer addresses",
            state.peer_connections.len(),
            state.peer_x25519_keys.len(),
            state.peer_addresses.len()
        );

        // Restore peer connections
        let mut connections = self.peer_connections.write().await;

        // Restore modern format (peer_connections)
        for (did_str, snapshot_info) in state.peer_connections {
            let did =
                Did::from_str(&did_str).context("Failed to parse DID from peer connections")?;

            let connection_info = PeerConnectionInfo {
                did: did.clone(),
                negotiated_version: snapshot_info.negotiated_version,
                peer_capabilities: CapabilityFlags::from_bits_truncate(
                    snapshot_info.peer_capabilities,
                ),
                peer_software: snapshot_info.peer_software,
                x25519_key: snapshot_info.x25519_key,
                ml_dsa_public: snapshot_info.ml_dsa_public,
                ml_kem_public: snapshot_info.ml_kem_public,
            };

            connections.insert(did, connection_info);
        }

        // Legacy migration: restore old peer_x25519_keys format
        // (for backward compatibility with old snapshots)
        for (did_str, key) in state.peer_x25519_keys {
            let did = Did::from_str(&did_str)
                .context("Failed to parse DID from legacy peer X25519 keys")?;

            // Only restore if not already present from modern format
            connections
                .entry(did.clone())
                .or_insert_with(|| PeerConnectionInfo {
                    did,
                    negotiated_version: 1, // Assume v1 for legacy
                    peer_capabilities: CapabilityFlags::empty(),
                    peer_software: "legacy-unknown".to_string(),
                    x25519_key: key,
                    ml_dsa_public: None,
                    ml_kem_public: None,
                });
        }
        drop(connections);

        // Note: We don't restore peer addresses directly because Discovery manages
        // its own peer list. Peer addresses will be rediscovered via mDNS.
        // We could optionally pre-populate the discovery with these addresses,
        // but that adds complexity and they'll be rediscovered quickly anyway.

        info!("✅ Network state restored successfully");
        Ok(())
    }
}
