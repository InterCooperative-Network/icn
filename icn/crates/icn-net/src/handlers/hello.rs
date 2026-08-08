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

            let ctx = ConnectionContext::new(
                Arc::new(|_| {}),
                Arc::new(RateLimiter::new(RateLimitConfig::default())),
                Arc::new(RwLock::new(ReplayGuard::new(300, 3600))),
                None, // neighbor_sets
                None, // topology_config
                Arc::new(RwLock::new(SessionManager::new())),
                Arc::new(RwLock::new(HashMap::new())),
                None, // blob_registry
                None, // misbehavior_detector
                node.clone(),
                node.did().clone(),
                ConnectionDirection::Inbound,
                Arc::new(AtomicBool::new(false)),
                None, // expected_peer
                Some(guard),
            );

            Ok(Self {
                ctx,
                connection,
                admission,
                source,
                _client_connection: client_connection,
                _client_endpoint: client_endpoint,
                _node_endpoint: node_endpoint,
            })
        }

        /// Deliver a Hello, with the claimed DID and the binding supplied independently.
        async fn hello(&self, from: &Did, binding: &BindingInfo) -> Result<()> {
            self.ctx
                .handle_hello(
                    &self.connection,
                    from,
                    binding,
                    &Some(VersionInfo::new("icnd-test".to_string())),
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

        /// Pre-authentication slots this connection's source still holds: 1 while anonymous,
        /// 0 once `record_authenticated_peer` has dropped the guard.
        fn slots_held(&self) -> usize {
            self.admission.live_for(self.source)
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

    /// COMPOSITION GUARD: **exactly one** site may establish an authenticated identity.
    ///
    /// Everything above pins the ordering *inside* `handle_hello`, and none of it can see a
    /// caller that binds an identity before dispatching to one. The masking is the same, one
    /// frame up: `ctx.record_authenticated_peer(&message.from)` placed above
    /// `ctx.handle_hello(...)` in `actor::connection` binds a sender-chosen name to every
    /// connection that sends a Hello, and leaves all six tests above green — along with
    /// `hello_current_cert_binding`, `preauth_authentication_deadline`,
    /// `authenticated_rate_limit_identity`, `peer_exchange_policy` and
    /// `preauth_connection_admission`. The rejected Hello still returns `Err`, the transport
    /// still dies, and every assertion any of them makes still holds.
    ///
    /// What makes the in-function ordering sufficient is that there is nowhere else the state
    /// can be established. That is a whole-crate property rather than a property of one
    /// function, so it is asserted the way this crate already asserts one — by refusing a
    /// second site (compare `weak_binding_verifier_is_confined_to_authorised_sites` in
    /// `tests/hello_current_cert_binding.rs`).
    ///
    /// **Counted, not merely located.** Confining the setter to a *file* is weaker than it
    /// looks: a second helper added beside the first one, and reached from the dispatcher,
    /// satisfies "the call lives in `hello.rs`" while authenticating a connection before any
    /// check runs. The same holds for a second direct writer added beside the setter in
    /// `handlers/mod.rs`, which additionally never drops the admission guard, so neither
    /// observable the tests above read would move. Both survive a location-only guard. So the
    /// assertion is a cardinality: **zero** sites fail, **one** passes, **two** fail, wherever
    /// the second one lives.
    ///
    /// Comments are stripped before counting, which is what makes counting safe — the prose
    /// documenting a guard is precisely where the token it forbids gets written down, and the
    /// paragraph above is itself a second textual occurrence of the call.
    #[test]
    fn authenticated_state_has_exactly_one_establishing_site() {
        const RECORD: &str = "record_authenticated_peer";
        const FIELD: &str = "authenticated_peer";
        const CALLER: &str = "src/handlers/hello.rs";
        const WRITER: &str = "src/handlers/mod.rs";
        // Every accessor on the field's lock that can yield a mutable reference to it.
        const MUTATORS: [&str; 3] = ["write()", "blocking_write()", "get_mut()"];

        // Composed at run time, so none of these literals is itself a match.
        let calls = [format!(".{RECORD}("), format!("::{RECORD}(")];
        let writes: Vec<String> = MUTATORS.iter().map(|m| format!("{FIELD}.{m}")).collect();

        /// Line comments and doc comments are prose, not sites.
        fn code_only(src: &str) -> String {
            src.lines()
                .map(|line| line.split_once("//").map_or(line, |(code, _)| code))
                .collect::<Vec<_>>()
                .join("\n")
        }
        fn tally(code: &str, needles: &[String]) -> usize {
            needles
                .iter()
                .map(|n| code.matches(n.as_str()).count())
                .sum()
        }

        let crate_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut call_sites: Vec<(String, usize)> = Vec::new();
        let mut write_sites: Vec<(String, usize)> = Vec::new();
        let mut stack = vec![crate_root.join("src")];

        while let Some(dir) = stack.pop() {
            for entry in std::fs::read_dir(&dir).expect("readable src dir") {
                let path = entry.expect("dir entry").path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().and_then(|e| e.to_str()) != Some("rs") {
                    continue;
                }
                let code = code_only(&std::fs::read_to_string(&path).expect("readable rust file"));
                let rel = path
                    .strip_prefix(crate_root)
                    .expect("path under crate root")
                    .to_string_lossy()
                    .replace('\\', "/");
                match tally(&code, &calls) {
                    0 => {}
                    n => call_sites.push((rel.clone(), n)),
                }
                match tally(&code, &writes) {
                    0 => {}
                    n => write_sites.push((rel, n)),
                }
            }
        }
        call_sites.sort();
        write_sites.sort();

        assert_eq!(
            call_sites,
            vec![(CALLER.to_owned(), 1)],
            "`{RECORD}` must be called exactly once, from `{CALLER}`; found {call_sites:?}. \
             None is a guard that proves nothing, because the identity is no longer bound \
             there. More than one is a path the ordering tests in this module cannot see: they \
             enter at `handle_hello`, so a second site — even in this same file — can bind a \
             claimed DID before a single DID-TLS check has run and leave every one of them \
             green. Authenticate through `handle_hello`, or extend this module to cover the \
             new site."
        );
        assert_eq!(
            write_sites,
            vec![(WRITER.to_owned(), 1)],
            "`{FIELD}` must be written exactly once, from `{WRITER}`; found {write_sites:?}. \
             `{RECORD}` is also what drops the pre-authentication admission guard, so a write \
             that bypasses it can bind an identity — and unbind it again before returning — \
             without moving either observable this module reads. Neither the identity nor the \
             slot would object; only this count does."
        );
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
}
