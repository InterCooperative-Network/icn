//! Hello message handler - DID-TLS binding verification and version negotiation
//!
//! The Hello message is the first message exchanged between peers and establishes:
//! - DID-TLS binding verification (TOFU model)
//! - Protocol version negotiation
//! - Capability exchange
//! - X25519 key exchange for E2E encryption
//! - Post-quantum key binding verification (DID-PQ binding)

use super::{ConnectionContext, ConnectionDirection};
use crate::actor::PeerConnectionInfo;
use crate::protocol::{NetworkMessage, PqBindingProof};
use crate::topology::{NeighborLimitsConfig, PeerId, TopologyInfo};
#[cfg(feature = "post-quantum")]
use anyhow::Context;
use anyhow::Result;
use icn_identity::Did;
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
        // Bind the claimed DID to *this* connection, before anything derived from `from`
        // is stored. Three facts together are what make `from` authenticated here, and
        // all three are required:
        //
        //   1. the binding names the DID that sent this Hello  (`did == from`)
        //   2. that DID's key signed the binding               (Ed25519 over the hash)
        //   3. the hash is of the certificate this connection is actually using
        //
        // (1) and (2) alone only prove the DID authenticated *some* certificate at some
        // point. Every node publishes its own BindingInfo in every Hello, so that pair is
        // replayable by anyone who has ever exchanged a Hello with the peer. (3) is what
        // ties the claim to the live TLS session and makes replay useless.
        //
        // Deliberately no misbehavior scoring on any failure below: the party on this
        // connection is unauthenticated, so `from` is a name it chose. Recording a
        // violation against `from` would let an attacker degrade the reputation of any
        // peer it wants to name. The connection is refused; the claimed DID is untouched.
        let Some(peer_cert) = crate::tls::current_peer_certificate(connection) else {
            warn!(
                claimed_did = %from,
                remote_addr = %connection.remote_address(),
                "Rejecting Hello: connection presented no peer certificate, so the claimed \
                 DID cannot be bound to this session"
            );
            icn_obs::metrics::network::hello_binding_rejected_inc("no_peer_certificate");
            return Err(anyhow::anyhow!(
                "DID-TLS binding verification failed: no peer certificate on this connection"
            ));
        };

        // (1) + (2): the binding is for `from`, and `from`'s key signed it.
        if let Err(e) = icn_identity::verify_did_matches_binding(from, binding_info) {
            warn!(
                claimed_did = %from,
                "DID-TLS binding verification failed: {e}"
            );
            icn_obs::metrics::network::hello_binding_rejected_inc("did_or_signature_mismatch");
            return Err(anyhow::anyhow!("DID-TLS binding verification failed: {e}"));
        }

        // (3): the signed hash is of the certificate presented by THIS connection.
        if let Err(e) = icn_identity::verify_binding_info(binding_info, &peer_cert) {
            warn!(
                claimed_did = %from,
                remote_addr = %connection.remote_address(),
                "Rejecting Hello: BindingInfo is bound to a different certificate than this \
                 connection presents (replayed binding): {e}"
            );
            icn_obs::metrics::network::hello_binding_rejected_inc("current_cert_mismatch");
            return Err(anyhow::anyhow!(
                "DID-TLS binding verification failed: binding does not match current peer certificate"
            ));
        }

        debug!(
            peer_did = %from,
            "DID-TLS binding verified against current connection certificate"
        );

        // This is the moment `from` stops being a claim and becomes an identity: all three
        // DID-TLS facts hold, against the certificate *this* connection is presenting. Bind
        // it to the connection so handlers that disclose something about this node can ask
        // who they are talking to without trusting a sender-chosen field (#2535, #2491).
        self.record_authenticated_peer(from).await;

        // Now — and only now — is there an identity to weigh the operator's expectation
        // against. Before the three checks above, `from` is a name the sender chose; after
        // them it is a DID proven against this connection's certificate. A configured DID is
        // still not a pin, so this records a divergence and changes nothing else (#2533).
        self.resolve_peer_expectation(from, connection.remote_address());

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

        // Taken before the row below consumes it. This is the *only* value in this function
        // that has passed every check making it this principal's key — see the claim
        // installation further down for what those are.
        let current_pq_key: Option<std::sync::Arc<[u8]>> =
            validated_ml_dsa.as_deref().map(std::sync::Arc::from);

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

        // Record what *this* connection proves about its peer's sequence regime, for as long
        // as this connection lasts (#2644).
        //
        // The row written above cannot carry this. Nothing removes it when the connection ends,
        // and `restore_state` recreates it from a snapshot before any Hello, so it says what a
        // peer once proved rather than what it is proving now — and a peer that had rolled back
        // to an unproven regime kept a durable one it no longer had. The lease below expires
        // with the connection, because `ConnectionContext` is owned by the connection handler's
        // own stack frame.
        //
        // Keyed by the decoded signing key, not by the spelling of `from`: `handle_signed` asks
        // the same question of the same equivalence class the signature check and `ReplayGuard`
        // use, so no spelling of one key can select a different answer (#2640).
        //
        // Assignment order matters. The new claim is taken *before* the old one is dropped, so a
        // repeated Hello re-stating the same capability never leaves a window in which a
        // concurrent message reads the peer as unproven. Assigning to the slot is what drops the
        // previous claim, so a peer that stops advertising the capability on this connection
        // stops proving it here too.
        {
            let claim = common_caps
                .contains(crate::CapabilityFlags::DURABLE_SIGNING_SEQUENCE)
                .then(|| crate::replay_guard::SenderPrincipal::from_did(from).ok())
                .flatten()
                .map(|principal| self.capability_registry.claim_durable(principal));
            self.replace_durable_claim(claim);
        }

        // And what this connection proves about its peer's ML-DSA key, on the same lease
        // (#2646).
        //
        // The row written above cannot carry this either, for the same two reasons and with a
        // worse consequence. It is keyed by the *spelling* of `from`, and `Did` equality is
        // string equality, so a captured hybrid envelope re-spelled into any of the 22 other
        // accepted multibase bases missed the lookup entirely and was verified classically — a
        // PQ downgrade selected by the sender field's textual representation rather than by
        // policy, costing no key material because `SignedEnvelope`'s signed bytes do not
        // include `from`. And
        // nothing removes the row when the connection ends, so a key from a previous
        // incarnation — including one `restore_state` wrote from a snapshot before any Hello —
        // was read as though the peer were still using it.
        //
        // `current_pq_key` is `Some` only where all of the following already hold, which is
        // what makes this claim safe to key by the principal rather than by the connection:
        //
        //   1. the three #2520 DID-TLS facts, or this function returned above;
        //   2. `verify_pq_binding` accepted a `PqBindingProof` — an ML-DSA signature over
        //      `DID-PQ-BINDING-V1:<did>:<ts>` — so the advertised key's *own* holder named this
        //      DID, and an invalid proof rejected the connection rather than dropping the key;
        //   3. `HYBRID_SIGNATURES` was negotiated;
        //   4. the bytes parse as an ML-DSA public key;
        //   5. `pq_binding_verified`, so a legacy peer that sent a key with no proof at all
        //      contributes nothing.
        //
        // Together with (1) that is the trust rule: the ML-DSA key has no binding to the
        // Ed25519 principal beyond *this authenticated connection asserting it*, plus the PQ
        // side signing for the DID. Nothing off-connection — no DID document, no registry —
        // attests it. Which is exactly why the claim's lifetime must be the connection's.
        {
            let claim = current_pq_key
                .and_then(|key| {
                    crate::replay_guard::SenderPrincipal::from_did(from)
                        .ok()
                        .map(|principal| (principal, key))
                })
                .map(|(principal, key)| self.capability_registry.claim_pq_key(principal, key));
            self.replace_pq_key_claim(claim);
        }

        // Store the incoming QUIC connection in session_manager
        {
            // Route through the canonical installer so the replace-if-closed rule cannot drift
            // between this path and `SessionManager::store_incoming_connection` (#2504). The
            // DID-TLS binding above has already been verified, so `from` is authenticated here.
            let connections_arc = self.session_manager.read().await.connections_arc();
            crate::session::install_incoming_connection(
                &connections_arc,
                Some(self.own_did.as_str()),
                from.to_string(),
                connection.clone(),
            )
            .await;
        }

        // Add peer to neighbor sets if topology is enabled
        if let Some(ref sets) = self.neighbor_sets {
            if let Some(peer_topology) = topology_info {
                // TODO(Phase 2.3): Get trust score from PolicyOracle when available
                // For now, use neutral trust score for topology decisions
                let trust_score = 0.5f32;

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

        // Answer only if we are the responder, and only once.
        //
        // Everything above ran unconditionally and still does: this Hello was verified against
        // this connection's certificate and its peer state installed, whichever end we are.
        // What is conditional is whether it *demands an answer*. A Hello response is byte-for-
        // byte the same kind of message as an initial Hello, so a node that answers every
        // Hello answers responses too, and the two ends sustain the exchange until the rate
        // limiter denies one — roughly forty messages, and the peer's application-message
        // budget with them (#2532).
        //
        // The exchange is therefore bounded by the protocol: the initiator owes one Hello, the
        // responder owes one answer, and receiving that answer completes the initiator's
        // handshake rather than obliging it to speak again. Rate limiting stays defence in
        // depth; it is no longer what stops a *successful* handshake.
        //
        // A repeated Hello on an already-answered connection is still verified above — so any
        // state it carries is refreshed — but is not answered, so it cannot restart a chain.
        match self.direction {
            ConnectionDirection::Outbound => {
                debug!(
                    peer_did = %from,
                    "Hello response completes our handshake; not replying (#2532)"
                );
            }
            ConnectionDirection::Inbound => {
                if self
                    .hello_responded
                    .swap(true, std::sync::atomic::Ordering::SeqCst)
                {
                    debug!(
                        peer_did = %from,
                        "Already answered this connection's Hello; not replying again (#2532)"
                    );
                } else {
                    self.send_hello_response(connection, from).await?;
                }
            }
        }

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

#[cfg(test)]
mod binding_before_authentication {
    //! A claim becomes this connection's identity only *after* every DID-TLS check passes.
    //!
    //! `handle_hello` calls `record_authenticated_peer` below all three checks, and that
    //! ordering is what makes an authenticated identity mean anything. Moving the call above
    //! them — so an unverified claim is bound first and verified afterwards — survived every
    //! suite that should plausibly have caught it (#2554): `hello_current_cert_binding`,
    //! `preauth_authentication_deadline`, `authenticated_rate_limit_identity`.
    //!
    //! It survived because those suites assert through the *connection*, and the connection is
    //! exactly what hides this. A failed check returns `Err`, the error propagates out of the
    //! stream loop, the handler task ends and the transport is destroyed — so "the peer entry
    //! is absent" and "the connection closed" are true whether the bogus identity was bound or
    //! not. The safety rests entirely on there being nothing between the failing check and its
    //! `return`, which is a property of this function, not of the transport.
    //!
    //! So these tests call `handle_hello` directly, on a real QUIC connection with a real
    //! certificate, and read the state the invariant is actually about. The dispatch loop is
    //! deliberately absent: its teardown is the masking, and a test that keeps it can only
    //! re-observe what the suites above already do. Everything else is production — real TLS,
    //! real `BindingInfo`, real admission table.
    //!
    //! Two observables, both owned by the test and neither global:
    //!
    //! - `authenticated_peer` — the identity bound to this connection. This *is* the state
    //!   "authenticated" names.
    //! - the pre-authentication slot (#2547/#2551) — `record_authenticated_peer` also drops the
    //!   admission guard, so a released slot is a second, independent witness that it ran. The
    //!   table is constructed here, so it counts one connection and nothing else in the process
    //!   can move it. The published `icn_network_preauth_admission_released_total` counter
    //!   would say the same thing, but it is process-global and shared with every concurrently
    //!   running test.

    use super::*;
    use crate::preauth_admission::PreAuthAdmission;
    use crate::replay_guard::ReplayGuard;
    use crate::{RateLimitConfig, RateLimiter, SessionManager, VersionInfo};
    use icn_identity::{BindingInfo, IdentityBundle};
    use quinn::{ClientConfig, Endpoint, ServerConfig};
    use std::collections::HashMap;
    use std::net::SocketAddr;
    use std::sync::atomic::AtomicBool;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    fn init() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    }

    /// One inbound connection, mid-handshake: admitted, anonymous, Hello not yet delivered.
    ///
    /// The endpoints and the client's connection are held for the test's lifetime because
    /// dropping either end closes the connection, and `current_peer_certificate` would then
    /// have nothing to read — which would make every assertion below pass for the wrong reason.
    struct Harness {
        ctx: ConnectionContext,
        connection: quinn::Connection,
        admission: Arc<PreAuthAdmission>,
        /// The same registry `ctx` holds, kept separately so a test can outlive the context and
        /// ask what its teardown released (#2644).
        capability_registry: Arc<crate::capability_evidence::LiveCapabilityRegistry>,
        source: SocketAddr,
        _client_connection: quinn::Connection,
        _client_endpoint: Endpoint,
        _node_endpoint: Endpoint,
    }

    impl Harness {
        /// Bring up `node` and dial it presenting `wire_identity`'s certificate.
        ///
        /// `wire_identity` is `None` for the client that declines to present one at all. It is
        /// separate from anything the Hello later *claims* on purpose: the gap between the
        /// certificate on the wire and the DID in the message is the whole subject.
        async fn accept(
            node: &IdentityBundle,
            wire_identity: Option<&IdentityBundle>,
        ) -> Result<Self> {
            let node_endpoint = Endpoint::server(
                ServerConfig::with_crypto(Arc::new(
                    quinn::crypto::rustls::QuicServerConfig::try_from(
                        crate::tls::create_server_config(
                            vec![node.tls_cert().clone()],
                            node.tls_key(),
                        )?,
                    )?,
                )),
                "127.0.0.1:0".parse()?,
            )?;
            let node_addr = node_endpoint.local_addr()?;

            let client_tls = match wire_identity {
                Some(bundle) => crate::tls::create_tofu_client_config(
                    vec![bundle.tls_cert().clone()],
                    bundle.tls_key(),
                )?,
                // The server's TOFU verifier does not make client auth mandatory, so a client
                // may simply decline. That still completes the TLS handshake and still reaches
                // `handle_hello` — with nothing to bind a claim to.
                None => {
                    let mut cfg = rustls::ClientConfig::builder()
                        .dangerous()
                        .with_custom_certificate_verifier(Arc::new(AcceptAnyServerCert))
                        .with_no_client_auth();
                    cfg.alpn_protocols = vec![b"icn/1".to_vec()];
                    cfg
                }
            };
            let mut client_endpoint = Endpoint::client("127.0.0.1:0".parse()?)?;
            client_endpoint.set_default_client_config(ClientConfig::new(Arc::new(
                quinn::crypto::rustls::QuicClientConfig::try_from(client_tls)?,
            )));

            let accepting = {
                let endpoint = node_endpoint.clone();
                tokio::spawn(async move {
                    let incoming = endpoint
                        .accept()
                        .await
                        .ok_or_else(|| anyhow::anyhow!("node endpoint closed before accepting"))?;
                    Ok::<quinn::Connection, anyhow::Error>(incoming.await?)
                })
            };
            let client_connection = client_endpoint.connect(node_addr, "localhost")?.await?;
            let connection = accepting.await??;

            // The slot this connection would have been admitted on (#2547). Taking it here
            // reproduces the state `handle_connection` hands to a real inbound context, so the
            // release below is the production release and not a test-only side effect.
            let source = connection.remote_address();
            let admission = Arc::new(PreAuthAdmission::new());
            let guard = admission.try_admit(source).map_err(|refused| {
                anyhow::anyhow!("test connection was not admitted: {}", refused.as_str())
            })?;

            let capability_registry =
                Arc::new(crate::capability_evidence::LiveCapabilityRegistry::new());

            let ctx = ConnectionContext::new(
                Arc::new(|_| {}),
                Arc::new(RateLimiter::new(RateLimitConfig::default())),
                Arc::new(RwLock::new(ReplayGuard::new(300, 3600))),
                None, // neighbor_sets
                None, // topology_config
                Arc::new(RwLock::new(SessionManager::new())),
                Arc::new(RwLock::new(HashMap::new())),
                capability_registry.clone(),
                None, // blob_registry
                None, // misbehavior_detector
                node.clone(),
                node.did().clone(),
                ConnectionDirection::Inbound,
                Arc::new(AtomicBool::new(false)),
                None, // expected_peer
                Some(guard),
                None, // pre_auth_budget: this harness exercises Hello, not source accounting
            );

            Ok(Self {
                ctx,
                connection,
                admission,
                capability_registry,
                source,
                _client_connection: client_connection,
                _client_endpoint: client_endpoint,
                _node_endpoint: node_endpoint,
            })
        }

        /// Deliver a Hello, with the claimed DID and the binding supplied independently.
        async fn hello(&self, from: &Did, binding: &BindingInfo) -> Result<()> {
            self.hello_advertising(from, binding, VersionInfo::new("icnd-test".to_string()))
                .await
        }

        /// The same, with the peer's advertised capabilities chosen by the caller.
        ///
        /// `VersionInfo::new` carries `CapabilityFlags::current()`, which includes
        /// `DURABLE_SIGNING_SEQUENCE` — so the default Hello is a durable one, and stating the
        /// *absence* of the capability needs this.
        async fn hello_advertising(
            &self,
            from: &Did,
            binding: &BindingInfo,
            version: VersionInfo,
        ) -> Result<()> {
            self.ctx
                .handle_hello(
                    &self.connection,
                    from,
                    binding,
                    &Some(version),
                    &None,         // topology_info
                    &[0x2eu8; 32], // x25519_public
                    None,          // ml_dsa_public
                    None,          // ml_kem_public
                    None,          // pq_binding_proof
                )
                .await
        }

        /// The identity proven for this connection. `None` means "we cannot say who this is".
        async fn authenticated(&self) -> Option<Did> {
            self.ctx.authenticated_peer().await
        }

        /// Does any live connection currently claim the durable signing sequence for `did`'s key?
        fn proves_durable(&self, did: &Did) -> bool {
            Self::proves_durable_in(&self.capability_registry, did)
        }

        /// The same question asked of a registry that has outlived its context.
        fn proves_durable_in(
            registry: &crate::capability_evidence::LiveCapabilityRegistry,
            did: &Did,
        ) -> bool {
            let principal = crate::replay_guard::SenderPrincipal::from_did(did)
                .expect("a generated DID decodes to a key");
            registry.proves_durable_signing_sequence(&principal)
        }

        /// Pre-authentication slots this connection's source still holds: 1 while anonymous,
        /// 0 once `record_authenticated_peer` has dropped the guard.
        fn slots_held(&self) -> usize {
            self.admission.live_for(self.source)
        }

        /// Deliver a Hello that advertises an ML-DSA key, with the binding proof supplied
        /// independently of the key (#2646).
        ///
        /// Independent on purpose: the proof is what makes the key *this DID's*, and a case
        /// that can only ever pass a matching pair cannot show that.
        #[cfg(feature = "post-quantum")]
        async fn hello_with_pq(
            &self,
            from: &Did,
            binding: &BindingInfo,
            ml_dsa_public: Option<Vec<u8>>,
            proof: Option<PqBindingProof>,
        ) -> Result<()> {
            self.ctx
                .handle_hello(
                    &self.connection,
                    from,
                    binding,
                    &Some(VersionInfo::new("icnd-test".to_string())),
                    &None,
                    &[0x2eu8; 32],
                    ml_dsa_public,
                    None,
                    proof,
                )
                .await
        }

        /// What live connections currently prove about `did`'s ML-DSA key (#2646).
        #[cfg(feature = "post-quantum")]
        fn current_pq(&self, did: &Did) -> crate::capability_evidence::CurrentPqKey {
            let principal = crate::replay_guard::SenderPrincipal::from_did(did)
                .expect("a generated DID decodes to a key");
            self.capability_registry.current_pq_key(&principal)
        }
    }

    /// The invariant, stated once.
    ///
    /// Note what is deliberately *not* asserted: that the connection closed. It does, and
    /// `hello_current_cert_binding` already pins that — but closing is a consequence of the
    /// rejection, not evidence about when the identity was bound, and asserting it here is
    /// what let the reordering survive in the first place.
    async fn assert_left_anonymous(harness: &Harness, outcome: Result<()>, mode: &str) {
        assert!(
            outcome.is_err(),
            "{mode}: handle_hello must reject this Hello"
        );
        assert_eq!(
            harness.authenticated().await,
            None,
            "{mode}: the connection was recorded as authenticated by a Hello that failed \
             DID-TLS verification — a claim was bound before it was proven"
        );
        assert_eq!(
            harness.slots_held(),
            1,
            "{mode}: the pre-authentication slot was released for a connection that never \
             authenticated, so `record_authenticated_peer` ran before the checks (#2547/#2551)"
        );
    }

    /// (1) The binding names a DID other than the one that sent the Hello.
    ///
    /// The sender presents its own certificate and its own internally-valid binding, and
    /// claims to be somebody else. Nothing here is forged; the mismatch is the attack.
    #[tokio::test]
    async fn a_hello_naming_a_different_did_never_authenticates() -> Result<()> {
        init();
        let node = IdentityBundle::generate()?;
        let attacker = IdentityBundle::generate()?;
        let victim_did = IdentityBundle::generate()?.did().clone();

        let harness = Harness::accept(&node, Some(&attacker)).await?;
        let outcome = harness.hello(&victim_did, &attacker.binding_info()).await;

        assert_left_anonymous(&harness, outcome, "binding names a different DID").await;
        Ok(())
    }

    /// (2) The binding's signature does not verify under the claimed DID's key.
    #[tokio::test]
    async fn a_hello_with_an_invalid_binding_signature_never_authenticates() -> Result<()> {
        init();
        let node = IdentityBundle::generate()?;
        let peer = IdentityBundle::generate()?;

        let mut binding = peer.binding_info();
        binding.tls_binding_sig[0] ^= 0xff;

        let harness = Harness::accept(&node, Some(&peer)).await?;
        let outcome = harness.hello(peer.did(), &binding).await;

        assert_left_anonymous(&harness, outcome, "invalid binding signature").await;
        Ok(())
    }

    /// (3) The binding is genuine, but binds a certificate other than this connection's.
    ///
    /// Every node publishes its own `BindingInfo` in every Hello, so a replayed one is not
    /// privileged material. Checks (1) and (2) both pass here — only the tie to *this* TLS
    /// session fails, which is the check #2520 added.
    #[tokio::test]
    async fn a_hello_bound_to_another_certificate_never_authenticates() -> Result<()> {
        init();
        let node = IdentityBundle::generate()?;
        let attacker = IdentityBundle::generate()?;
        let victim = IdentityBundle::generate()?;

        // Wire certificate: the attacker's. Claimed DID and binding: the victim's authentic
        // material, correctly signed — over a certificate this connection is not presenting.
        let harness = Harness::accept(&node, Some(&attacker)).await?;
        let outcome = harness.hello(victim.did(), &victim.binding_info()).await;

        assert_left_anonymous(&harness, outcome, "binding is of a different certificate").await;
        Ok(())
    }

    /// (4) There is no certificate on the connection to bind anything to.
    ///
    /// The absent-certificate branch fails closed, and must fail closed *before* recording an
    /// identity: "cannot check" is the one case where a bound claim would be pure assertion.
    #[tokio::test]
    async fn a_hello_without_a_peer_certificate_never_authenticates() -> Result<()> {
        init();
        let node = IdentityBundle::generate()?;
        let claimant = IdentityBundle::generate()?;

        let harness = Harness::accept(&node, None).await?;
        let outcome = harness
            .hello(claimant.did(), &claimant.binding_info())
            .await;

        assert_left_anonymous(&harness, outcome, "no peer certificate").await;
        Ok(())
    }

    /// POSITIVE CONTROL: a verified Hello still authenticates, and releases exactly one slot.
    ///
    /// Without this, every assertion above is satisfied by a `handle_hello` that rejects
    /// everything. It also pins #2553's guarantee from the other side.
    ///
    /// "Exactly one" is measured against a *companion* slot held by the same source, because
    /// `release` saturates: a double release of the only outstanding slot leaves the count at 0
    /// either way and would be invisible. With a second slot outstanding, releasing twice takes
    /// the source to 0 instead of 1, and the repeated Hello below — verified afresh, as the
    /// handler always does — is what would do it if taking the guard were not idempotent.
    #[tokio::test]
    async fn a_verified_hello_authenticates_and_releases_exactly_its_own_slot() -> Result<()> {
        init();
        let node = IdentityBundle::generate()?;
        let peer = IdentityBundle::generate()?;

        let harness = Harness::accept(&node, Some(&peer)).await?;
        // A second anonymous connection from the same source, so an over-release is visible.
        let _companion = harness
            .admission
            .try_admit(harness.source)
            .map_err(|refused| anyhow::anyhow!("companion not admitted: {}", refused.as_str()))?;
        assert_eq!(
            harness.slots_held(),
            2,
            "two anonymous connections, one source"
        );

        harness.hello(peer.did(), &peer.binding_info()).await?;

        assert_eq!(
            harness.authenticated().await.as_ref(),
            Some(peer.did()),
            "a Hello passing every DID-TLS check must authenticate the connection"
        );
        assert_eq!(
            harness.slots_held(),
            1,
            "authenticating must release this connection's slot and only this connection's"
        );

        harness.hello(peer.did(), &peer.binding_info()).await?;
        assert_eq!(
            harness.slots_held(),
            1,
            "a repeated verified Hello must not release a slot this connection no longer holds"
        );
        Ok(())
    }

    /// A connection that never authenticates still returns its slot when it is torn down.
    ///
    /// The negative tests above assert the slot is *not* released by a rejected Hello. That is
    /// only safe because the guard's destructor is the other release path — otherwise pinning
    /// "not released here" would be pinning a leak. Both halves of #2547's "no exit path leaks
    /// a slot" therefore have to be asserted together.
    #[tokio::test]
    async fn a_rejected_hello_still_returns_its_slot_when_the_connection_is_dropped() -> Result<()>
    {
        init();
        let node = IdentityBundle::generate()?;
        let attacker = IdentityBundle::generate()?;
        let victim = IdentityBundle::generate()?;

        let harness = Harness::accept(&node, Some(&attacker)).await?;
        let outcome = harness.hello(victim.did(), &victim.binding_info()).await;
        assert_left_anonymous(&harness, outcome, "teardown precondition").await;

        // What the handler task ending does: the context is dropped, and the guard with it.
        let Harness {
            ctx,
            admission,
            source,
            ..
        } = harness;
        drop(ctx);

        assert_eq!(
            admission.live_for(source),
            0,
            "dropping the connection context must return the slot it never released"
        );
        Ok(())
    }

    /// Accepts the node's self-signed certificate, for the no-client-certificate case only.
    #[derive(Debug)]
    struct AcceptAnyServerCert;

    impl rustls::client::danger::ServerCertVerifier for AcceptAnyServerCert {
        fn verify_server_cert(
            &self,
            _e: &rustls::pki_types::CertificateDer<'_>,
            _i: &[rustls::pki_types::CertificateDer<'_>],
            _n: &rustls::pki_types::ServerName<'_>,
            _o: &[u8],
            _now: rustls::pki_types::UnixTime,
        ) -> std::result::Result<rustls::client::danger::ServerCertVerified, rustls::Error>
        {
            Ok(rustls::client::danger::ServerCertVerified::assertion())
        }
        fn verify_tls12_signature(
            &self,
            _m: &[u8],
            _c: &rustls::pki_types::CertificateDer<'_>,
            _d: &rustls::DigitallySignedStruct,
        ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error>
        {
            Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
        }
        fn verify_tls13_signature(
            &self,
            _m: &[u8],
            _c: &rustls::pki_types::CertificateDer<'_>,
            _d: &rustls::DigitallySignedStruct,
        ) -> std::result::Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error>
        {
            Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
        }
        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
            vec![rustls::SignatureScheme::ED25519]
        }
    }

    // ==================================================================================
    // #2644 — live capability evidence is leased for exactly one connection's lifetime
    // ==================================================================================

    /// A verified Hello claims the durable regime for the key it just authenticated.
    ///
    /// This is the write side of what `handlers::signed` reads. It has to be asserted through a
    /// real Hello on a real connection rather than by seeding a registry, because what makes the
    /// claim trustworthy is that it happens *after* the three #2520 DID-TLS checks — a claim
    /// registered before them would be an unproven peer's claim.
    ///
    /// Asked by *key*, not by the spelling the Hello used: the registry is indexed by
    /// `SenderPrincipal`, so an alias of the same key must find the same claim (#2640).
    #[tokio::test]
    async fn a_verified_hello_claims_the_durable_regime_for_the_key_it_authenticated() -> Result<()>
    {
        init();
        let node = IdentityBundle::generate()?;
        let peer = IdentityBundle::generate()?;
        let harness = Harness::accept(&node, Some(&peer)).await?;

        assert!(
            !harness.proves_durable(peer.did()),
            "precondition: nothing is claimed before the Hello"
        );

        harness.hello(peer.did(), &peer.binding_info()).await?;

        assert!(
            harness.proves_durable(peer.did()),
            "a Hello passing every DID-TLS check and advertising DURABLE_SIGNING_SEQUENCE must \
             make this key durable for as long as this connection lasts"
        );

        let key = peer.did().to_verifying_key().expect("the DID decodes");
        let alias = Did::from_str(&format!(
            "did:icn:{}",
            multibase::encode(multibase::Base::Base16Lower, key.as_bytes())
        ))?;
        assert_ne!(
            alias.as_str(),
            peer.did().as_str(),
            "CONTROL: a different string"
        );
        assert!(
            harness.proves_durable(&alias),
            "the claim is about the signing key, so another spelling of it must find the same \
             claim rather than a missing one (#2640)"
        );
        Ok(())
    }

    /// The other half: the claim goes away with the connection that made it.
    ///
    /// Asserting only that a Hello registers would pin a leak — that is exactly the shape #2644
    /// reported, where `peer_connections` kept a capability row forever because no exit path
    /// removed it. Nothing here calls a release: the lease lives in the `ConnectionContext`,
    /// which the connection handler owns on its own stack frame, so dropping the context *is*
    /// the release and every way that handler can exit performs it.
    #[tokio::test]
    async fn a_connection_that_goes_away_stops_proving_the_capability() -> Result<()> {
        init();
        let node = IdentityBundle::generate()?;
        let peer = IdentityBundle::generate()?;
        let registry = {
            let harness = Harness::accept(&node, Some(&peer)).await?;
            harness.hello(peer.did(), &peer.binding_info()).await?;
            assert!(harness.proves_durable(peer.did()), "precondition: claimed");
            let registry = harness.capability_registry.clone();
            drop(harness);
            registry
        };

        assert!(
            !Harness::proves_durable_in(&registry, peer.did()),
            "a capability claim outliving the connection that made it is the #2644 defect: a \
             peer that rolled back would keep a durable regime it no longer has"
        );
        assert_eq!(
            registry.claimed_keys(),
            0,
            "and the entry must be gone, not merely zeroed, or the registry grows with every \
             peer that has ever connected"
        );
        Ok(())
    }

    /// A peer that does not advertise the capability claims nothing.
    ///
    /// Without this, every test above would pass on a build that claimed unconditionally — and
    /// unconditional claiming is worse than the bug, since it would promise durability for
    /// genuinely pre-#2510 senders.
    #[tokio::test]
    async fn a_hello_without_the_capability_claims_nothing() -> Result<()> {
        init();
        let node = IdentityBundle::generate()?;
        let peer = IdentityBundle::generate()?;
        let harness = Harness::accept(&node, Some(&peer)).await?;

        let mut legacy = VersionInfo::new("icnd-before-2510".to_string());
        legacy
            .capabilities
            .remove(crate::CapabilityFlags::DURABLE_SIGNING_SEQUENCE);
        assert!(
            !legacy
                .capabilities
                .contains(crate::CapabilityFlags::DURABLE_SIGNING_SEQUENCE),
            "CONTROL: the peer really is not advertising it"
        );

        harness
            .hello_advertising(peer.did(), &peer.binding_info(), legacy)
            .await?;

        assert_eq!(
            harness.authenticated().await.as_ref(),
            Some(peer.did()),
            "CONTROL: the connection still authenticated, so this is about the capability and \
             not about a rejected Hello"
        );
        assert!(
            !harness.proves_durable(peer.did()),
            "no advertisement, no claim"
        );
        Ok(())
    }

    /// A Hello that fails the DID-TLS checks claims nothing for the DID it named.
    ///
    /// The claim must sit behind the same gate the authentication does, or an unauthenticated
    /// peer could assert a durable regime for somebody else's key by naming it.
    #[tokio::test]
    async fn a_rejected_hello_claims_nothing_for_the_did_it_named() -> Result<()> {
        init();
        let node = IdentityBundle::generate()?;
        let attacker = IdentityBundle::generate()?;
        let victim = IdentityBundle::generate()?;

        // The attacker's certificate is on the wire; the Hello names the victim.
        let harness = Harness::accept(&node, Some(&attacker)).await?;
        let outcome = harness.hello(victim.did(), &attacker.binding_info()).await;
        assert!(
            outcome.is_err(),
            "CONTROL: the binding check must reject this"
        );

        assert!(
            !harness.proves_durable(victim.did()),
            "a rejected Hello must not leave a durable claim standing for the key it named"
        );
        assert!(
            !harness.proves_durable(attacker.did()),
            "nor for the key that was actually on the wire, which never claimed anything"
        );
        Ok(())
    }

    /// A repeated Hello re-states the claim; it does not stack a second one.
    ///
    /// The lease is reference counted, so a connection that registered twice would need two
    /// drops to release — and only one ever happens. That would leave a permanent claim behind
    /// a connection that has closed, which is the reported defect reintroduced by a different
    /// route. Mirrors `a_verified_hello_authenticates_and_releases_exactly_its_own_slot`, which
    /// pins the same idempotence for the admission slot.
    #[tokio::test]
    async fn a_repeated_hello_does_not_stack_a_second_claim() -> Result<()> {
        init();
        let node = IdentityBundle::generate()?;
        let peer = IdentityBundle::generate()?;
        let registry = {
            let harness = Harness::accept(&node, Some(&peer)).await?;
            harness.hello(peer.did(), &peer.binding_info()).await?;
            harness.hello(peer.did(), &peer.binding_info()).await?;
            harness.hello(peer.did(), &peer.binding_info()).await?;
            assert!(harness.proves_durable(peer.did()), "precondition: claimed");
            let registry = harness.capability_registry.clone();
            drop(harness);
            registry
        };

        assert!(
            !Harness::proves_durable_in(&registry, peer.did()),
            "three Hellos on one connection are one claim, so one teardown must release it"
        );
        Ok(())
    }

    /// A peer that stops advertising the capability mid-connection stops proving it.
    ///
    /// The claim tracks the peer's current statement rather than the best one it ever made.
    /// Otherwise a downgrade would have to wait for the connection to close to take effect,
    /// which is the same "once proved, always proved" error at a shorter timescale.
    #[tokio::test]
    async fn a_hello_that_drops_the_capability_retracts_the_claim() -> Result<()> {
        init();
        let node = IdentityBundle::generate()?;
        let peer = IdentityBundle::generate()?;
        let harness = Harness::accept(&node, Some(&peer)).await?;

        harness.hello(peer.did(), &peer.binding_info()).await?;
        assert!(harness.proves_durable(peer.did()), "precondition: claimed");

        let mut rolled_back = VersionInfo::new("icnd-rolled-back".to_string());
        rolled_back
            .capabilities
            .remove(crate::CapabilityFlags::DURABLE_SIGNING_SEQUENCE);
        harness
            .hello_advertising(peer.did(), &peer.binding_info(), rolled_back)
            .await?;

        assert!(
            !harness.proves_durable(peer.did()),
            "the peer's current Hello does not advertise it, so it is no longer proved"
        );
        Ok(())
    }
    // -----------------------------------------------------------------------------------
    // #2646 — ML-DSA key evidence: installed only after authentication, released with the
    // connection, and never accumulated.
    // -----------------------------------------------------------------------------------

    /// A Hello that fails DID-TLS verification installs no PQ evidence for the DID it named.
    ///
    /// The consequence if it did: a connection could name a principal it cannot authenticate
    /// as and choose the ML-DSA key that principal's hybrid envelopes are verified against,
    /// without ever holding that principal's Ed25519 key. Every rejection mode is driven,
    /// because the invariant is a property of where `handle_hello` returns, not of any one
    /// check.
    #[tokio::test]
    #[cfg(feature = "post-quantum")]
    async fn a_hello_that_fails_authentication_installs_no_pq_evidence() -> Result<()> {
        init();
        let node = IdentityBundle::generate()?;
        let connecting = IdentityBundle::generate()?;
        let named = IdentityBundle::generate()?;
        let connecting_key = connecting
            .keypair()?
            .pq_public_key()
            .map(|pk| pk.as_bytes().to_vec());
        assert!(
            connecting_key.is_some(),
            "CONTROL: the connecting identity has a PQ key"
        );

        // (1) The binding names a DID other than the connecting identity's.
        {
            let harness = Harness::accept(&node, Some(&connecting)).await?;
            let proof = PqBindingProof::create(named.did(), &connecting.keypair()?);
            let outcome = harness
                .hello_with_pq(
                    named.did(),
                    &connecting.binding_info(),
                    connecting_key.clone(),
                    proof,
                )
                .await;
            assert!(outcome.is_err(), "CONTROL: this Hello must be rejected");
            assert_eq!(
                harness.current_pq(named.did()),
                crate::capability_evidence::CurrentPqKey::NoCurrentKey,
                "a connection that never proved the named DID's Ed25519 key installed PQ \
                 evidence against that principal"
            );
        }

        // (2) A replayed binding: internally valid, but for another certificate.
        {
            let harness = Harness::accept(&node, Some(&connecting)).await?;
            let proof = PqBindingProof::create(named.did(), &connecting.keypair()?);
            let outcome = harness
                .hello_with_pq(
                    named.did(),
                    &named.binding_info(),
                    connecting_key.clone(),
                    proof,
                )
                .await;
            assert!(outcome.is_err(), "CONTROL: this Hello must be rejected");
            assert_eq!(
                harness.current_pq(named.did()),
                crate::capability_evidence::CurrentPqKey::NoCurrentKey,
                "a replayed binding installed PQ evidence for the DID it named"
            );
        }

        // (3) The connection presents no certificate at all.
        {
            let harness = Harness::accept(&node, None).await?;
            let proof = PqBindingProof::create(named.did(), &connecting.keypair()?);
            let outcome = harness
                .hello_with_pq(named.did(), &named.binding_info(), connecting_key, proof)
                .await;
            assert!(outcome.is_err(), "CONTROL: this Hello must be rejected");
            assert_eq!(
                harness.current_pq(named.did()),
                crate::capability_evidence::CurrentPqKey::NoCurrentKey,
                "an anonymous connection installed PQ evidence"
            );
        }
        Ok(())
    }

    /// An authenticated peer whose ML-DSA key carries no binding proof installs no evidence.
    ///
    /// This is the legacy-node path: `verify_pq_binding` returns `Ok(false)` rather than an
    /// error, the connection is allowed, and the key is discarded. The discard has to reach the
    /// evidence index too — a key nobody proved control of must not decide how that peer's
    /// hybrid envelopes are verified.
    #[tokio::test]
    #[cfg(feature = "post-quantum")]
    async fn an_unproven_ml_dsa_key_installs_no_evidence() -> Result<()> {
        init();
        let node = IdentityBundle::generate()?;
        let peer = IdentityBundle::generate()?;
        let key = peer
            .keypair()?
            .pq_public_key()
            .map(|pk| pk.as_bytes().to_vec());

        let harness = Harness::accept(&node, Some(&peer)).await?;
        harness
            .hello_with_pq(peer.did(), &peer.binding_info(), key, None)
            .await?;

        assert_eq!(
            harness.authenticated().await.as_ref(),
            Some(peer.did()),
            "CONTROL: the connection did authenticate; only the PQ key was unproven"
        );
        assert_eq!(
            harness.current_pq(peer.did()),
            crate::capability_evidence::CurrentPqKey::NoCurrentKey,
            "an ML-DSA key advertised without a binding proof must not become current evidence"
        );
        Ok(())
    }

    /// Malformed ML-DSA key material is refused at the ingestion boundary, not carried inward.
    ///
    /// `PqBindingProof::verify` parses the advertised key before checking the signature over
    /// it, so a key that cannot parse cannot carry a valid proof and the *connection* is
    /// rejected. That is the earliest boundary available, and it is why the registry never
    /// holds unparseable bytes in production — the fail-closed branch in
    /// `verify_with_cached_pq_key` is a backstop for a future ingestion path, not a live case.
    #[tokio::test]
    #[cfg(feature = "post-quantum")]
    async fn malformed_ml_dsa_key_material_is_refused_at_the_hello_boundary() -> Result<()> {
        init();
        let node = IdentityBundle::generate()?;
        let peer = IdentityBundle::generate()?;

        let harness = Harness::accept(&node, Some(&peer)).await?;
        let proof = PqBindingProof::create(peer.did(), &peer.keypair()?);
        assert!(proof.is_some(), "CONTROL: the peer can make a proof");
        let outcome = harness
            .hello_with_pq(
                peer.did(),
                &peer.binding_info(),
                Some(vec![0xDEu8, 0xAD, 0xBE, 0xEF]),
                proof,
            )
            .await;

        assert!(
            outcome.is_err(),
            "a Hello advertising unparseable ML-DSA key material must be rejected"
        );
        assert_eq!(
            harness.current_pq(peer.did()),
            crate::capability_evidence::CurrentPqKey::NoCurrentKey,
            "and nothing may be left behind for it"
        );
        Ok(())
    }

    /// A properly proven key becomes current, and stays current for one connection's lifetime.
    #[tokio::test]
    #[cfg(feature = "post-quantum")]
    async fn a_proven_ml_dsa_key_becomes_the_current_key() -> Result<()> {
        init();
        let node = IdentityBundle::generate()?;
        let peer = IdentityBundle::generate()?;
        let key = peer
            .keypair()?
            .pq_public_key()
            .map(|pk| pk.as_bytes().to_vec())
            .expect("a PQ key");

        let harness = Harness::accept(&node, Some(&peer)).await?;
        harness
            .hello_with_pq(
                peer.did(),
                &peer.binding_info(),
                Some(key.clone()),
                PqBindingProof::create(peer.did(), &peer.keypair()?),
            )
            .await?;

        assert_eq!(
            harness.current_pq(peer.did()),
            crate::capability_evidence::CurrentPqKey::UniqueCurrentKey(key.as_slice().into()),
            "a Hello that proved control of its ML-DSA key makes that key current"
        );
        Ok(())
    }

    /// A repeated Hello re-stating the same key does not stack a second claim.
    ///
    /// Stacking would make the claim outlive the connection by one release per extra Hello,
    /// which a peer controls the number of — evidence that never expires, at the peer's
    /// discretion.
    #[tokio::test]
    #[cfg(feature = "post-quantum")]
    async fn a_repeated_hello_does_not_stack_pq_claims() -> Result<()> {
        init();
        let node = IdentityBundle::generate()?;
        let peer = IdentityBundle::generate()?;
        let key = peer
            .keypair()?
            .pq_public_key()
            .map(|pk| pk.as_bytes().to_vec())
            .expect("a PQ key");
        let registry = {
            let harness = Harness::accept(&node, Some(&peer)).await?;
            for _ in 0..5 {
                harness
                    .hello_with_pq(
                        peer.did(),
                        &peer.binding_info(),
                        Some(key.clone()),
                        PqBindingProof::create(peer.did(), &peer.keypair()?),
                    )
                    .await?;
            }
            assert_eq!(
                harness.current_pq(peer.did()),
                crate::capability_evidence::CurrentPqKey::UniqueCurrentKey(key.as_slice().into()),
                "five Hellos advertising one key are still one key, not a conflict"
            );
            Arc::clone(&harness.capability_registry)
        };

        // The context is gone, so the connection is gone.
        let principal = crate::replay_guard::SenderPrincipal::from_did(peer.did())
            .expect("a generated DID decodes");
        assert_eq!(
            registry.current_pq_key(&principal),
            crate::capability_evidence::CurrentPqKey::NoCurrentKey,
            "one connection holds one claim however many Hellos it sends, so dropping it \
             releases everything it was claiming"
        );
        Ok(())
    }

    /// A Hello that changes the key leaves no ghost claim for the old one.
    ///
    /// A stale claim surviving here would present as a *conflict* between the peer's old and
    /// new key, which fails closed — so a peer rotating its ML-DSA key would silence itself.
    #[tokio::test]
    #[cfg(feature = "post-quantum")]
    async fn a_hello_that_changes_the_key_leaves_no_ghost() -> Result<()> {
        init();
        let node = IdentityBundle::generate()?;
        let peer = IdentityBundle::generate()?;
        let rotated = IdentityBundle::generate()?;
        let first = peer
            .keypair()?
            .pq_public_key()
            .map(|pk| pk.as_bytes().to_vec())
            .expect("a PQ key");
        let second = rotated
            .keypair()?
            .pq_public_key()
            .map(|pk| pk.as_bytes().to_vec())
            .expect("a PQ key");
        assert_ne!(first, second, "CONTROL: two distinct ML-DSA keys");

        let harness = Harness::accept(&node, Some(&peer)).await?;
        harness
            .hello_with_pq(
                peer.did(),
                &peer.binding_info(),
                Some(first),
                PqBindingProof::create(peer.did(), &peer.keypair()?),
            )
            .await?;

        // The same connection now advertises a different key, proven by that key's own holder.
        harness
            .hello_with_pq(
                peer.did(),
                &peer.binding_info(),
                Some(second.clone()),
                PqBindingProof::create(peer.did(), &rotated.keypair()?),
            )
            .await?;

        assert_eq!(
            harness.current_pq(peer.did()),
            crate::capability_evidence::CurrentPqKey::UniqueCurrentKey(second.as_slice().into()),
            "the newest Hello on this connection is the current one, and the previous key \
             must not linger as a second, conflicting claim"
        );
        Ok(())
    }

    /// A peer that stops advertising a key stops having a current one.
    #[tokio::test]
    #[cfg(feature = "post-quantum")]
    async fn withdrawing_the_key_returns_the_peer_to_no_current_key() -> Result<()> {
        init();
        let node = IdentityBundle::generate()?;
        let peer = IdentityBundle::generate()?;
        let key = peer
            .keypair()?
            .pq_public_key()
            .map(|pk| pk.as_bytes().to_vec())
            .expect("a PQ key");

        let harness = Harness::accept(&node, Some(&peer)).await?;
        harness
            .hello_with_pq(
                peer.did(),
                &peer.binding_info(),
                Some(key),
                PqBindingProof::create(peer.did(), &peer.keypair()?),
            )
            .await?;
        assert!(
            matches!(
                harness.current_pq(peer.did()),
                crate::capability_evidence::CurrentPqKey::UniqueCurrentKey(_)
            ),
            "precondition: claimed"
        );

        harness.hello(peer.did(), &peer.binding_info()).await?;

        assert_eq!(
            harness.current_pq(peer.did()),
            crate::capability_evidence::CurrentPqKey::NoCurrentKey,
            "the peer's current Hello advertises no ML-DSA key, so it has no current one"
        );
        Ok(())
    }

    /// The claim is released when the connection's context is dropped — every exit path.
    ///
    /// `ConnectionContext` is owned by the connection handler's own stack frame, so this is the
    /// destructor that runs on normal close, remote close, stream EOF, transport error, handler
    /// error and task cancellation alike. There is no release call for a future exit path to
    /// forget.
    #[tokio::test]
    #[cfg(feature = "post-quantum")]
    async fn the_claim_is_released_when_the_connection_ends() -> Result<()> {
        init();
        let node = IdentityBundle::generate()?;
        let peer = IdentityBundle::generate()?;
        let key = peer
            .keypair()?
            .pq_public_key()
            .map(|pk| pk.as_bytes().to_vec())
            .expect("a PQ key");
        let principal = crate::replay_guard::SenderPrincipal::from_did(peer.did())
            .expect("a generated DID decodes");

        let registry = {
            let harness = Harness::accept(&node, Some(&peer)).await?;
            harness
                .hello_with_pq(
                    peer.did(),
                    &peer.binding_info(),
                    Some(key.clone()),
                    PqBindingProof::create(peer.did(), &peer.keypair()?),
                )
                .await?;
            let registry = Arc::clone(&harness.capability_registry);
            assert_eq!(
                registry.current_pq_key(&principal),
                crate::capability_evidence::CurrentPqKey::UniqueCurrentKey(key.as_slice().into()),
                "precondition: claimed while the connection is up"
            );
            registry
        };

        assert_eq!(
            registry.current_pq_key(&principal),
            crate::capability_evidence::CurrentPqKey::NoCurrentKey,
            "the key must stop being current the moment the connection holding it goes"
        );
        Ok(())
    }
}
