//! Peer exchange protocol handler - cross-network peer discovery
//!
//! Handles PeerExchangeMessage variants:
//! - Request: Respond with known peers
//! - Response: Process discovered peers
//! - Announce: New peer introduction
//! - Unannounce: Peer departure notification

use super::ConnectionContext;
use crate::candidate::{EndpointCandidate, EndpointKind};
use crate::protocol::{KnownPeer, NetworkMessage, PeerExchangeMessage};
use icn_identity::Did;
use icn_obs::metrics::peer_exchange::PeerExchangeDenialReason as DenialReason;
use std::net::{IpAddr, SocketAddr};
use tracing::{debug, info, warn};

impl ConnectionContext {
    /// Handle a PeerExchange message
    pub async fn handle_peer_exchange(
        &self,
        connection: &quinn::Connection,
        message: NetworkMessage,
        from: &Did,
        peer_msg: &PeerExchangeMessage,
    ) {
        match peer_msg {
            PeerExchangeMessage::Request {
                max_peers,
                network_filter,
            } => {
                // `from` is passed on only so a refusal can name what the sender *claimed*
                // to be; it is not used to authorise anything. See the handler.
                self.handle_peer_exchange_request(connection, from, *max_peers, network_filter)
                    .await;
            }
            PeerExchangeMessage::Response { peers, total_known } => {
                self.handle_peer_exchange_response(message, from, peers, *total_known);
            }
            PeerExchangeMessage::Announce { peer } => {
                self.handle_peer_announce(message, peer);
            }
            PeerExchangeMessage::Unannounce { did } => {
                self.handle_peer_unannounce(message, did);
            }
        }
    }

    /// Handle peer exchange request - respond with known peers
    ///
    /// Answering enumerates this node's authenticated neighbourhood — peer DIDs and
    /// reachable endpoints — so *who is asking* is part of the decision, not a detail
    /// (#2535). The claimed `from` on the request cannot answer that: the sender chose it
    /// and nobody verified it. The requester's identity is instead taken from the
    /// connection, where Hello bound it to the certificate in use (#2520).
    ///
    /// A refusal here is not an accusation. An unauthenticated request may simply have
    /// raced ahead of its own Hello, and a disabled local policy is this node's own
    /// posture, not the peer's fault — so nothing is scored against anyone. Compare the
    /// same reasoning in `handle_hello`: attributing a violation to an unproven DID would
    /// let an attacker degrade the reputation of any peer it cared to name.
    async fn handle_peer_exchange_request(
        &self,
        connection: &quinn::Connection,
        claimed_from: &Did,
        max_peers: usize,
        network_filter: &Option<String>,
    ) {
        icn_obs::metrics::peer_exchange::requests_received_inc();

        // Participation before identity. Whether this node hands out its neighbourhood at
        // all is its own posture, decided by the operator and independent of who is
        // asking, so a node that is not participating need not evaluate the requester at
        // all. `federation.enabled` already governs the rest of peer exchange in both
        // directions; answering a request was the one behaviour it did not reach (#2535).
        if !self.peer_exchange_enabled() {
            debug!(
                claimed_did = %claimed_from,
                remote_addr = %connection.remote_address(),
                "Declining peer exchange: this node does not participate in peer exchange"
            );
            icn_obs::metrics::peer_exchange::requests_denied_inc(DenialReason::Disabled);
            return;
        }

        // Identity next: until Hello has authenticated this connection there is no
        // requester, only a socket that sent bytes. Disclosing topology to it would hand a
        // node's neighbourhood to anyone able to complete a QUIC handshake.
        //
        // Logged at `debug!`, not `warn!`: reaching here needs nothing but a completed QUIC
        // handshake, so a remote can emit this line as fast as it can open streams. A
        // warning per attempt would let an unauthenticated caller drive this node's log
        // volume — and the event may be entirely benign, since a request can arrive before
        // its own Hello on a separate stream. The denial counter is the durable record;
        // `icn_peer_exchange_requests_denied_total{reason="unauthenticated_requester"}` is
        // what an operator alerts on, and it costs one increment rather than a log line.
        let Some(authenticated_from) = self.authenticated_peer().await else {
            debug!(
                claimed_did = %claimed_from,
                remote_addr = %connection.remote_address(),
                "Refusing peer exchange: this connection has not authenticated a peer, so \
                 the requester's identity is unproven"
            );
            icn_obs::metrics::peer_exchange::requests_denied_inc(
                DenialReason::UnauthenticatedRequester,
            );
            return;
        };

        // From here on `authenticated_from` is the requester. `claimed_from` is not consulted
        // again: it named nothing that was checked, and the two are deliberately distinct
        // identifiers so that reaching for the wrong one has to be deliberate.
        info!(
            "Received peer exchange request from {} (max={}, filter={:?})",
            authenticated_from, max_peers, network_filter
        );

        // Gather known peers from session manager
        let sm = self.session_manager.read().await;
        let connections = sm.connections().await;
        drop(sm);

        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut known_peers: Vec<KnownPeer> = Vec::new();

        for (did_str, conn) in connections {
            // Skip the requesting peer — identified by the DID this connection *proved*, so
            // a requester cannot steer what it is excluded from by choosing a `from`.
            if did_str == authenticated_from.as_str() {
                continue;
            }

            let remote = conn.remote_address();
            let ep = EndpointCandidate::new(remote, endpoint_kind_for_addr(remote));
            known_peers.push(KnownPeer {
                did: did_str.to_string(),
                endpoints: vec![ep],
                version: "0.1.0".to_string(),
                network_name: network_filter.clone(),
                observed_trust: None,
                last_seen: now_secs,
                is_local: false,
            });

            if known_peers.len() >= max_peers {
                break;
            }
        }

        let total_known = known_peers.len();

        // Send response
        let response = NetworkMessage::peer_exchange_response(
            self.own_did.clone(),
            authenticated_from.clone(),
            known_peers,
            total_known,
        );

        let connection_clone = connection.clone();
        let from_did = authenticated_from;
        tokio::spawn(async move {
            match connection_clone.open_bi().await {
                Ok((mut send_stream, _recv)) => {
                    if let Err(e) =
                        crate::protocol::write_message(&mut send_stream, &response).await
                    {
                        warn!("Failed to send peer exchange response: {}", e);
                    } else if let Err(e) = send_stream.finish() {
                        warn!("Failed to finish peer exchange response stream: {}", e);
                    } else {
                        info!("Sent {} peers to {}", total_known, from_did);
                        icn_obs::metrics::peer_exchange::responses_sent_inc();
                    }
                }
                Err(e) => {
                    warn!("Failed to open stream for peer exchange response: {}", e);
                }
            }
        });
    }

    /// Handle peer exchange response - process discovered peers
    fn handle_peer_exchange_response(
        &self,
        message: NetworkMessage,
        from: &Did,
        peers: &[KnownPeer],
        total_known: usize,
    ) {
        info!(
            "Received {} peers from {} (total known: {})",
            peers.len(),
            from,
            total_known
        );
        icn_obs::metrics::peer_exchange::responses_received_inc();
        icn_obs::metrics::peer_exchange::peers_discovered_add(peers.len() as u64);

        // Forward to handler for processing
        self.forward_to_handler(message);
    }

    /// Handle peer announce - new peer introduction
    fn handle_peer_announce(&self, message: NetworkMessage, peer: &KnownPeer) {
        info!("Peer announced: {} at {:?}", peer.did, peer.endpoints);
        icn_obs::metrics::peer_exchange::announces_received_inc();
        self.forward_to_handler(message);
    }

    /// Handle peer unannounce - peer departure
    fn handle_peer_unannounce(&self, message: NetworkMessage, did: &str) {
        info!("Peer unannounced: {}", did);
        icn_obs::metrics::peer_exchange::unannounces_received_inc();
        self.forward_to_handler(message);
    }
}

/// Derive an `EndpointKind` from the socket address's IP characteristics.
///
/// Private/loopback/link-local addresses (RFC1918, ULA, fe80::/10) are classified
/// as `Local`; everything else is `Public`. This prevents misclassifying LAN peers
/// as publicly reachable when the observed remote address is non-routable.
fn endpoint_kind_for_addr(addr: SocketAddr) -> EndpointKind {
    match addr.ip() {
        IpAddr::V4(ip) => {
            if ip.is_private() || ip.is_loopback() || ip.is_link_local() {
                EndpointKind::Local
            } else {
                EndpointKind::Public
            }
        }
        IpAddr::V6(ip) => {
            let octets = ip.octets();
            // Loopback: ::1
            // ULA: fc00::/7 (first byte high bits 1111110x)
            // Link-local: fe80::/10 (first byte 0xfe, second byte high bits 10)
            let is_loopback = ip.is_loopback();
            let is_ula = octets[0] & 0xfe == 0xfc;
            let is_link_local = octets[0] == 0xfe && (octets[1] & 0xc0) == 0x80;
            if is_loopback || is_ula || is_link_local {
                EndpointKind::Local
            } else {
                EndpointKind::Public
            }
        }
    }
}
