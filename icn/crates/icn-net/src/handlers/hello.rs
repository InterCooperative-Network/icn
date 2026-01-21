//! Hello message handler - DID-TLS binding verification and version negotiation
//!
//! The Hello message is the first message exchanged between peers and establishes:
//! - DID-TLS binding verification (TOFU model)
//! - Protocol version negotiation
//! - Capability exchange
//! - X25519 key exchange for E2E encryption
//! - Post-quantum key binding verification (DID-PQ binding)

use super::ConnectionContext;
use crate::actor::PeerConnectionInfo;
use crate::protocol::{NetworkMessage, PqBindingProof};
use crate::topology::{NeighborLimitsConfig, PeerId, TopologyInfo};
use anyhow::{Context, Result};
use icn_identity::Did;
use std::collections::hash_map::Entry;
use tracing::{debug, info, warn};

impl ConnectionContext {
    /// Handle an incoming Hello message
    ///
    /// Performs:
    /// 1. DID-TLS binding verification
    /// 2. Protocol version negotiation
    /// 3. Capability exchange
    /// 4. X25519 key storage for E2E encryption
    /// 5. PQ public key storage for hybrid crypto (if present)
    /// 6. DID-PQ binding verification (if proof present)
    /// 7. Connection storage in session manager
    /// 8. Neighbor set updates (if topology enabled)
    /// 9. Hello response with our info
    pub async fn handle_hello(
        &self,
        connection: &quinn::Connection,
        from: &Did,
        binding_info: &icn_identity::BindingInfo,
        version_info: &Option<crate::VersionInfo>,
        topology_info: &Option<TopologyInfo>,
        x25519_public: &[u8; 32],
        ml_dsa_public: Option<Vec<u8>>,
        ml_kem_public: Option<Vec<u8>>,
        pq_binding_proof: Option<PqBindingProof>,
    ) -> Result<()> {
        // Verify DID-TLS binding using TOFU model
        if let Err(e) = icn_identity::verify_did_matches_binding(from, binding_info) {
            warn!(
                peer_did = %from,
                "DID-TLS binding verification failed: {e}"
            );
            return Err(anyhow::anyhow!("DID-TLS binding verification failed: {e}"));
        }

        debug!(
            peer_did = %from,
            "DID-TLS binding verified successfully"
        );

        // Verify DID-PQ binding if proof is present
        // Returns: Ok(true) = verified, Ok(false) = no PQ key or legacy (no proof), Err = invalid proof
        let pq_binding_result =
            Self::verify_pq_binding(from, ml_dsa_public.as_deref(), pq_binding_proof.as_ref());

        // If binding verification explicitly failed (invalid proof), reject the connection
        // This is a fail-closed security approach for potential attacks
        if let Err(e) = &pq_binding_result {
            warn!(
                peer_did = %from,
                error = %e,
                "Rejecting connection: DID-PQ binding verification failed"
            );
            return Err(anyhow::anyhow!("DID-PQ binding verification failed: {e}"));
        }

        let pq_binding_verified = pq_binding_result.unwrap_or(false);

        // Perform version negotiation
        let local_version_info =
            crate::VersionInfo::new(format!("icnd-{}", env!("CARGO_PKG_VERSION")));

        let (negotiated_version, common_caps, peer_software) = match version_info {
            Some(remote_info) => {
                // Modern node with version info
                let negotiated = match crate::negotiate_version(&local_version_info, remote_info) {
                    Ok(v) => v,
                    Err(e) => {
                        warn!(
                            peer_did = %from,
                            local_range = format!("[{}-{}]", local_version_info.min_supported, local_version_info.max_supported),
                            peer_range = format!("[{}-{}]", remote_info.min_supported, remote_info.max_supported),
                            "Version negotiation failed: {}",
                            e
                        );
                        icn_obs::metrics::network::version_negotiation_failure_inc(
                            "incompatible_version",
                        );
                        return Err(anyhow::anyhow!("Incompatible protocol version"));
                    }
                };

                icn_obs::metrics::network::version_negotiation_success_inc(negotiated);

                let caps = crate::common_capabilities(&local_version_info, remote_info);
                (negotiated, caps, remote_info.software_version.clone())
            }
            None => {
                // Legacy node without version info
                info!(
                    peer_did = %from,
                    "Received Hello from legacy node (no version_info), treating as protocol v1"
                );
                icn_obs::metrics::network::version_negotiation_success_inc(1);
                (
                    1,
                    crate::CapabilityFlags::empty(),
                    "legacy-node".to_string(),
                )
            }
        };

        info!(
            peer_did = %from,
            peer_software = %peer_software,
            negotiated_version = negotiated_version,
            common_capabilities = ?common_caps.describe(),
            has_topology = topology_info.is_some(),
            message_type = "Hello",
            "Received Hello with version negotiation"
        );

        // Store peer connection info
        // Validate and filter PQ keys based on negotiated capabilities
        let validated_ml_dsa = Self::validate_pq_key_for_capability(
            from,
            ml_dsa_public,
            common_caps.contains(crate::CapabilityFlags::HYBRID_SIGNATURES),
            "ML-DSA",
            "HYBRID_SIGNATURES",
        );
        let validated_ml_kem = Self::validate_pq_key_for_capability(
            from,
            ml_kem_public,
            common_caps.contains(crate::CapabilityFlags::HYBRID_KEM),
            "ML-KEM",
            "HYBRID_KEM",
        );

        // Validate ML-DSA key format if present (fail-fast)
        #[cfg(feature = "post-quantum")]
        let validated_ml_dsa = if let Some(ref key_bytes) = validated_ml_dsa {
            match icn_crypto_pq::MlDsaPublicKey::from_bytes(key_bytes) {
                Ok(_) => validated_ml_dsa,
                Err(e) => {
                    warn!(
                        peer_did = %from,
                        "Invalid ML-DSA public key format: {e}, discarding"
                    );
                    None
                }
            }
        } else {
            validated_ml_dsa
        };

        // Discard ML-DSA keys if binding wasn't verified (legacy nodes without proofs)
        // This ensures we don't use unverified PQ keys for hybrid verification
        let validated_ml_dsa = if !pq_binding_verified && validated_ml_dsa.is_some() {
            debug!(
                peer_did = %from,
                "Discarding ML-DSA key: binding not verified (legacy node without proof)"
            );
            None
        } else {
            validated_ml_dsa
        };

        {
            let has_pq_keys = validated_ml_dsa.is_some() || validated_ml_kem.is_some();
            let connection_info = PeerConnectionInfo {
                did: from.clone(),
                negotiated_version,
                peer_capabilities: common_caps,
                peer_software: peer_software.clone(),
                x25519_key: *x25519_public,
                ml_dsa_public: validated_ml_dsa,
                ml_kem_public: validated_ml_kem,
            };

            let mut connections = self.peer_connections.write().await;
            connections.insert(from.clone(), connection_info);
            info!(
                peer_did = %from,
                negotiated_version = negotiated_version,
                peer_software = %peer_software,
                capabilities = ?common_caps.describe(),
                has_pq_keys = has_pq_keys,
                pq_binding_verified = pq_binding_verified,
                "Stored peer connection info"
            );
        }

        // Store the incoming QUIC connection in session_manager
        {
            let connections_arc = self.session_manager.read().await.connections_arc();
            let peer_did = from.to_string();
            let mut connections = connections_arc.write().await;
            match connections.entry(peer_did) {
                Entry::Occupied(entry) => {
                    info!(
                        "Connection already exists for {}, not overwriting with incoming connection from {}",
                        entry.key(),
                        connection.remote_address()
                    );
                }
                Entry::Vacant(entry) => {
                    info!(
                        "Storing incoming connection from {} at {}",
                        entry.key(),
                        connection.remote_address()
                    );
                    entry.insert(connection.clone());
                }
            }
        }

        // Add peer to neighbor sets if topology is enabled
        if let Some(ref sets) = self.neighbor_sets {
            if let Some(peer_topology) = topology_info {
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
                    peer_topology.clone(),
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
        }

        // Send Hello response
        self.send_hello_response(connection, from).await?;

        info!("Processed Hello from {}", from);
        Ok(())
    }

    /// Send Hello response with our identity info
    async fn send_hello_response(&self, connection: &quinn::Connection, to: &Did) -> Result<()> {
        let binding_info = self.identity_bundle.binding_info();
        let x25519_public = *self.identity_bundle.x25519_public_bytes();
        let version_info = crate::VersionInfo::new(format!("icnd-{}", env!("CARGO_PKG_VERSION")));
        let topology_info = self.topology_config.as_ref().map(|topo_cfg| TopologyInfo {
            region: topo_cfg.region.clone(),
            cluster_id: topo_cfg.cluster_id.clone(),
            role: topo_cfg.role,
        });

        // Build Hello response with PQ binding proof if available
        #[cfg(feature = "post-quantum")]
        let hello_response = {
            let keypair = self
                .identity_bundle
                .keypair()
                .context("Failed to load keypair for PQ binding")?;
            let ml_dsa = keypair.pq_public_key().map(|pk| pk.as_bytes().to_vec());
            let ml_kem = self
                .identity_bundle
                .kem_pq_public_bytes()
                .map(|b| b.to_vec());

            // Use hello_with_binding to include DID-PQ binding proof
            NetworkMessage::hello_with_binding(
                self.own_did.clone(),
                to.clone(),
                binding_info,
                version_info,
                topology_info,
                x25519_public,
                ml_dsa,
                ml_kem,
                &keypair,
            )
        };

        #[cfg(not(feature = "post-quantum"))]
        let hello_response = NetworkMessage::hello(
            self.own_did.clone(),
            to.clone(),
            binding_info,
            version_info,
            topology_info,
            x25519_public,
            None,
            None,
        );

        let connection_clone = connection.clone();
        #[cfg(feature = "post-quantum")]
        let log_msg = "Sent Hello response with X25519 public key and PQ binding";
        #[cfg(not(feature = "post-quantum"))]
        let log_msg = "Sent Hello response with X25519 public key";

        tokio::spawn(async move {
            match connection_clone.open_bi().await {
                Ok((mut send, _recv)) => {
                    if let Err(e) = crate::protocol::write_message(&mut send, &hello_response).await
                    {
                        warn!("Failed to write Hello response: {}", e);
                    } else {
                        info!("{}", log_msg);
                    }
                }
                Err(e) => {
                    warn!("Failed to open stream for Hello response: {}", e);
                }
            }
        });

        Ok(())
    }

    /// Validate PQ key against negotiated capability
    ///
    /// Returns the key only if:
    /// - The key is present AND the capability was negotiated, OR
    /// - The key is not present
    ///
    /// Logs a warning if a key was sent without the corresponding capability.
    fn validate_pq_key_for_capability(
        peer_did: &Did,
        key: Option<Vec<u8>>,
        capability_negotiated: bool,
        key_name: &str,
        capability_name: &str,
    ) -> Option<Vec<u8>> {
        match (key, capability_negotiated) {
            (Some(k), true) => Some(k),
            (Some(_), false) => {
                warn!(
                    peer_did = %peer_did,
                    key_type = key_name,
                    capability = capability_name,
                    "Peer sent {} key without {} capability, discarding",
                    key_name,
                    capability_name
                );
                None
            }
            (None, _) => None,
        }
    }

    /// Verify DID-PQ key binding proof
    ///
    /// Returns:
    /// - `Ok(true)` if ML-DSA key is present with valid binding proof
    /// - `Ok(false)` if no ML-DSA key is present, or legacy node (no proof but no attack)
    /// - `Err(e)` if ML-DSA key is present with INVALID binding proof (potential attack)
    ///
    /// Security policy:
    /// - Invalid proofs cause connection rejection (fail-closed)
    /// - Missing proofs from legacy nodes are accepted but ML-DSA keys are discarded
    fn verify_pq_binding(
        peer_did: &Did,
        ml_dsa_public: Option<&[u8]>,
        binding_proof: Option<&PqBindingProof>,
    ) -> Result<bool, anyhow::Error> {
        match (ml_dsa_public, binding_proof) {
            // No PQ key - nothing to verify
            (None, _) => {
                debug!(
                    peer_did = %peer_did,
                    "No ML-DSA key present, skipping DID-PQ binding verification"
                );
                Ok(false)
            }

            // PQ key present with binding proof - verify it
            (Some(ml_dsa_bytes), Some(proof)) => match proof.verify(peer_did, ml_dsa_bytes) {
                Ok(()) => {
                    info!(
                        peer_did = %peer_did,
                        "DID-PQ binding verification successful"
                    );
                    Ok(true)
                }
                Err(e) => {
                    // Invalid proof is a potential attack - return error to reject connection
                    Err(anyhow::anyhow!(
                        "Invalid DID-PQ binding proof from {peer_did}: {e}"
                    ))
                }
            },

            // PQ key present but no binding proof - legacy node
            // Accept connection but return Ok(false) to discard ML-DSA keys
            (Some(_), None) => {
                warn!(
                    peer_did = %peer_did,
                    "ML-DSA key present without binding proof (legacy node); discarding PQ key"
                );
                Ok(false)
            }
        }
    }
}
