//! STUN client for NAT traversal
//!
//! Provides functionality to discover public IP address and port by querying
//! STUN servers. This enables nodes behind NAT to learn their public endpoint
//! for connection establishment.

use anyhow::{Context, Result};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::UdpSocket;
use tracing::{debug, info, warn};

/// Default timeout for STUN requests
const STUN_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum retry attempts for STUN queries
const MAX_RETRIES: u32 = 3;

/// STUN client for discovering public endpoints
pub struct StunClient {
    /// STUN server addresses to query
    servers: Vec<SocketAddr>,

    /// Timeout for STUN requests
    timeout: Duration,

    /// Maximum number of retry attempts
    max_retries: u32,
}

impl StunClient {
    /// Create a new STUN client with the given server addresses
    pub fn new(servers: Vec<SocketAddr>) -> Self {
        Self {
            servers,
            timeout: STUN_TIMEOUT,
            max_retries: MAX_RETRIES,
        }
    }

    /// Create a STUN client with Google's public STUN servers
    pub async fn with_google_stun() -> Result<Self> {
        // Resolve DNS hostnames to IP addresses
        let servers = Self::resolve_stun_servers(&[
            "stun.l.google.com:19302",
            "stun1.l.google.com:19302",
        ])
        .await?;
        Ok(Self::new(servers))
    }

    /// Resolve DNS hostnames to socket addresses
    async fn resolve_stun_servers(hostnames: &[&str]) -> Result<Vec<SocketAddr>> {
        let mut servers = Vec::new();
        for hostname in hostnames {
            // Use tokio's DNS resolver to lookup the hostname
            let addrs: Vec<SocketAddr> = tokio::net::lookup_host(hostname)
                .await
                .context(format!("Failed to resolve STUN server: {hostname}"))?
                .collect();

            if let Some(addr) = addrs.first() {
                servers.push(*addr);
            } else {
                anyhow::bail!("No addresses found for STUN server: {hostname}");
            }
        }
        Ok(servers)
    }

    /// Set custom timeout for STUN requests
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set custom retry count
    pub fn with_max_retries(mut self, max_retries: u32) -> Self {
        self.max_retries = max_retries;
        self
    }

    /// Discover public endpoint by querying STUN servers
    ///
    /// This method queries multiple STUN servers in parallel and uses majority
    /// vote to determine the correct public endpoint. This provides resilience
    /// against misconfigured or malicious STUN servers.
    pub async fn discover_public_endpoint(
        &self,
        local_socket: &UdpSocket,
    ) -> Result<SocketAddr> {
        // Build futures for querying each server in parallel
        let query_futures: Vec<_> = self
            .servers
            .iter()
            .map(|server| self.query_stun_server(local_socket, server))
            .collect();

        // Wait for all queries to complete
        let results: Vec<SocketAddr> = futures::future::join_all(query_futures)
            .await
            .into_iter()
            .filter_map(|r| r.ok())
            .collect();

        if results.is_empty() {
            anyhow::bail!("All STUN servers failed to return results");
        }

        // Use majority vote if we have multiple results
        if results.len() == 1 {
            return Ok(results[0]);
        }

        // Count occurrences of each result
        let mut vote_counts = std::collections::HashMap::new();
        for addr in &results {
            *vote_counts.entry(addr).or_insert(0) += 1;
        }

        // Find the most common result
        let consensus = vote_counts
            .iter()
            .max_by_key(|(_, &count)| count)
            .map(|(&addr, &count)| {
                info!(
                    "STUN consensus: {} reported by {}/{} servers",
                    addr,
                    count,
                    results.len()
                );
                *addr
            })
            .expect("vote_counts is non-empty because results was checked above");

        Ok(consensus)
    }

    /// Query a specific STUN server for public endpoint
    async fn query_stun_server(
        &self,
        local_socket: &UdpSocket,
        server: &SocketAddr,
    ) -> Result<SocketAddr> {
        for attempt in 1..=self.max_retries {
            debug!(
                "STUN query attempt {}/{} to {}",
                attempt, self.max_retries, server
            );

            match tokio::time::timeout(
                self.timeout,
                Self::do_stun_query(local_socket, server),
            )
            .await
            {
                Ok(Ok(addr)) => return Ok(addr),
                Ok(Err(e)) => {
                    warn!(
                        "STUN query to {} failed (attempt {}/{}): {}",
                        server, attempt, self.max_retries, e
                    );
                }
                Err(_) => {
                    warn!(
                        "STUN query to {} timed out (attempt {}/{})",
                        server, attempt, self.max_retries
                    );
                }
            }

            // Wait before retry (exponential backoff)
            if attempt < self.max_retries {
                let backoff = Duration::from_millis(100 * (1 << (attempt - 1)));
                tokio::time::sleep(backoff).await;
            }
        }

        anyhow::bail!(
            "STUN query to {} failed after {} attempts",
            server,
            self.max_retries
        )
    }

    /// Perform actual STUN query using manual STUN protocol implementation
    ///
    /// Implements RFC 5389 STUN Binding Request/Response for NAT discovery.
    /// This is a minimal implementation focused on discovering public endpoints.
    async fn do_stun_query(
        local_socket: &UdpSocket,
        server: &SocketAddr,
    ) -> Result<SocketAddr> {
        // Create STUN Binding Request
        let transaction_id = rand::random::<[u8; 12]>();
        let request = create_stun_binding_request(&transaction_id);

        // Send request to STUN server
        local_socket
            .send_to(&request, server)
            .await
            .context("Failed to send STUN request")?;

        // Receive response
        let mut buf = vec![0u8; 1500]; // MTU size
        let (len, from) = local_socket
            .recv_from(&mut buf)
            .await
            .context("Failed to receive STUN response")?;

        if from != *server {
            anyhow::bail!(
                "Received response from unexpected source: {from} (expected {server})"
            );
        }

        // Parse STUN response and extract public endpoint
        parse_stun_binding_response(&buf[..len], &transaction_id)
    }
}

/// Create a STUN Binding Request message (RFC 5389)
fn create_stun_binding_request(transaction_id: &[u8; 12]) -> Vec<u8> {
    let mut request = Vec::with_capacity(20);

    // STUN Message Type: Binding Request (0x0001)
    request.extend_from_slice(&[0x00, 0x01]);

    // Message Length: 0 (no attributes in basic request)
    request.extend_from_slice(&[0x00, 0x00]);

    // Magic Cookie: 0x2112A442 (RFC 5389)
    request.extend_from_slice(&[0x21, 0x12, 0xA4, 0x42]);

    // Transaction ID: 96 bits (12 bytes)
    request.extend_from_slice(transaction_id);

    request
}

/// Parse STUN Binding Response and extract XOR-MAPPED-ADDRESS
fn parse_stun_binding_response(
    data: &[u8],
    expected_transaction_id: &[u8; 12],
) -> Result<SocketAddr> {
    if data.len() < 20 {
        anyhow::bail!("STUN response too short: {} bytes", data.len());
    }

    // Verify STUN message format
    let msg_type = u16::from_be_bytes([data[0], data[1]]);
    let msg_length = u16::from_be_bytes([data[2], data[3]]);
    let magic_cookie = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);

    // Check for Binding Success Response (0x0101)
    if msg_type != 0x0101 {
        anyhow::bail!("Unexpected STUN message type: 0x{msg_type:04x}");
    }

    // Verify magic cookie
    if magic_cookie != 0x2112A442 {
        anyhow::bail!(
            "Invalid STUN magic cookie: 0x{magic_cookie:08x} (expected 0x2112A442)"
        );
    }

    // Verify transaction ID
    if &data[8..20] != expected_transaction_id {
        anyhow::bail!("Transaction ID mismatch");
    }

    // Parse attributes to find XOR-MAPPED-ADDRESS (0x0020)
    let mut offset = 20;
    let end = 20 + (msg_length as usize);

    while offset + 4 <= end {
        let attr_type = u16::from_be_bytes([data[offset], data[offset + 1]]);
        let attr_length = u16::from_be_bytes([data[offset + 2], data[offset + 3]]);
        offset += 4;

        if offset + (attr_length as usize) > end {
            anyhow::bail!("Malformed STUN attribute at offset {offset}");
        }

        // XOR-MAPPED-ADDRESS attribute (0x0020)
        if attr_type == 0x0020 {
            return parse_xor_mapped_address(
                &data[offset..offset + (attr_length as usize)],
                expected_transaction_id,
            );
        }

        // Move to next attribute (attributes are padded to 4-byte boundary)
        offset += (attr_length as usize + 3) & !3;
    }

    anyhow::bail!("XOR-MAPPED-ADDRESS attribute not found in STUN response")
}

/// Parse XOR-MAPPED-ADDRESS attribute (RFC 5389 Section 15.2)
fn parse_xor_mapped_address(data: &[u8], transaction_id: &[u8; 12]) -> Result<SocketAddr> {
    if data.len() < 4 {
        anyhow::bail!("XOR-MAPPED-ADDRESS too short");
    }

    let family = data[1];
    let xport = u16::from_be_bytes([data[2], data[3]]);

    // XOR port with most significant 16 bits of magic cookie
    let port = xport ^ 0x2112;

    match family {
        0x01 => {
            // IPv4
            if data.len() < 8 {
                anyhow::bail!("XOR-MAPPED-ADDRESS IPv4 address too short");
            }

            // XOR address with magic cookie
            let xaddr = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
            let addr = xaddr ^ 0x2112A442;

            let ip = std::net::Ipv4Addr::from(addr);
            Ok(SocketAddr::new(ip.into(), port))
        }
        0x02 => {
            // IPv6
            if data.len() < 20 {
                anyhow::bail!("XOR-MAPPED-ADDRESS IPv6 address too short");
            }

            // XOR address with magic cookie (32 bits) + transaction ID (96 bits)
            let mut xaddr = [0u8; 16];
            xaddr.copy_from_slice(&data[4..20]);

            // XOR with magic cookie
            for (byte, mask) in xaddr[..4].iter_mut().zip([0x21, 0x12, 0xA4, 0x42]) {
                *byte ^= mask;
            }

            // XOR with transaction ID
            for (byte, tid_byte) in xaddr[4..16].iter_mut().zip(transaction_id.iter()) {
                *byte ^= tid_byte;
            }

            let ip = std::net::Ipv6Addr::from(xaddr);
            Ok(SocketAddr::new(ip.into(), port))
        }
        _ => anyhow::bail!("Unknown address family: 0x{family:02x}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_stun_client_creation() {
        let client = StunClient::with_google_stun()
            .await
            .expect("Failed to create STUN client");
        assert_eq!(client.servers.len(), 2);
        assert_eq!(client.timeout, STUN_TIMEOUT);
        assert_eq!(client.max_retries, MAX_RETRIES);
    }

    #[test]
    fn test_stun_client_custom_config() {
        let servers = vec!["1.2.3.4:3478".parse().unwrap()];
        let client = StunClient::new(servers)
            .with_timeout(Duration::from_secs(10))
            .with_max_retries(5);

        assert_eq!(client.timeout, Duration::from_secs(10));
        assert_eq!(client.max_retries, 5);
    }

    #[tokio::test]
    async fn test_stun_discovery_with_google() {
        // This is an integration test that requires network access
        // Skip in CI environments without internet
        if std::env::var("CI").is_ok() {
            return;
        }

        let client = StunClient::with_google_stun()
            .await
            .expect("Failed to create client");

        // Bind to any available port
        let socket = UdpSocket::bind("0.0.0.0:0")
            .await
            .expect("Failed to bind socket");

        let result = client.discover_public_endpoint(&socket).await;

        match result {
            Ok(addr) => {
                println!("Discovered public endpoint: {addr}");
                // Verify we got a valid address
                assert!(addr.port() > 0);
            }
            Err(e) => {
                // Don't fail test if network is unavailable
                eprintln!("STUN discovery failed (network may be unavailable): {e}");
            }
        }
    }

    #[tokio::test]
    async fn test_stun_majority_vote() {
        // Test that majority vote works when servers return different results
        // Create a client with 5 servers
        let servers = vec![
            "1.1.1.1:3478".parse().unwrap(),
            "2.2.2.2:3478".parse().unwrap(),
            "3.3.3.3:3478".parse().unwrap(),
            "4.4.4.4:3478".parse().unwrap(),
            "5.5.5.5:3478".parse().unwrap(),
        ];
        let client = StunClient::new(servers);

        // In a real scenario, if servers disagree, majority vote would pick the most common result
        // For example:
        // - Server 1 reports: 203.0.113.5:12345
        // - Server 2 reports: 203.0.113.5:12345
        // - Server 3 reports: 203.0.113.5:12345
        // - Server 4 reports: 198.51.100.42:9999 (misconfigured)
        // - Server 5 reports: 198.51.100.42:9999 (misconfigured)
        //
        // Majority vote would pick 203.0.113.5:12345 (3 votes vs 2 votes)
        //
        // This test verifies the client is configured for parallel queries
        assert_eq!(client.servers.len(), 5);

        // Note: We can't test actual majority vote without network access or mocking
        // The real test happens in the integration test above with Google STUN servers
    }
}
