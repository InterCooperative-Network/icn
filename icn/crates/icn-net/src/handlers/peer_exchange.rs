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
///
/// IPv4-mapped addresses are unmapped before classifying, and that is load bearing rather than
/// tidiness. `listen_addr` defaults to `[::]:7777`, nothing in this crate sets `IPV6_V6ONLY`, and
/// where the OS leaves it off the socket is dual-stack — so an IPv4 peer is reported as
/// `::ffff:a.b.c.d`. Rust does not unmap that implicitly, so without this the v6 arm would see a
/// LAN peer, match neither ULA nor link-local, and call it `Public`: precisely the
/// misclassification described above. `Ipv6Addr::is_loopback` has the same gap — it is true only
/// for `::1`, never for a mapped `127/8` address.
///
/// Only `::ffff:0:0/96` is unmapped, via [`std::net::Ipv6Addr::to_ipv4_mapped`]. Deprecated
/// IPv4-*compatible* addresses (`::a.b.c.d`) stay on the v6 path: they are not what a dual-stack
/// socket produces, and `to_ipv4` would additionally rewrite `::1` to `0.0.0.1` and classify v6
/// loopback through the IPv4 rules. This matches `SourceKey::from_addr` (#2557), so the two
/// subsystems agree on what a peer's address means.
///
/// Normalising once here keeps both classification arms exactly as they were: what counts as local
/// or public is unchanged, only the representation it is read from.
fn endpoint_kind_for_addr(addr: SocketAddr) -> EndpointKind {
    let normalized = match addr.ip() {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => IpAddr::V6(v6),
        },
        already_v4 => already_v4,
    };

    match normalized {
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

#[cfg(test)]
mod endpoint_kind_tests {
    use super::*;

    fn kind(addr: &str) -> EndpointKind {
        endpoint_kind_for_addr(addr.parse::<SocketAddr>().expect("test address"))
    }

    /// The address a dual-stack listener actually reports for an IPv4 peer.
    ///
    /// `listen_addr` defaults to `[::]:7777` and nothing sets `IPV6_V6ONLY`, so an IPv4 peer
    /// arrives as `::ffff:a.b.c.d`. Rust does not unmap that implicitly, so without normalization
    /// these take the IPv6 arm, match none of loopback/ULA/link-local, and come out `Public` —
    /// exactly the misclassification this function exists to prevent.
    fn mapped(v4: &str) -> String {
        format!("[::ffff:{v4}]:7777")
    }

    #[test]
    fn mapped_rfc1918_is_local() {
        for v4 in ["10.0.0.1", "172.16.0.1", "192.168.1.5"] {
            assert_eq!(
                kind(&mapped(v4)),
                EndpointKind::Local,
                "mapped private address ::ffff:{v4} was not classified Local"
            );
        }
    }

    #[test]
    fn mapped_loopback_is_local() {
        assert_eq!(
            kind(&mapped("127.0.0.1")),
            EndpointKind::Local,
            "mapped loopback was not classified Local — Ipv6Addr::is_loopback() is true only for \
             ::1, never for a mapped 127/8 address"
        );
    }

    #[test]
    fn mapped_link_local_is_local() {
        assert_eq!(
            kind(&mapped("169.254.1.1")),
            EndpointKind::Local,
            "mapped IPv4 link-local was not classified Local"
        );
    }

    #[test]
    fn mapped_public_v4_is_public() {
        // TEST-NET-3, a documentation range the existing V4 logic treats as non-private.
        assert_eq!(kind(&mapped("203.0.113.7")), EndpointKind::Public);
    }

    /// The strongest compact invariant: representation must not change classification.
    #[test]
    fn native_and_mapped_v4_classify_identically() {
        for v4 in [
            "10.0.0.1",
            "172.16.0.1",
            "192.168.1.5",
            "127.0.0.1",
            "169.254.1.1",
            "203.0.113.7",
            "8.8.8.8",
            // The existing V4 arm does not treat the unspecified address as local, and this
            // change does not alter that — it only has to reach the same verdict either way.
            "0.0.0.0",
        ] {
            assert_eq!(
                kind(&format!("{v4}:7777")),
                kind(&mapped(v4)),
                "{v4} classified differently depending on whether it arrived native or mapped"
            );
        }
    }

    /// Unmapping must not disturb addresses that are genuinely IPv6.
    #[test]
    fn genuine_ipv6_classification_is_unchanged() {
        assert_eq!(
            kind("[::1]:7777"),
            EndpointKind::Local,
            "::1 is v6 loopback"
        );
        assert_eq!(kind("[fd00::1]:7777"), EndpointKind::Local, "ULA");
        assert_eq!(kind("[fc00::1]:7777"), EndpointKind::Local, "ULA low half");
        assert_eq!(kind("[fe80::1]:7777"), EndpointKind::Local, "link-local");
        assert_eq!(
            kind("[2001:db8::1]:7777"),
            EndpointKind::Public,
            "public v6"
        );
        assert_eq!(kind("[2606:4700::1111]:7777"), EndpointKind::Public);
    }
}
