//! Per-peer TURN relay proxy for data-plane translation.
//!
//! This module provides a local UDP relay that sits between Quinn (raw UDP)
//! and a TURN server, transparently wrapping outbound packets as
//! SEND-INDICATION messages and unwrapping inbound DATA-INDICATION messages.
//!
//! ```text
//!   Quinn (raw UDP) <-> [local_socket 127.0.0.1:ephemeral] <-> relay task <-> [turn_socket (matches TURN server AF)] <-> TURN server
//! ```
//!
//! The relay task is a `tokio::spawn`ed loop that:
//! 1. Reads outbound UDP from local_socket, wraps as SEND-INDICATION, sends to TURN server
//! 2. Reads inbound from turn_socket, if DATA-INDICATION unwraps payload, writes to local_socket

use anyhow::{bail, Context, Result};
use rand::RngCore;
use std::net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};
use std::sync::Arc;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tracing::{debug, trace, warn};

use crate::turn::TurnClient;

/// TURN magic cookie (RFC 5389).
const MAGIC_COOKIE: u32 = 0x2112_A442;

/// SEND-INDICATION message type (RFC 5766).
const SEND_INDICATION: u16 = 0x0016;

/// XOR-PEER-ADDRESS attribute type.
const XOR_PEER_ADDRESS: u16 = 0x0012;

/// DATA attribute type.
const DATA_ATTR: u16 = 0x0013;

/// Maximum UDP datagram size for the relay buffer.
///
/// Sized to hold any UDP datagram Quinn might emit, including on
/// localhost / high-MTU paths where packets can exceed 1500 bytes.
const MAX_DATAGRAM: usize = 65535;

/// Handle to a running TURN relay proxy task.
///
/// Holds the local loopback address that Quinn should connect through,
/// the TURN-side socket address, and a channel to signal shutdown.
pub struct ProxyHandle {
    local_addr: SocketAddr,
    turn_side_addr: SocketAddr,
    shutdown_tx: mpsc::Sender<()>,
    task_handle: JoinHandle<()>,
}

impl ProxyHandle {
    /// Returns the local loopback address Quinn should bind/send to.
    pub fn local_addr(&self) -> SocketAddr {
        self.local_addr
    }

    /// Returns the TURN-facing socket address (useful for diagnostics/testing).
    pub fn turn_side_addr(&self) -> SocketAddr {
        self.turn_side_addr
    }

    /// Signal the relay task to stop and wait for it to finish.
    pub async fn shutdown(self) -> Result<()> {
        // Send shutdown signal; ignore error if receiver already dropped.
        let _ = self.shutdown_tx.send(()).await;
        self.task_handle
            .await
            .map_err(|e| anyhow::anyhow!("relay task panicked: {e}"))
    }

    /// Test-only constructor.
    #[cfg(test)]
    fn new_test(local_addr: SocketAddr) -> Self {
        let (tx, _rx) = mpsc::channel(1);
        Self {
            local_addr,
            turn_side_addr: local_addr,
            shutdown_tx: tx,
            task_handle: tokio::spawn(async {}),
        }
    }
}

/// Factory for starting per-peer TURN relay proxy tasks.
pub struct TurnRelayProxy;

impl TurnRelayProxy {
    /// Start a relay proxy for a single peer.
    ///
    /// * `turn_server_addr` - address of the TURN server
    /// * `peer_relay_addr` - the peer's relay address (XOR-PEER-ADDRESS target)
    /// * `turn_client` - shared TurnClient used to parse inbound DATA-INDICATION
    ///
    /// Returns a [`ProxyHandle`] whose `local_addr()` should be given to Quinn.
    pub async fn start(
        turn_server_addr: SocketAddr,
        peer_relay_addr: SocketAddr,
        turn_client: Arc<TurnClient>,
    ) -> Result<ProxyHandle> {
        // Bind local loopback socket (Quinn side) always to IPv4 loopback.
        // The Quinn relay endpoint in actor/messages.rs binds to 127.0.0.1:0 and
        // cannot send to an IPv6 destination, so this side must stay IPv4 regardless
        // of the peer's relay address family.
        let local_socket =
            UdpSocket::bind(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)))
                .await
                .context("failed to bind local relay socket")?;
        let local_addr = local_socket
            .local_addr()
            .context("failed to get local relay address")?;

        // Bind TURN-side socket matching the TURN server's address family — not the
        // peer relay address. The TURN socket communicates with turn_server_addr, so
        // a mismatch (e.g. IPv4 TURN server + IPv6 peer relay) would prevent the
        // socket from reaching the server.
        let turn_socket = if turn_server_addr.is_ipv6() {
            UdpSocket::bind(SocketAddr::V6(SocketAddrV6::new(
                Ipv6Addr::UNSPECIFIED,
                0,
                0,
                0,
            )))
            .await
            .context("failed to bind TURN-side relay socket (IPv6)")?
        } else {
            UdpSocket::bind(SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::UNSPECIFIED, 0)))
                .await
                .context("failed to bind TURN-side relay socket (IPv4)")?
        };
        let turn_side_addr = turn_socket
            .local_addr()
            .context("failed to get TURN-side relay address")?;

        let (shutdown_tx, shutdown_rx) = mpsc::channel::<()>(1);

        let task_handle = tokio::spawn(relay_loop(
            local_socket,
            turn_socket,
            turn_server_addr,
            peer_relay_addr,
            turn_client,
            shutdown_rx,
        ));

        debug!(
            %local_addr,
            %turn_side_addr,
            %turn_server_addr,
            %peer_relay_addr,
            "TURN relay proxy started"
        );

        Ok(ProxyHandle {
            local_addr,
            turn_side_addr,
            shutdown_tx,
            task_handle,
        })
    }

    /// Test-only variant that does not require a real TurnClient.
    ///
    /// Uses a default TurnClient internally; useful for unit tests where
    /// you control both sides of the UDP sockets.
    #[cfg(test)]
    pub async fn start_test(
        turn_server_addr: SocketAddr,
        peer_relay_addr: SocketAddr,
    ) -> Result<ProxyHandle> {
        let config = crate::turn::TurnConfig::new(turn_server_addr);
        let client = Arc::new(TurnClient::new(config));
        Self::start(turn_server_addr, peer_relay_addr, client).await
    }
}

/// Core relay loop: bidirectional translation between raw UDP and TURN framing.
async fn relay_loop(
    local_socket: UdpSocket,
    turn_socket: UdpSocket,
    turn_server_addr: SocketAddr,
    peer_relay_addr: SocketAddr,
    turn_client: Arc<TurnClient>,
    mut shutdown_rx: mpsc::Receiver<()>,
) {
    let mut local_buf = [0u8; MAX_DATAGRAM];
    let mut turn_buf = [0u8; MAX_DATAGRAM];

    // We need to remember the Quinn-side sender so we can route inbound
    // DATA-INDICATION payloads back.  Initially unknown; set on first
    // outbound packet from Quinn.
    let mut quinn_sender: Option<SocketAddr> = None;

    loop {
        tokio::select! {
            // Shutdown signal
            _ = shutdown_rx.recv() => {
                debug!("relay proxy shutting down");
                break;
            }

            // Outbound: Quinn -> local_socket -> wrap -> turn_socket -> TURN server
            result = local_socket.recv_from(&mut local_buf) => {
                match result {
                    Ok((len, from)) => {
                        quinn_sender = Some(from);
                        match build_send_indication(peer_relay_addr, &local_buf[..len]) {
                            Ok(indication) => {
                                if let Err(e) = turn_socket.send_to(&indication, turn_server_addr).await {
                                    warn!(error = %e, "failed to send TURN indication");
                                }
                                trace!(bytes = len, "relayed outbound via SEND-INDICATION");
                            }
                            Err(e) => {
                                warn!(error = %e, "failed to build SEND-INDICATION");
                            }
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "local_socket recv error");
                    }
                }
            }

            // Inbound: TURN server -> turn_socket -> unwrap DATA-INDICATION -> local_socket -> Quinn
            result = turn_socket.recv_from(&mut turn_buf) => {
                match result {
                    Ok((len, _from)) => {
                        let packet = &turn_buf[..len];
                        if TurnClient::is_data_indication(packet) {
                            match turn_client.parse_data_indication(packet) {
                                Ok((_peer_addr, payload)) => {
                                    if let Some(quinn_addr) = quinn_sender {
                                        if let Err(e) = local_socket.send_to(&payload, quinn_addr).await {
                                            warn!(error = %e, "failed to send to Quinn");
                                        }
                                        trace!(bytes = payload.len(), "relayed inbound DATA-INDICATION");
                                    } else {
                                        warn!("received DATA-INDICATION but no Quinn sender known yet");
                                    }
                                }
                                Err(e) => {
                                    warn!(error = %e, "failed to parse DATA-INDICATION");
                                }
                            }
                        } else {
                            trace!(len, "ignoring non-DATA-INDICATION on TURN socket");
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "turn_socket recv error");
                    }
                }
            }
        }
    }
}

// =============================================================================
// Standalone TURN framing helpers
// =============================================================================

/// Build a TURN SEND-INDICATION message (RFC 5766 section 10.1).
///
/// The message contains:
/// - 20-byte STUN header (type + length + magic cookie + transaction ID)
/// - XOR-PEER-ADDRESS attribute (the peer to relay to)
/// - DATA attribute (the actual payload)
///
/// Both IPv4 and IPv6 peer addresses are supported.
pub fn build_send_indication(peer_addr: SocketAddr, data: &[u8]) -> Result<Vec<u8>> {
    // Generate random transaction ID
    let mut transaction_id = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut transaction_id);

    // Estimate capacity: 20 header + xor-peer-address attr (8 v4 / 20 v6) + data attr
    let xor_attr_val_len = if peer_addr.is_ipv6() { 20 } else { 8 };
    let mut msg = Vec::with_capacity(20 + 4 + xor_attr_val_len + 4 + data.len() + 4);

    // Message type: SEND-INDICATION
    msg.extend_from_slice(&SEND_INDICATION.to_be_bytes());

    // Message length placeholder
    let len_pos = msg.len();
    msg.extend_from_slice(&[0, 0]);

    // Magic cookie
    msg.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());

    // Transaction ID
    msg.extend_from_slice(&transaction_id);

    // XOR-PEER-ADDRESS attribute (dispatch on address family)
    let xor_addr = if peer_addr.is_ipv6() {
        xor_encode_address_v6(peer_addr, &transaction_id)?
    } else {
        xor_encode_address_v4(peer_addr, &transaction_id)?
    };
    msg.extend_from_slice(&XOR_PEER_ADDRESS.to_be_bytes());
    msg.extend_from_slice(&(xor_addr.len() as u16).to_be_bytes());
    msg.extend_from_slice(&xor_addr);
    // Pad to 4-byte boundary
    while msg.len() % 4 != 0 {
        msg.push(0);
    }

    // DATA attribute
    msg.extend_from_slice(&DATA_ATTR.to_be_bytes());
    msg.extend_from_slice(&(data.len() as u16).to_be_bytes());
    msg.extend_from_slice(data);
    // Pad to 4-byte boundary
    while msg.len() % 4 != 0 {
        msg.push(0);
    }

    // Fill in message length (total length minus 20-byte header)
    let attr_len = (msg.len() - 20) as u16;
    msg[len_pos..len_pos + 2].copy_from_slice(&attr_len.to_be_bytes());

    Ok(msg)
}

/// XOR-encode an IPv4 socket address per RFC 5389 section 15.2.
///
/// Produces the attribute value bytes (without the attribute type/length header):
/// - 1 byte reserved (0x00)
/// - 1 byte family (0x01 for IPv4)
/// - 2 bytes XOR'd port
/// - 4 bytes XOR'd IPv4 address
pub fn xor_encode_address_v4(addr: SocketAddr, transaction_id: &[u8; 12]) -> Result<Vec<u8>> {
    let _ = transaction_id; // Not used for IPv4 XOR, but kept for API consistency with v6

    match addr {
        SocketAddr::V4(v4) => {
            let mut result = Vec::with_capacity(8);
            result.push(0x00); // Reserved
            result.push(0x01); // IPv4 family

            let port = v4.port() ^ ((MAGIC_COOKIE >> 16) as u16);
            result.extend_from_slice(&port.to_be_bytes());

            let ip_bytes = v4.ip().octets();
            let cookie_bytes = MAGIC_COOKIE.to_be_bytes();
            for i in 0..4 {
                result.push(ip_bytes[i] ^ cookie_bytes[i]);
            }

            Ok(result)
        }
        SocketAddr::V6(_) => {
            bail!("xor_encode_address_v4 called with IPv6 address; use xor_encode_address_v6");
        }
    }
}

/// XOR-encode an IPv6 socket address per RFC 5766 section 10.2.
///
/// Produces the attribute value bytes (without the attribute type/length header):
/// - 1 byte reserved (0x00)
/// - 1 byte family (0x02 for IPv6)
/// - 2 bytes XOR'd port (XOR with high 16 bits of magic cookie)
/// - 4 bytes XOR'd with magic cookie (first 4 bytes of IPv6 address)
/// - 12 bytes XOR'd with transaction ID (remaining 12 bytes of IPv6 address)
pub fn xor_encode_address_v6(addr: SocketAddr, transaction_id: &[u8; 12]) -> Result<Vec<u8>> {
    match addr {
        SocketAddr::V6(v6) => {
            let mut result = Vec::with_capacity(20);
            result.push(0x00); // Reserved
            result.push(0x02); // IPv6 family

            let port = v6.port() ^ ((MAGIC_COOKIE >> 16) as u16);
            result.extend_from_slice(&port.to_be_bytes());

            let ip_bytes = v6.ip().octets();
            let cookie_bytes = MAGIC_COOKIE.to_be_bytes();
            // First 4 bytes XOR'd with magic cookie
            for i in 0..4 {
                result.push(ip_bytes[i] ^ cookie_bytes[i]);
            }
            // Remaining 12 bytes XOR'd with transaction ID
            for i in 0..12 {
                result.push(ip_bytes[4 + i] ^ transaction_id[i]);
            }

            Ok(result)
        }
        SocketAddr::V4(_) => {
            bail!("xor_encode_address_v6 called with IPv4 address; use xor_encode_address_v4");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    /// Build a DATA-INDICATION message for testing inbound relay.
    ///
    /// This mirrors the framing from `TurnClient::build_send_indication` but
    /// with message type DATA-INDICATION (0x0017).
    fn build_test_data_indication(peer_addr: SocketAddr, payload: &[u8]) -> Vec<u8> {
        const DATA_INDICATION: u16 = 0x0017;

        let mut transaction_id = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut transaction_id);

        let mut msg = Vec::with_capacity(20 + 12 + 4 + payload.len() + 4);

        // Header
        msg.extend_from_slice(&DATA_INDICATION.to_be_bytes());
        let len_pos = msg.len();
        msg.extend_from_slice(&[0, 0]); // length placeholder
        msg.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
        msg.extend_from_slice(&transaction_id);

        // XOR-PEER-ADDRESS
        let xor_addr = xor_encode_address_v4(peer_addr, &transaction_id).unwrap();
        msg.extend_from_slice(&XOR_PEER_ADDRESS.to_be_bytes());
        msg.extend_from_slice(&(xor_addr.len() as u16).to_be_bytes());
        msg.extend_from_slice(&xor_addr);
        while msg.len() % 4 != 0 {
            msg.push(0);
        }

        // DATA
        msg.extend_from_slice(&DATA_ATTR.to_be_bytes());
        msg.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        msg.extend_from_slice(payload);
        while msg.len() % 4 != 0 {
            msg.push(0);
        }

        // Fill length
        let attr_len = (msg.len() - 20) as u16;
        msg[len_pos..len_pos + 2].copy_from_slice(&attr_len.to_be_bytes());

        msg
    }

    #[tokio::test]
    async fn test_proxy_handle_has_local_addr() {
        let addr: SocketAddr = "127.0.0.1:12345".parse().unwrap();
        let handle = ProxyHandle::new_test(addr);
        assert_eq!(handle.local_addr(), addr);
    }

    #[tokio::test]
    async fn test_proxy_outbound_wraps_as_send_indication() {
        // Set up a fake "TURN server" socket to receive the indication.
        let fake_turn_server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let fake_turn_addr = fake_turn_server.local_addr().unwrap();

        let peer_relay_addr: SocketAddr = "10.0.0.1:5000".parse().unwrap();

        let proxy = TurnRelayProxy::start_test(fake_turn_addr, peer_relay_addr)
            .await
            .unwrap();

        // Send raw UDP through the proxy's local address (simulating Quinn).
        let quinn_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let test_payload = b"hello from quinn";
        quinn_socket
            .send_to(test_payload, proxy.local_addr())
            .await
            .unwrap();

        // Read from the fake TURN server and verify it is a SEND-INDICATION.
        let mut buf = [0u8; MAX_DATAGRAM];
        let (len, _from) =
            tokio::time::timeout(Duration::from_secs(2), fake_turn_server.recv_from(&mut buf))
                .await
                .expect("timed out waiting for SEND-INDICATION")
                .unwrap();

        let packet = &buf[..len];

        // Verify SEND-INDICATION message type (0x0016)
        let msg_type = u16::from_be_bytes([packet[0], packet[1]]);
        assert_eq!(
            msg_type, SEND_INDICATION,
            "expected SEND-INDICATION (0x0016)"
        );

        // Verify magic cookie
        let cookie = u32::from_be_bytes([packet[4], packet[5], packet[6], packet[7]]);
        assert_eq!(cookie, MAGIC_COOKIE);

        // Verify the DATA attribute contains our payload.
        // Walk attributes to find DATA (0x0013).
        let msg_len = u16::from_be_bytes([packet[2], packet[3]]) as usize;
        let mut pos = 20;
        let mut found_data = false;
        while pos + 4 <= 20 + msg_len && pos + 4 <= len {
            let attr_type = u16::from_be_bytes([packet[pos], packet[pos + 1]]);
            let attr_len = u16::from_be_bytes([packet[pos + 2], packet[pos + 3]]) as usize;
            if attr_type == DATA_ATTR {
                let data = &packet[pos + 4..pos + 4 + attr_len];
                assert_eq!(data, test_payload);
                found_data = true;
                break;
            }
            pos += 4 + ((attr_len + 3) & !3);
        }
        assert!(found_data, "DATA attribute not found in SEND-INDICATION");

        proxy.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn test_proxy_inbound_unwraps_data_indication() {
        // Set up a fake "TURN server" socket.
        let fake_turn_server = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let fake_turn_addr = fake_turn_server.local_addr().unwrap();

        let peer_relay_addr: SocketAddr = "10.0.0.2:6000".parse().unwrap();

        let proxy = TurnRelayProxy::start_test(fake_turn_addr, peer_relay_addr)
            .await
            .unwrap();

        // Quinn-side socket that will receive the unwrapped payload.
        let quinn_socket = UdpSocket::bind("127.0.0.1:0").await.unwrap();
        let _quinn_addr = quinn_socket.local_addr().unwrap();

        // First, send a packet from Quinn through the proxy so the relay
        // learns quinn_sender. This also sets up the outbound path.
        quinn_socket
            .send_to(b"init", proxy.local_addr())
            .await
            .unwrap();

        // Wait for the init packet to arrive at the fake TURN server,
        // confirming the relay has processed it and recorded quinn_sender.
        let mut discard = [0u8; MAX_DATAGRAM];
        tokio::time::timeout(
            Duration::from_secs(2),
            fake_turn_server.recv_from(&mut discard),
        )
        .await
        .expect("timed out waiting for init packet")
        .unwrap();

        // Now send a crafted DATA-INDICATION from the fake TURN server
        // to the proxy's TURN-side socket.
        let test_payload = b"hello from peer via TURN";
        let data_ind = build_test_data_indication(peer_relay_addr, test_payload);
        fake_turn_server
            .send_to(&data_ind, proxy.turn_side_addr())
            .await
            .unwrap();

        // Read from Quinn's socket and verify we got the raw payload.
        let mut buf = [0u8; MAX_DATAGRAM];
        let (len, _from) =
            tokio::time::timeout(Duration::from_secs(2), quinn_socket.recv_from(&mut buf))
                .await
                .expect("timed out waiting for unwrapped payload")
                .unwrap();

        assert_eq!(&buf[..len], test_payload);

        proxy.shutdown().await.unwrap();
    }

    #[test]
    fn test_build_send_indication_format() {
        let peer_addr: SocketAddr = "10.0.0.1:5000".parse().unwrap();
        let data = b"test payload";

        let msg = build_send_indication(peer_addr, data).unwrap();

        // Minimum: 20 header + 12 xor-peer-addr attr (4 hdr + 8 val) + 4+12 data attr = 48
        assert!(msg.len() >= 44, "message too short: {}", msg.len());

        // Verify header
        let msg_type = u16::from_be_bytes([msg[0], msg[1]]);
        assert_eq!(msg_type, SEND_INDICATION);

        let cookie = u32::from_be_bytes([msg[4], msg[5], msg[6], msg[7]]);
        assert_eq!(cookie, MAGIC_COOKIE);

        // Verify message length field matches actual attribute payload
        let msg_len = u16::from_be_bytes([msg[2], msg[3]]) as usize;
        assert_eq!(msg_len, msg.len() - 20);

        // Verify total length is 4-byte aligned
        assert_eq!(msg.len() % 4, 0, "message not 4-byte aligned");

        // Walk attributes and find XOR-PEER-ADDRESS and DATA
        let mut pos = 20;
        let mut found_peer = false;
        let mut found_data = false;
        while pos + 4 <= msg.len() {
            let attr_type = u16::from_be_bytes([msg[pos], msg[pos + 1]]);
            let attr_len = u16::from_be_bytes([msg[pos + 2], msg[pos + 3]]) as usize;
            match attr_type {
                XOR_PEER_ADDRESS => {
                    assert_eq!(attr_len, 8, "IPv4 XOR-PEER-ADDRESS should be 8 bytes");
                    found_peer = true;
                }
                DATA_ATTR => {
                    assert_eq!(attr_len, data.len());
                    assert_eq!(&msg[pos + 4..pos + 4 + attr_len], data);
                    found_data = true;
                }
                _ => {}
            }
            pos += 4 + ((attr_len + 3) & !3);
        }
        assert!(found_peer, "XOR-PEER-ADDRESS attribute missing");
        assert!(found_data, "DATA attribute missing");
    }

    #[test]
    fn test_xor_encode_address_v4_roundtrip() {
        let addr: SocketAddr = "192.168.1.100:8080".parse().unwrap();
        let txn_id = [0u8; 12];

        let encoded = xor_encode_address_v4(addr, &txn_id).unwrap();
        assert_eq!(encoded.len(), 8);
        assert_eq!(encoded[0], 0x00); // reserved
        assert_eq!(encoded[1], 0x01); // IPv4 family

        // Decode manually to verify roundtrip
        let cookie_bytes = MAGIC_COOKIE.to_be_bytes();
        let port = u16::from_be_bytes([encoded[2], encoded[3]]) ^ ((MAGIC_COOKIE >> 16) as u16);
        let ip = std::net::Ipv4Addr::new(
            encoded[4] ^ cookie_bytes[0],
            encoded[5] ^ cookie_bytes[1],
            encoded[6] ^ cookie_bytes[2],
            encoded[7] ^ cookie_bytes[3],
        );
        let decoded = SocketAddr::new(std::net::IpAddr::V4(ip), port);
        assert_eq!(decoded, addr);
    }

    #[test]
    fn test_xor_encode_address_v4_rejects_ipv6() {
        let addr: SocketAddr = "[::1]:8080".parse().unwrap();
        let txn_id = [0u8; 12];
        // v4 encoder must reject IPv6 input
        assert!(xor_encode_address_v4(addr, &txn_id).is_err());
    }

    #[test]
    fn test_xor_encode_address_v6_roundtrip() {
        // Build a known IPv6 address and verify XOR encoding + manual decode round-trips.
        let ip: std::net::Ipv6Addr = "2001:db8::1".parse().unwrap();
        let addr = SocketAddr::new(std::net::IpAddr::V6(ip), 9876);
        let txn_id: [u8; 12] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12];

        let encoded = xor_encode_address_v6(addr, &txn_id).unwrap();
        assert_eq!(encoded.len(), 20);
        assert_eq!(encoded[0], 0x00); // reserved
        assert_eq!(encoded[1], 0x02); // IPv6 family

        // Decode port
        let decoded_port =
            u16::from_be_bytes([encoded[2], encoded[3]]) ^ ((MAGIC_COOKIE >> 16) as u16);
        assert_eq!(decoded_port, 9876);

        // Decode IP
        let ip_bytes = ip.octets();
        let cookie_bytes = MAGIC_COOKIE.to_be_bytes();
        for i in 0..4 {
            assert_eq!(encoded[4 + i], ip_bytes[i] ^ cookie_bytes[i]);
        }
        for i in 0..12 {
            assert_eq!(encoded[8 + i], ip_bytes[4 + i] ^ txn_id[i]);
        }
    }

    #[test]
    fn test_xor_encode_address_v6_rejects_ipv4() {
        let addr: SocketAddr = "192.168.1.1:8080".parse().unwrap();
        let txn_id = [0u8; 12];
        assert!(xor_encode_address_v6(addr, &txn_id).is_err());
    }

    #[test]
    fn test_build_send_indication_accepts_ipv6() {
        let addr: SocketAddr = "[2001:db8::1]:5000".parse().unwrap();
        let data = b"test payload v6";
        let msg = build_send_indication(addr, data).unwrap();

        // Verify SEND-INDICATION header
        let msg_type = u16::from_be_bytes([msg[0], msg[1]]);
        assert_eq!(msg_type, SEND_INDICATION);
        let cookie = u32::from_be_bytes([msg[4], msg[5], msg[6], msg[7]]);
        assert_eq!(cookie, MAGIC_COOKIE);

        // Verify total length is 4-byte aligned
        assert_eq!(msg.len() % 4, 0);

        // Walk attributes and find XOR-PEER-ADDRESS (should be 20 bytes for IPv6)
        let msg_len = u16::from_be_bytes([msg[2], msg[3]]) as usize;
        let mut pos = 20;
        let mut found_peer = false;
        let mut found_data = false;
        while pos + 4 <= 20 + msg_len && pos + 4 <= msg.len() {
            let attr_type = u16::from_be_bytes([msg[pos], msg[pos + 1]]);
            let attr_len = u16::from_be_bytes([msg[pos + 2], msg[pos + 3]]) as usize;
            match attr_type {
                XOR_PEER_ADDRESS => {
                    assert_eq!(attr_len, 20, "IPv6 XOR-PEER-ADDRESS should be 20 bytes");
                    assert_eq!(msg[pos + 5], 0x02, "family byte should be 0x02 for IPv6");
                    found_peer = true;
                }
                DATA_ATTR => {
                    assert_eq!(&msg[pos + 4..pos + 4 + attr_len], data);
                    found_data = true;
                }
                _ => {}
            }
            pos += 4 + ((attr_len + 3) & !3);
        }
        assert!(found_peer, "XOR-PEER-ADDRESS attribute missing");
        assert!(found_data, "DATA attribute missing");
    }

    #[tokio::test]
    async fn test_proxy_start_with_ipv6_peer() {
        // Verify that start() no longer rejects IPv6 peer addresses.
        // Use a fake TURN server on IPv6 loopback.
        let fake_turn = UdpSocket::bind("[::1]:0").await;
        if fake_turn.is_err() {
            // IPv6 loopback not available in this environment — skip.
            return;
        }
        let fake_turn = fake_turn.unwrap();
        let fake_turn_addr = fake_turn.local_addr().unwrap();

        let peer_v6: SocketAddr = "[::1]:9000".parse().unwrap();
        let result = TurnRelayProxy::start_test(fake_turn_addr, peer_v6).await;
        assert!(
            result.is_ok(),
            "start() should accept IPv6 peer: {:?}",
            result.err()
        );
        result.unwrap().shutdown().await.unwrap();
    }
}
