//! TURN relay client for NAT traversal (Phase 4 - M1)
//!
//! TURN (Traversal Using Relays around NAT) provides relay-based connectivity
//! when direct connections via STUN fail (e.g., symmetric NAT). This is the
//! fallback mechanism when hole-punching is not possible.
//!
//! RFC 5766: https://tools.ietf.org/html/rfc5766
//!
//! Note: This is a simplified implementation that focuses on the core functionality
//! needed for ICN peer connectivity. Production use may require additional features
//! like long-term credentials, TLS-over-TURN, etc.

use anyhow::{Context, Result};
use rand::RngCore;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::net::UdpSocket;
use tokio::sync::RwLock;
use tracing::{debug, info};

/// Default timeout for TURN requests
const TURN_TIMEOUT: Duration = Duration::from_secs(10);

/// Default allocation lifetime (10 minutes per RFC 5766)
const DEFAULT_ALLOCATION_LIFETIME: Duration = Duration::from_secs(600);

/// Refresh margin - refresh allocation before expiry
const ALLOCATION_REFRESH_MARGIN: Duration = Duration::from_secs(60);

/// Permission lifetime (5 minutes per RFC 5766)
const PERMISSION_LIFETIME: Duration = Duration::from_secs(300);

/// TURN message types (from RFC 5766)
#[allow(dead_code)]
mod message_type {
    pub const ALLOCATE_REQUEST: u16 = 0x0003;
    pub const ALLOCATE_RESPONSE: u16 = 0x0103;
    pub const ALLOCATE_ERROR: u16 = 0x0113;
    pub const REFRESH_REQUEST: u16 = 0x0004;
    pub const REFRESH_RESPONSE: u16 = 0x0104;
    pub const CREATE_PERMISSION_REQUEST: u16 = 0x0008;
    pub const CREATE_PERMISSION_RESPONSE: u16 = 0x0108;
    pub const SEND_INDICATION: u16 = 0x0016;
    pub const DATA_INDICATION: u16 = 0x0017;
    pub const CHANNEL_BIND_REQUEST: u16 = 0x0009;
    pub const CHANNEL_BIND_RESPONSE: u16 = 0x0109;
}

/// TURN attribute types (from RFC 5766)
#[allow(dead_code)]
mod attribute_type {
    pub const XOR_RELAYED_ADDRESS: u16 = 0x0016;
    pub const XOR_MAPPED_ADDRESS: u16 = 0x0020;
    pub const LIFETIME: u16 = 0x000D;
    pub const XOR_PEER_ADDRESS: u16 = 0x0012;
    pub const DATA: u16 = 0x0013;
    pub const REQUESTED_TRANSPORT: u16 = 0x0019;
    pub const ERROR_CODE: u16 = 0x0009;
}

/// TURN magic cookie (same as STUN)
const MAGIC_COOKIE: u32 = 0x2112A442;

/// Information about an active TURN allocation
#[derive(Debug, Clone)]
pub struct TurnAllocation {
    /// The relay address allocated by the TURN server
    pub relay_addr: SocketAddr,

    /// The mapped address (our public endpoint)
    pub mapped_addr: SocketAddr,

    /// When this allocation was created
    pub created_at: Instant,

    /// Lifetime of the allocation
    pub lifetime: Duration,

    /// Transaction ID for this allocation
    pub transaction_id: [u8; 12],
}

impl TurnAllocation {
    /// Check if this allocation is expired
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() > self.lifetime
    }

    /// Check if this allocation needs refresh
    pub fn needs_refresh(&self) -> bool {
        self.created_at.elapsed() > (self.lifetime - ALLOCATION_REFRESH_MARGIN)
    }
}

/// Permission entry for a peer
#[derive(Debug, Clone)]
pub struct TurnPermission {
    /// Peer address that has permission
    pub peer_addr: SocketAddr,

    /// When this permission was created
    pub created_at: Instant,
}

impl TurnPermission {
    /// Check if this permission is expired
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() > PERMISSION_LIFETIME
    }
}

/// Configuration for TURN client
#[derive(Debug, Clone)]
pub struct TurnConfig {
    /// TURN server address
    pub server: SocketAddr,

    /// Username for authentication (optional)
    pub username: Option<String>,

    /// Password for authentication (optional)
    pub password: Option<String>,

    /// Request timeout
    pub timeout: Duration,

    /// Allocation lifetime to request
    pub allocation_lifetime: Duration,
}

impl TurnConfig {
    /// Create a new TURN config with the given server address
    pub fn new(server: SocketAddr) -> Self {
        Self {
            server,
            username: None,
            password: None,
            timeout: TURN_TIMEOUT,
            allocation_lifetime: DEFAULT_ALLOCATION_LIFETIME,
        }
    }

    /// Set the username for authentication
    pub fn with_username(mut self, username: Option<String>) -> Self {
        self.username = username;
        self
    }

    /// Set the password for authentication
    pub fn with_password(mut self, password: Option<String>) -> Self {
        self.password = password;
        self
    }

    /// Set the request timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set the allocation lifetime
    pub fn with_allocation_lifetime(mut self, lifetime: Duration) -> Self {
        self.allocation_lifetime = lifetime;
        self
    }
}

impl Default for TurnConfig {
    fn default() -> Self {
        // SAFETY: "0.0.0.0:3478" is a valid socket address literal
        #[allow(clippy::unwrap_used)]
        let server = "0.0.0.0:3478".parse().unwrap();
        Self {
            // Default to no TURN server - must be configured
            server,
            username: None,
            password: None,
            timeout: TURN_TIMEOUT,
            allocation_lifetime: DEFAULT_ALLOCATION_LIFETIME,
        }
    }
}

/// TURN client for relay-based NAT traversal
pub struct TurnClient {
    /// Configuration
    config: TurnConfig,

    /// Current allocation (if any)
    allocation: Arc<RwLock<Option<TurnAllocation>>>,

    /// Active permissions
    permissions: Arc<RwLock<HashMap<SocketAddr, TurnPermission>>>,
}

impl TurnClient {
    /// Create a new TURN client with the given configuration
    pub fn new(config: TurnConfig) -> Self {
        Self {
            config,
            allocation: Arc::new(RwLock::new(None)),
            permissions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Create a TURN client for a specific server
    pub fn with_server(server: SocketAddr) -> Self {
        Self::new(TurnConfig {
            server,
            ..Default::default()
        })
    }

    /// Allocate a relay address from the TURN server
    ///
    /// Returns the allocated relay address that can be shared with peers
    pub async fn allocate(&self, local_socket: &UdpSocket) -> Result<TurnAllocation> {
        info!(
            server = %self.config.server,
            "Requesting TURN allocation"
        );

        // Generate transaction ID
        let mut transaction_id = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut transaction_id);

        // Build Allocate request
        let request = self.build_allocate_request(&transaction_id)?;

        // Send request
        local_socket
            .send_to(&request, self.config.server)
            .await
            .context("Failed to send TURN allocate request")?;

        // Wait for response
        let mut response_buf = [0u8; 1500];
        let response = tokio::time::timeout(
            self.config.timeout,
            local_socket.recv_from(&mut response_buf),
        )
        .await
        .context("TURN allocate request timed out")?
        .context("Failed to receive TURN response")?;

        let (len, from) = response;
        if from != self.config.server {
            anyhow::bail!("Response from unexpected source: {from}");
        }

        // Parse response
        let allocation = self.parse_allocate_response(&response_buf[..len], &transaction_id)?;

        // Store allocation
        *self.allocation.write().await = Some(allocation.clone());

        icn_obs::metrics::nat::turn_allocation_inc();
        info!(
            relay_addr = %allocation.relay_addr,
            mapped_addr = %allocation.mapped_addr,
            lifetime_secs = allocation.lifetime.as_secs(),
            "TURN allocation successful"
        );

        Ok(allocation)
    }

    /// Refresh an existing allocation to extend its lifetime
    pub async fn refresh(&self, local_socket: &UdpSocket) -> Result<()> {
        let allocation = self.allocation.read().await.clone();
        let allocation = allocation.context("No active allocation to refresh")?;

        debug!(
            relay_addr = %allocation.relay_addr,
            "Refreshing TURN allocation"
        );

        // Generate transaction ID
        let mut transaction_id = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut transaction_id);

        // Build Refresh request
        let request = self.build_refresh_request(&transaction_id)?;

        // Send request
        local_socket
            .send_to(&request, self.config.server)
            .await
            .context("Failed to send TURN refresh request")?;

        // Wait for response
        let mut response_buf = [0u8; 1500];
        let response = tokio::time::timeout(
            self.config.timeout,
            local_socket.recv_from(&mut response_buf),
        )
        .await
        .context("TURN refresh request timed out")?
        .context("Failed to receive TURN response")?;

        let (len, _from) = response;

        // Parse response (just verify success)
        self.verify_refresh_response(&response_buf[..len], &transaction_id)?;

        // Update allocation timestamp
        if let Some(ref mut alloc) = *self.allocation.write().await {
            // Create updated allocation with reset created_at
            let updated = TurnAllocation {
                created_at: Instant::now(),
                ..alloc.clone()
            };
            *alloc = updated;
        }

        icn_obs::metrics::nat::turn_permission_refresh_inc();
        debug!("TURN allocation refreshed");
        Ok(())
    }

    /// Create a permission for a peer address
    ///
    /// Permissions are required before the TURN server will relay data to/from a peer
    pub async fn create_permission(
        &self,
        local_socket: &UdpSocket,
        peer_addr: SocketAddr,
    ) -> Result<()> {
        if self.allocation.read().await.is_none() {
            anyhow::bail!("No active allocation - call allocate() first");
        }

        debug!(
            peer = %peer_addr,
            "Creating TURN permission"
        );

        // Generate transaction ID
        let mut transaction_id = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut transaction_id);

        // Build CreatePermission request
        let request = self.build_permission_request(&transaction_id, peer_addr)?;

        // Send request
        local_socket
            .send_to(&request, self.config.server)
            .await
            .context("Failed to send TURN permission request")?;

        // Wait for response
        let mut response_buf = [0u8; 1500];
        let response = tokio::time::timeout(
            self.config.timeout,
            local_socket.recv_from(&mut response_buf),
        )
        .await
        .context("TURN permission request timed out")?
        .context("Failed to receive TURN response")?;

        let (len, _from) = response;

        // Parse response (just verify success)
        self.verify_permission_response(&response_buf[..len], &transaction_id)?;

        // Store permission
        self.permissions.write().await.insert(
            peer_addr,
            TurnPermission {
                peer_addr,
                created_at: Instant::now(),
            },
        );

        debug!(peer = %peer_addr, "TURN permission created");
        Ok(())
    }

    /// Get the current relay address (if allocated)
    pub async fn relay_addr(&self) -> Option<SocketAddr> {
        self.allocation.read().await.as_ref().map(|a| a.relay_addr)
    }

    /// Check if we have an active, non-expired allocation
    pub async fn has_valid_allocation(&self) -> bool {
        match self.allocation.read().await.as_ref() {
            Some(alloc) => !alloc.is_expired(),
            None => false,
        }
    }

    /// Check if allocation needs refresh
    pub async fn needs_refresh(&self) -> bool {
        match self.allocation.read().await.as_ref() {
            Some(alloc) => alloc.needs_refresh(),
            None => false,
        }
    }

    /// Send data to a peer through the TURN relay
    ///
    /// This sends a SEND-INDICATION message to the TURN server, which will
    /// then forward the data to the specified peer address. A permission must
    /// already exist for the peer address.
    ///
    /// Note: SEND-INDICATION is an indication (not a request), so there is no
    /// response from the server. Delivery is not guaranteed.
    pub async fn send_indication(
        &self,
        local_socket: &UdpSocket,
        peer_addr: SocketAddr,
        data: &[u8],
    ) -> Result<()> {
        if self.allocation.read().await.is_none() {
            anyhow::bail!("No active allocation - call allocate() first");
        }

        // Check if we have permission for this peer
        let permissions = self.permissions.read().await;
        if let Some(perm) = permissions.get(&peer_addr) {
            if perm.is_expired() {
                drop(permissions);
                anyhow::bail!("Permission for peer {peer_addr} has expired");
            }
        } else {
            drop(permissions);
            anyhow::bail!("No permission for peer {peer_addr} - call create_permission() first");
        }
        drop(permissions);

        debug!(
            peer = %peer_addr,
            data_len = data.len(),
            "Sending data indication through TURN relay"
        );

        // Generate a transaction ID for XOR encoding (not used for response matching
        // since indications have no response)
        let mut transaction_id = [0u8; 12];
        rand::thread_rng().fill_bytes(&mut transaction_id);

        // Build SEND-INDICATION message
        let indication = self.build_send_indication(&transaction_id, peer_addr, data)?;

        // Send to TURN server
        local_socket
            .send_to(&indication, self.config.server)
            .await
            .context("Failed to send TURN indication")?;

        icn_obs::metrics::nat::turn_relay_send_inc();
        Ok(())
    }

    /// Parse a DATA-INDICATION message received from the TURN server
    ///
    /// Returns the peer address the data came from and the data payload.
    /// This should be called when data is received from the TURN server
    /// to extract relayed peer data.
    pub fn parse_data_indication(&self, data: &[u8]) -> Result<(SocketAddr, Vec<u8>)> {
        if data.len() < 20 {
            anyhow::bail!("DATA-INDICATION too short");
        }

        // Check message type
        let msg_type = u16::from_be_bytes([data[0], data[1]]);
        if msg_type != message_type::DATA_INDICATION {
            anyhow::bail!(
                "Not a DATA-INDICATION message: expected 0x{:04x}, got 0x{msg_type:04x}",
                message_type::DATA_INDICATION
            );
        }

        // Verify magic cookie
        let cookie = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        if cookie != MAGIC_COOKIE {
            anyhow::bail!("Invalid magic cookie in DATA-INDICATION");
        }

        // Extract transaction ID for XOR decoding
        let transaction_id: [u8; 12] = data[8..20]
            .try_into()
            .context("Failed to extract transaction ID")?;

        // Parse attributes
        let mut peer_addr = None;
        let mut payload = None;

        let msg_len = u16::from_be_bytes([data[2], data[3]]) as usize;
        let mut pos = 20;

        while pos + 4 <= 20 + msg_len && pos + 4 <= data.len() {
            let attr_type = u16::from_be_bytes([data[pos], data[pos + 1]]);
            let attr_len = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;

            if pos + 4 + attr_len > data.len() {
                break;
            }

            let attr_data = &data[pos + 4..pos + 4 + attr_len];

            match attr_type {
                attribute_type::XOR_PEER_ADDRESS => {
                    peer_addr = Some(self.xor_decode_address(attr_data, &transaction_id)?);
                }
                attribute_type::DATA => {
                    payload = Some(attr_data.to_vec());
                }
                _ => {
                    debug!(attr_type, "Ignoring unknown attribute in DATA-INDICATION");
                }
            }

            // Move to next attribute (4-byte aligned)
            pos += 4 + ((attr_len + 3) & !3);
        }

        let peer_addr = peer_addr.context("No XOR-PEER-ADDRESS in DATA-INDICATION")?;
        let payload = payload.context("No DATA attribute in DATA-INDICATION")?;

        icn_obs::metrics::nat::turn_relay_recv_inc();
        debug!(
            peer = %peer_addr,
            data_len = payload.len(),
            "Received data indication from TURN relay"
        );

        Ok((peer_addr, payload))
    }

    /// Check if a received packet is a DATA-INDICATION from the TURN server
    ///
    /// This can be used to filter incoming UDP packets and identify which
    /// ones need to be processed as relay data.
    pub fn is_data_indication(data: &[u8]) -> bool {
        if data.len() < 4 {
            return false;
        }
        let msg_type = u16::from_be_bytes([data[0], data[1]]);
        msg_type == message_type::DATA_INDICATION
    }

    // =========================================================================
    // Message building helpers
    // =========================================================================

    fn build_allocate_request(&self, transaction_id: &[u8; 12]) -> Result<Vec<u8>> {
        let mut msg = Vec::with_capacity(100);

        // Message type (Allocate Request)
        msg.extend_from_slice(&message_type::ALLOCATE_REQUEST.to_be_bytes());

        // Message length (placeholder - will fill in at end)
        let len_pos = msg.len();
        msg.extend_from_slice(&[0, 0]);

        // Magic cookie
        msg.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());

        // Transaction ID
        msg.extend_from_slice(transaction_id);

        // REQUESTED-TRANSPORT attribute (UDP = 17)
        let _transport_attr_start = msg.len();
        msg.extend_from_slice(&attribute_type::REQUESTED_TRANSPORT.to_be_bytes());
        msg.extend_from_slice(&4u16.to_be_bytes()); // length
        msg.push(17); // UDP protocol number
        msg.extend_from_slice(&[0, 0, 0]); // RFFU (reserved)

        // LIFETIME attribute
        msg.extend_from_slice(&attribute_type::LIFETIME.to_be_bytes());
        msg.extend_from_slice(&4u16.to_be_bytes());
        msg.extend_from_slice(&(self.config.allocation_lifetime.as_secs() as u32).to_be_bytes());

        // Update message length (excluding 20-byte header)
        let attr_len = (msg.len() - 20) as u16;
        msg[len_pos..len_pos + 2].copy_from_slice(&attr_len.to_be_bytes());

        Ok(msg)
    }

    fn build_refresh_request(&self, transaction_id: &[u8; 12]) -> Result<Vec<u8>> {
        let mut msg = Vec::with_capacity(50);

        // Message type (Refresh Request)
        msg.extend_from_slice(&message_type::REFRESH_REQUEST.to_be_bytes());

        // Message length (placeholder)
        let len_pos = msg.len();
        msg.extend_from_slice(&[0, 0]);

        // Magic cookie
        msg.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());

        // Transaction ID
        msg.extend_from_slice(transaction_id);

        // LIFETIME attribute
        msg.extend_from_slice(&attribute_type::LIFETIME.to_be_bytes());
        msg.extend_from_slice(&4u16.to_be_bytes());
        msg.extend_from_slice(&(self.config.allocation_lifetime.as_secs() as u32).to_be_bytes());

        // Update message length
        let attr_len = (msg.len() - 20) as u16;
        msg[len_pos..len_pos + 2].copy_from_slice(&attr_len.to_be_bytes());

        Ok(msg)
    }

    fn build_permission_request(
        &self,
        transaction_id: &[u8; 12],
        peer_addr: SocketAddr,
    ) -> Result<Vec<u8>> {
        let mut msg = Vec::with_capacity(100);

        // Message type (CreatePermission Request)
        msg.extend_from_slice(&message_type::CREATE_PERMISSION_REQUEST.to_be_bytes());

        // Message length (placeholder)
        let len_pos = msg.len();
        msg.extend_from_slice(&[0, 0]);

        // Magic cookie
        msg.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());

        // Transaction ID
        msg.extend_from_slice(transaction_id);

        // XOR-PEER-ADDRESS attribute
        let xor_addr = self.xor_encode_address(peer_addr, transaction_id)?;
        msg.extend_from_slice(&attribute_type::XOR_PEER_ADDRESS.to_be_bytes());
        msg.extend_from_slice(&(xor_addr.len() as u16).to_be_bytes());
        msg.extend_from_slice(&xor_addr);
        // Pad to 4-byte boundary
        while msg.len() % 4 != 0 {
            msg.push(0);
        }

        // Update message length
        let attr_len = (msg.len() - 20) as u16;
        msg[len_pos..len_pos + 2].copy_from_slice(&attr_len.to_be_bytes());

        Ok(msg)
    }

    fn build_send_indication(
        &self,
        transaction_id: &[u8; 12],
        peer_addr: SocketAddr,
        data: &[u8],
    ) -> Result<Vec<u8>> {
        let mut msg = Vec::with_capacity(20 + 8 + 4 + data.len() + 8); // header + peer addr attr + data attr

        // Message type (SEND-INDICATION)
        msg.extend_from_slice(&message_type::SEND_INDICATION.to_be_bytes());

        // Message length (placeholder)
        let len_pos = msg.len();
        msg.extend_from_slice(&[0, 0]);

        // Magic cookie
        msg.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());

        // Transaction ID
        msg.extend_from_slice(transaction_id);

        // XOR-PEER-ADDRESS attribute
        let xor_addr = self.xor_encode_address(peer_addr, transaction_id)?;
        msg.extend_from_slice(&attribute_type::XOR_PEER_ADDRESS.to_be_bytes());
        msg.extend_from_slice(&(xor_addr.len() as u16).to_be_bytes());
        msg.extend_from_slice(&xor_addr);
        // Pad to 4-byte boundary
        while msg.len() % 4 != 0 {
            msg.push(0);
        }

        // DATA attribute
        msg.extend_from_slice(&attribute_type::DATA.to_be_bytes());
        msg.extend_from_slice(&(data.len() as u16).to_be_bytes());
        msg.extend_from_slice(data);
        // Pad to 4-byte boundary
        while msg.len() % 4 != 0 {
            msg.push(0);
        }

        // Update message length (excluding 20-byte header)
        let attr_len = (msg.len() - 20) as u16;
        msg[len_pos..len_pos + 2].copy_from_slice(&attr_len.to_be_bytes());

        Ok(msg)
    }

    // =========================================================================
    // Response parsing helpers
    // =========================================================================

    fn parse_allocate_response(
        &self,
        data: &[u8],
        expected_transaction_id: &[u8; 12],
    ) -> Result<TurnAllocation> {
        if data.len() < 20 {
            anyhow::bail!("TURN response too short");
        }

        // Check message type
        let msg_type = u16::from_be_bytes([data[0], data[1]]);
        if msg_type == message_type::ALLOCATE_ERROR {
            let error_code = self.extract_error_code(data);
            anyhow::bail!("TURN allocation failed with error: {error_code}");
        }
        if msg_type != message_type::ALLOCATE_RESPONSE {
            anyhow::bail!("Unexpected TURN message type: 0x{msg_type:04x}");
        }

        // Verify magic cookie
        let cookie = u32::from_be_bytes([data[4], data[5], data[6], data[7]]);
        if cookie != MAGIC_COOKIE {
            anyhow::bail!("Invalid magic cookie");
        }

        // Verify transaction ID
        let transaction_id: [u8; 12] = data[8..20].try_into()?;
        if &transaction_id != expected_transaction_id {
            anyhow::bail!("Transaction ID mismatch");
        }

        // Parse attributes
        let mut relay_addr = None;
        let mut mapped_addr = None;
        let mut lifetime = DEFAULT_ALLOCATION_LIFETIME;

        let msg_len = u16::from_be_bytes([data[2], data[3]]) as usize;
        let mut pos = 20;

        while pos + 4 <= 20 + msg_len && pos + 4 <= data.len() {
            let attr_type = u16::from_be_bytes([data[pos], data[pos + 1]]);
            let attr_len = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;

            if pos + 4 + attr_len > data.len() {
                break;
            }

            let attr_data = &data[pos + 4..pos + 4 + attr_len];

            match attr_type {
                attribute_type::XOR_RELAYED_ADDRESS => {
                    relay_addr = Some(self.xor_decode_address(attr_data, &transaction_id)?);
                }
                attribute_type::XOR_MAPPED_ADDRESS => {
                    mapped_addr = Some(self.xor_decode_address(attr_data, &transaction_id)?);
                }
                attribute_type::LIFETIME => {
                    if attr_data.len() >= 4 {
                        let secs = u32::from_be_bytes([
                            attr_data[0],
                            attr_data[1],
                            attr_data[2],
                            attr_data[3],
                        ]);
                        lifetime = Duration::from_secs(secs as u64);
                    }
                }
                _ => {
                    debug!(attr_type, "Ignoring unknown TURN attribute");
                }
            }

            // Move to next attribute (4-byte aligned)
            pos += 4 + ((attr_len + 3) & !3);
        }

        let relay_addr = relay_addr.context("No XOR-RELAYED-ADDRESS in response")?;
        let mapped_addr = mapped_addr.context("No XOR-MAPPED-ADDRESS in response")?;

        Ok(TurnAllocation {
            relay_addr,
            mapped_addr,
            created_at: Instant::now(),
            lifetime,
            transaction_id,
        })
    }

    fn verify_refresh_response(
        &self,
        data: &[u8],
        expected_transaction_id: &[u8; 12],
    ) -> Result<()> {
        if data.len() < 20 {
            anyhow::bail!("TURN response too short");
        }

        let msg_type = u16::from_be_bytes([data[0], data[1]]);
        if msg_type != message_type::REFRESH_RESPONSE {
            let error_code = self.extract_error_code(data);
            anyhow::bail!("TURN refresh failed with error: {error_code}");
        }

        // Verify transaction ID
        let transaction_id: [u8; 12] = data[8..20].try_into()?;
        if &transaction_id != expected_transaction_id {
            anyhow::bail!("Transaction ID mismatch");
        }

        Ok(())
    }

    fn verify_permission_response(
        &self,
        data: &[u8],
        expected_transaction_id: &[u8; 12],
    ) -> Result<()> {
        if data.len() < 20 {
            anyhow::bail!("TURN response too short");
        }

        let msg_type = u16::from_be_bytes([data[0], data[1]]);
        if msg_type != message_type::CREATE_PERMISSION_RESPONSE {
            let error_code = self.extract_error_code(data);
            anyhow::bail!("TURN permission request failed with error: {error_code}");
        }

        // Verify transaction ID
        let transaction_id: [u8; 12] = data[8..20].try_into()?;
        if &transaction_id != expected_transaction_id {
            anyhow::bail!("Transaction ID mismatch");
        }

        Ok(())
    }

    fn extract_error_code(&self, data: &[u8]) -> String {
        // Try to find ERROR-CODE attribute
        if data.len() < 24 {
            return "unknown".to_string();
        }

        let msg_len = u16::from_be_bytes([data[2], data[3]]) as usize;
        let mut pos = 20;

        while pos + 4 <= 20 + msg_len && pos + 4 <= data.len() {
            let attr_type = u16::from_be_bytes([data[pos], data[pos + 1]]);
            let attr_len = u16::from_be_bytes([data[pos + 2], data[pos + 3]]) as usize;

            if attr_type == attribute_type::ERROR_CODE && attr_len >= 4 {
                let class = data[pos + 6] & 0x07;
                let number = data[pos + 7];
                let code = (class as u16) * 100 + (number as u16);
                let reason = if attr_len > 4 {
                    String::from_utf8_lossy(&data[pos + 8..pos + 4 + attr_len]).to_string()
                } else {
                    String::new()
                };
                return format!("{code}: {reason}");
            }

            pos += 4 + ((attr_len + 3) & !3);
        }

        "unknown".to_string()
    }

    // =========================================================================
    // XOR address encoding/decoding (per RFC 5389)
    // =========================================================================

    fn xor_encode_address(&self, addr: SocketAddr, transaction_id: &[u8; 12]) -> Result<Vec<u8>> {
        let mut result = Vec::with_capacity(20);

        match addr {
            SocketAddr::V4(v4) => {
                result.push(0); // Reserved
                result.push(0x01); // IPv4 family
                let port = addr.port() ^ ((MAGIC_COOKIE >> 16) as u16);
                result.extend_from_slice(&port.to_be_bytes());
                let ip_bytes = v4.ip().octets();
                let cookie_bytes = MAGIC_COOKIE.to_be_bytes();
                for i in 0..4 {
                    result.push(ip_bytes[i] ^ cookie_bytes[i]);
                }
            }
            SocketAddr::V6(v6) => {
                result.push(0); // Reserved
                result.push(0x02); // IPv6 family
                let port = addr.port() ^ ((MAGIC_COOKIE >> 16) as u16);
                result.extend_from_slice(&port.to_be_bytes());
                let ip_bytes = v6.ip().octets();
                let cookie_bytes = MAGIC_COOKIE.to_be_bytes();
                for i in 0..4 {
                    result.push(ip_bytes[i] ^ cookie_bytes[i]);
                }
                for i in 0..12 {
                    result.push(ip_bytes[4 + i] ^ transaction_id[i]);
                }
            }
        }

        Ok(result)
    }

    fn xor_decode_address(&self, data: &[u8], transaction_id: &[u8; 12]) -> Result<SocketAddr> {
        if data.len() < 8 {
            anyhow::bail!("XOR address too short");
        }

        let family = data[1];
        let xor_port = u16::from_be_bytes([data[2], data[3]]);
        let port = xor_port ^ ((MAGIC_COOKIE >> 16) as u16);

        match family {
            0x01 => {
                // IPv4
                let cookie_bytes = MAGIC_COOKIE.to_be_bytes();
                let ip = std::net::Ipv4Addr::new(
                    data[4] ^ cookie_bytes[0],
                    data[5] ^ cookie_bytes[1],
                    data[6] ^ cookie_bytes[2],
                    data[7] ^ cookie_bytes[3],
                );
                Ok(SocketAddr::new(std::net::IpAddr::V4(ip), port))
            }
            0x02 => {
                // IPv6
                if data.len() < 20 {
                    anyhow::bail!("XOR IPv6 address too short");
                }
                let cookie_bytes = MAGIC_COOKIE.to_be_bytes();
                let mut ip_bytes = [0u8; 16];
                for i in 0..4 {
                    ip_bytes[i] = data[4 + i] ^ cookie_bytes[i];
                }
                for i in 0..12 {
                    ip_bytes[4 + i] = data[8 + i] ^ transaction_id[i];
                }
                let ip = std::net::Ipv6Addr::from(ip_bytes);
                Ok(SocketAddr::new(std::net::IpAddr::V6(ip), port))
            }
            _ => anyhow::bail!("Unknown address family: {family}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocation_expiry() {
        let alloc = TurnAllocation {
            relay_addr: "192.168.1.1:3478".parse().unwrap(),
            mapped_addr: "192.168.1.2:3478".parse().unwrap(),
            created_at: Instant::now() - Duration::from_secs(601),
            lifetime: Duration::from_secs(600),
            transaction_id: [0; 12],
        };
        assert!(alloc.is_expired());
    }

    #[test]
    fn test_allocation_needs_refresh() {
        let alloc = TurnAllocation {
            relay_addr: "192.168.1.1:3478".parse().unwrap(),
            mapped_addr: "192.168.1.2:3478".parse().unwrap(),
            created_at: Instant::now() - Duration::from_secs(550), // 550s elapsed, refresh at 540s
            lifetime: Duration::from_secs(600),
            transaction_id: [0; 12],
        };
        assert!(alloc.needs_refresh());
        assert!(!alloc.is_expired());
    }

    #[test]
    fn test_permission_expiry() {
        let perm = TurnPermission {
            peer_addr: "192.168.1.1:3478".parse().unwrap(),
            created_at: Instant::now() - Duration::from_secs(301),
        };
        assert!(perm.is_expired());
    }

    #[test]
    fn test_config_default() {
        let config = TurnConfig::default();
        assert_eq!(config.timeout, Duration::from_secs(10));
        assert_eq!(config.allocation_lifetime, Duration::from_secs(600));
    }

    #[test]
    fn test_xor_encode_decode_ipv4() {
        let client = TurnClient::new(TurnConfig::default());
        let transaction_id = [0u8; 12];
        let addr: SocketAddr = "192.168.1.100:8080".parse().unwrap();

        let encoded = client.xor_encode_address(addr, &transaction_id).unwrap();
        let decoded = client
            .xor_decode_address(&encoded, &transaction_id)
            .unwrap();

        assert_eq!(addr, decoded);
    }

    #[test]
    fn test_build_send_indication() {
        let client = TurnClient::new(TurnConfig::default());
        let transaction_id = [1u8; 12];
        let peer_addr: SocketAddr = "10.0.0.1:5000".parse().unwrap();
        let data = b"hello world";

        let indication = client
            .build_send_indication(&transaction_id, peer_addr, data)
            .unwrap();

        // Verify header
        let msg_type = u16::from_be_bytes([indication[0], indication[1]]);
        assert_eq!(msg_type, message_type::SEND_INDICATION);

        // Verify magic cookie
        let cookie =
            u32::from_be_bytes([indication[4], indication[5], indication[6], indication[7]]);
        assert_eq!(cookie, MAGIC_COOKIE);

        // Verify transaction ID
        assert_eq!(&indication[8..20], &transaction_id);

        // Message should be well-formed (length should be > 0)
        let msg_len = u16::from_be_bytes([indication[2], indication[3]]);
        assert!(msg_len > 0);
    }

    #[test]
    fn test_is_data_indication() {
        // Valid DATA-INDICATION header
        let mut data_ind = vec![0, 0x17, 0, 0]; // msg type + length placeholder
        data_ind.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
        data_ind.extend_from_slice(&[0u8; 12]); // transaction ID
        assert!(TurnClient::is_data_indication(&data_ind));

        // Not a DATA-INDICATION (ALLOCATE_REQUEST)
        let mut not_data_ind = vec![0, 0x03, 0, 0];
        not_data_ind.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
        assert!(!TurnClient::is_data_indication(&not_data_ind));

        // Too short
        assert!(!TurnClient::is_data_indication(&[0, 0x17]));
        assert!(!TurnClient::is_data_indication(&[]));
    }

    #[test]
    fn test_parse_data_indication() {
        let client = TurnClient::new(TurnConfig::default());
        let transaction_id = [2u8; 12];
        let peer_addr: SocketAddr = "10.0.0.5:9000".parse().unwrap();
        let payload = b"test data payload";

        // Build a mock DATA-INDICATION message
        let mut msg = Vec::with_capacity(100);

        // Message type (DATA-INDICATION)
        msg.extend_from_slice(&message_type::DATA_INDICATION.to_be_bytes());

        // Message length (placeholder)
        let len_pos = msg.len();
        msg.extend_from_slice(&[0, 0]);

        // Magic cookie
        msg.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());

        // Transaction ID
        msg.extend_from_slice(&transaction_id);

        // XOR-PEER-ADDRESS attribute
        let xor_addr = client
            .xor_encode_address(peer_addr, &transaction_id)
            .unwrap();
        msg.extend_from_slice(&attribute_type::XOR_PEER_ADDRESS.to_be_bytes());
        msg.extend_from_slice(&(xor_addr.len() as u16).to_be_bytes());
        msg.extend_from_slice(&xor_addr);
        // Pad to 4-byte boundary
        while msg.len() % 4 != 0 {
            msg.push(0);
        }

        // DATA attribute
        msg.extend_from_slice(&attribute_type::DATA.to_be_bytes());
        msg.extend_from_slice(&(payload.len() as u16).to_be_bytes());
        msg.extend_from_slice(payload);
        // Pad to 4-byte boundary
        while msg.len() % 4 != 0 {
            msg.push(0);
        }

        // Update message length (excluding 20-byte header)
        let attr_len = (msg.len() - 20) as u16;
        msg[len_pos..len_pos + 2].copy_from_slice(&attr_len.to_be_bytes());

        // Parse and verify
        let (parsed_addr, parsed_data) = client.parse_data_indication(&msg).unwrap();
        assert_eq!(parsed_addr, peer_addr);
        assert_eq!(parsed_data, payload);
    }

    #[test]
    fn test_parse_data_indication_invalid_type() {
        let client = TurnClient::new(TurnConfig::default());

        // Build an ALLOCATE_RESPONSE (wrong type)
        let mut msg = Vec::new();
        msg.extend_from_slice(&message_type::ALLOCATE_RESPONSE.to_be_bytes());
        msg.extend_from_slice(&[0, 0]); // length
        msg.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
        msg.extend_from_slice(&[0u8; 12]); // transaction ID

        let result = client.parse_data_indication(&msg);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Not a DATA-INDICATION"));
    }
}
