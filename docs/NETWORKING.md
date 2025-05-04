# ICN Networking Specification

## Introduction

This document specifies the networking architecture of the Intercooperative Network (ICN), detailing the peer-to-peer (P2P) communication mechanisms, node discovery protocols, NAT traversal strategies, and security considerations that enable resilient federation operations.

> **Related Documentation:**
> - [ARCHITECTURE.md](ARCHITECTURE.md) - Overall system architecture
> - [SECURITY.md](SECURITY.md) - Security model and threat mitigations
> - [INTEGRATION_GUIDE.md](INTEGRATION_GUIDE.md) - Integration guidance

## Network Architecture Overview

The ICN implements a multi-layered networking stack designed for federation-centric peer-to-peer communication:

```
┌─────────────────────────────────────────────────────────┐
│                   Network Stack Layers                  │
├─────────────────────────────────────────────────────────┤
│ • Application Layer: AgoraNet API & Federation Protocol │
│ • Transport Layer: libp2p with QUIC                     │
│ • Discovery Layer: DHT + Bootstrapping                  │
│ • Security Layer: TLS 1.3 + Noise Protocol              │
└─────────────────────────────────────────────────────────┘
```

### Design Principles

ICN's networking layer adheres to the following principles:

1. **Federation-First**: Optimized for federation-based communication patterns
2. **Resilience**: Robust against network partitions and node failures
3. **Verifiability**: All messages are cryptographically verifiable
4. **Privacy-Preserving**: Minimal metadata leakage
5. **NAT-Traversal**: Works across diverse network configurations
6. **Scalability**: Efficient with hundreds of federation nodes

## Transport Protocol

### libp2p with QUIC

ICN uses libp2p with QUIC (Quick UDP Internet Connections) as its primary transport protocol:

```rust
pub struct NetworkConfig {
    // Transport configuration
    pub transport: TransportConfig,
    
    // Listening addresses
    pub listen_addresses: Vec<Multiaddr>,
    
    // External addresses (for NAT mapping)
    pub external_addresses: Vec<Multiaddr>,
    
    // TLS configuration
    pub tls_config: TlsConfig,
    
    // QUIC-specific settings
    pub quic_config: QuicConfig,
}

pub struct QuicConfig {
    // Maximum concurrent bi-directional streams
    pub max_bi_streams: u32,
    
    // Maximum concurrent uni-directional streams
    pub max_uni_streams: u32,
    
    // Flow control parameters
    pub initial_max_data: u32,
    pub initial_max_stream_data: u32,
    
    // Keep-alive interval
    pub keep_alive_interval: Duration,
    
    // Idle timeout
    pub idle_timeout: Duration,
}
```

#### QUIC Protocol Benefits

1. **Multiplexed Connections**: Multiple streams over a single connection
2. **Low Latency**: 0-RTT connection establishment for known peers
3. **Improved Congestion Control**: Better performance in varying network conditions
4. **Built-in Encryption**: TLS 1.3 is integrated into the protocol
5. **Connection Migration**: Seamless handling of IP address changes

### Network Establishment

```rust
pub async fn establish_network(
    config: &NetworkConfig,
    identity: &NetworkIdentity,
) -> Result<Network, NetworkError> {
    // Create transport with QUIC
    let transport = build_quic_transport(
        &config.quic_config,
        identity,
        &config.tls_config,
    )?;
    
    // Create swarm with transport and behaviors
    let mut swarm = create_network_swarm(transport, config)?;
    
    // Listen on configured addresses
    for addr in &config.listen_addresses {
        swarm.listen_on(addr.clone())?;
    }
    
    // Announce external addresses if provided
    for addr in &config.external_addresses {
        swarm.add_external_address(addr.clone(), AddressScore::Explicit);
    }
    
    // Start network event loop
    spawn_network_event_loop(swarm.clone());
    
    Ok(Network::new(swarm))
}
```

## Node Discovery

ICN implements multiple discovery mechanisms to ensure nodes can reliably find each other across different network conditions:

### 1. DHT-Based Discovery

Kademlia Distributed Hash Table (DHT) for scalable peer discovery:

```rust
pub struct DhtConfig {
    // DHT mode
    pub mode: DhtMode,
    
    // Query timeout
    pub query_timeout: Duration,
    
    // Replication factor
    pub replication_factor: u16,
    
    // Record TTL
    pub record_ttl: Duration,
    
    // Bootstrap peers
    pub bootstrap_peers: Vec<Multiaddr>,
    
    // Provider record publication interval
    pub provider_publication_interval: Duration,
}

pub enum DhtMode {
    // Client mode (no record storage)
    Client,
    
    // Server mode (stores records)
    Server,
    
    // Autodetect based on observed network conditions
    Auto,
}
```

### 2. Bootstrap Nodes

Federation-operated bootstrap nodes provide reliable entry points to the network:

```rust
pub fn configure_bootstrap_nodes(
    swarm: &mut Swarm<ComposedBehavior>,
    bootstrap_nodes: &[Multiaddr],
) -> Result<(), DiscoveryError> {
    // Add bootstrap nodes to peer routing table
    for addr in bootstrap_nodes {
        swarm.add_known_address(
            extract_peer_id(addr)?,
            addr.clone(),
            AddressSource::Bootstrap,
        );
    }
    
    // Schedule bootstrap process
    swarm.behaviour_mut().bootstrap.bootstrap()?;
    
    Ok(())
}
```

### 3. Local Network Discovery

For nodes on the same local network, mDNS discovery is used:

```rust
pub struct MdnsConfig {
    // Enable/disable mDNS discovery
    pub enabled: bool,
    
    // Time-to-live for mDNS records
    pub ttl: Duration,
    
    // Query interval
    pub query_interval: Duration,
    
    // Service name
    pub service_name: String,
}
```

### 4. Federation Registry

Federation-specific discovery using on-chain and off-chain registries:

```rust
pub struct FederationRegistry {
    // Federation identifier
    pub federation_id: FederationId,
    
    // Registry nodes with their roles
    pub nodes: HashMap<PeerId, NodeRole>,
    
    // Last update timestamp
    pub last_updated: DateTime<Utc>,
    
    // Registry signature
    pub signature: FederationSignature,
}
```

## NAT Traversal

ICN implements several NAT traversal strategies to ensure connectivity across diverse network environments:

### 1. STUN/TURN Integration

STUN (Session Traversal Utilities for NAT) for NAT type detection and reflexive address discovery:

```rust
pub struct StunConfig {
    // STUN servers
    pub servers: Vec<SocketAddr>,
    
    // Binding refresh interval
    pub binding_refresh_interval: Duration,
    
    // Keep-alive interval
    pub keep_alive_interval: Duration,
}
```

TURN (Traversal Using Relays around NAT) for relaying traffic when direct connectivity fails:

```rust
pub struct TurnConfig {
    // TURN servers
    pub servers: Vec<TurnServer>,
    
    // Maximum allocation lifetime
    pub max_allocation_lifetime: Duration,
    
    // Credentials
    pub credentials: TurnCredentials,
}
```

### 2. Hole Punching

ICN implements both TCP and UDP hole punching for establishing peer-to-peer connections:

```rust
pub enum HolePunchingStrategy {
    // Direct connection attempt
    Direct,
    
    // Symmetric NAT detection and hole punching
    Symmetric,
    
    // Restricted NAT handling
    Restricted,
    
    // Relay fallback when hole punching fails
    RelayFallback,
}
```

### 3. Circuit Relay

For peers that cannot establish direct connections, ICN uses libp2p circuit relay:

```rust
pub struct RelayConfig {
    // Maximum relay connections
    pub max_relay_connections: usize,
    
    // Maximum circuit duration
    pub max_circuit_duration: Duration,
    
    // Buffer sizes for relayed connections
    pub max_circuit_buffer_size: usize,
    
    // Limit per peer
    pub per_peer_circuit_limit: usize,
}
```

Implementation example:

```rust
pub async fn establish_connection(
    network: &Network,
    peer: &PeerId,
    connection_options: &ConnectionOptions,
) -> Result<Connection, ConnectionError> {
    // Try direct connection
    if let Ok(conn) = network.dial_peer(peer, connection_options.dial_opts.clone()).await {
        return Ok(conn);
    }
    
    // Try hole punching if direct connection failed
    if connection_options.enable_hole_punching {
        if let Ok(conn) = attempt_hole_punching(network, peer).await {
            return Ok(conn);
        }
    }
    
    // Fall back to relay if hole punching failed
    if connection_options.enable_relay {
        if let Ok(conn) = establish_relayed_connection(network, peer).await {
            return Ok(conn);
        }
    }
    
    Err(ConnectionError::ConnectionFailed)
}
```

## Federation Network Topology

### Federation Mesh Network

Within a federation, nodes form a densely connected mesh network:

```rust
pub struct FederationNetwork {
    // Federation identifier
    pub federation_id: FederationId,
    
    // Mesh network configuration
    pub mesh_config: MeshNetworkConfig,
    
    // Connection management
    pub connection_manager: ConnectionManager,
    
    // Routing table
    pub routing_table: RoutingTable,
}

pub struct MeshNetworkConfig {
    // Target number of connections per node
    pub target_connections: usize,
    
    // Maximum connections per node
    pub max_connections: usize,
    
    // Connection pruning interval
    pub pruning_interval: Duration,
    
    // Heartbeat interval
    pub heartbeat_interval: Duration,
}
```

### Cross-Federation Connectivity

For cross-federation communication, designated gateway nodes establish connections:

```rust
pub struct FederationGateway {
    // Home federation
    pub home_federation: FederationId,
    
    // Connected federations
    pub connected_federations: HashMap<FederationId, GatewayConnection>,
    
    // Gateway capabilities
    pub capabilities: GatewayCapabilities,
    
    // Routing policies
    pub routing_policies: RoutingPolicies,
}
```

## Network Security

### Message Authentication

All ICN network messages are authenticated:

```rust
pub struct AuthenticatedMessage {
    // Message content
    pub content: Vec<u8>,
    
    // Message type
    pub message_type: MessageType,
    
    // Sender identity
    pub sender: Did,
    
    // Timestamp
    pub timestamp: DateTime<Utc>,
    
    // Signature
    pub signature: Signature,
}
```

### Transport Security

TLS 1.3 and Noise Protocol provide transport-level security:

```rust
pub struct TlsConfig {
    // Certificate chain
    pub certificates: Vec<Certificate>,
    
    // Private key
    pub private_key: PrivateKey,
    
    // Certificate verification
    pub verify_mode: CertificateVerifyMode,
    
    // ALPN protocols
    pub alpn_protocols: Vec<String>,
    
    // Cipher suites (in preference order)
    pub cipher_suites: Vec<CipherSuite>,
}
```

### Connection Filtering

Connections can be filtered based on various criteria:

```rust
pub struct ConnectionFilter {
    // IP allow/block lists
    pub ip_filter: IpFilter,
    
    // Peer ID filter
    pub peer_id_filter: PeerIdFilter,
    
    // Bandwidth usage filter
    pub bandwidth_filter: BandwidthFilter,
    
    // Behavior-based filtering
    pub behavior_filter: BehaviorFilter,
}
```

## Network Metrics and Diagnostics

### Prometheus Metrics

ICN exposes detailed network metrics via Prometheus:

```
# HELP icn_network_connections_total Total number of active connections
# TYPE icn_network_connections_total gauge
icn_network_connections_total{peer_type="federation"} 12
icn_network_connections_total{peer_type="client"} 156

# HELP icn_network_messages_sent_total Total number of messages sent
# TYPE icn_network_messages_sent_total counter
icn_network_messages_sent_total{message_type="node_sync"} 1452
icn_network_messages_sent_total{message_type="anchor"} 87

# HELP icn_network_bandwidth_bytes Network bandwidth usage in bytes
# TYPE icn_network_bandwidth_bytes counter
icn_network_bandwidth_bytes{direction="inbound"} 1546782
icn_network_bandwidth_bytes{direction="outbound"} 987234
```

### Network Diagnostics

ICN includes network diagnostic tools:

```rust
pub async fn run_network_diagnostics(
    network: &Network,
) -> Result<DiagnosticsReport, DiagnosticsError> {
    // Test local network connectivity
    let local_connectivity = test_local_connectivity(network).await?;
    
    // Test NAT traversal
    let nat_traversal = test_nat_traversal(network).await?;
    
    // Test DHT functionality
    let dht_functionality = test_dht_functionality(network).await?;
    
    // Test federation connectivity
    let federation_connectivity = test_federation_connectivity(network).await?;
    
    // Bandwidth measurement
    let bandwidth = measure_bandwidth(network).await?;
    
    // Create diagnostic report
    let report = DiagnosticsReport {
        timestamp: DateTime::now_utc(),
        local_connectivity,
        nat_traversal,
        dht_functionality,
        federation_connectivity,
        bandwidth,
        recommendations: generate_recommendations(
            &local_connectivity,
            &nat_traversal,
            &dht_functionality,
            &federation_connectivity,
            &bandwidth,
        )?,
    };
    
    Ok(report)
}
```

## Network Configuration

### Firewall Requirements

```
┌─────────────────────────────────────────────────────────┐
│                 Firewall Requirements                   │
├─────────────────────────────────────────────────────────┤
│ • TCP/UDP 9000: Primary libp2p QUIC listener           │
│ • TCP 9001: Backup TCP transport                       │
│ • UDP 9002: STUN/TURN protocols                        │
│ • TCP 9003: Metrics endpoint                           │
│ • TCP 9004: API endpoint                               │
│ • UDP 5353: mDNS (local discovery, optional)           │
└─────────────────────────────────────────────────────────┘
```

### Production Deployment Example

```toml
# network.toml
[transport]
primary = "quic"
fallback = "tcp"

[listen]
addresses = [
  "/ip4/0.0.0.0/udp/9000/quic",
  "/ip4/0.0.0.0/tcp/9001"
]

[external]
# Optional: Set if behind NAT
addresses = [
  "/ip4/198.51.100.1/udp/9000/quic",
  "/ip4/198.51.100.1/tcp/9001"
]

[discovery]
mode = "server"
bootstrap_peers = [
  "/ip4/198.51.100.2/udp/9000/quic/p2p/12D3KooWRsEKaG9KWZr6r1kPGC8XVT6nalvqXkx1xVLYgBXMVuJa",
  "/ip4/198.51.100.3/udp/9000/quic/p2p/12D3KooWJbAU7qVE9DSQkqgY7B3ziMnQ4oKwPF3dGJJXwoYN2aek"
]
mdns_enabled = false
enable_relay = true

[nat]
traversal_strategy = "all"
stun_servers = [
  "stun.example.com:3478",
  "stun2.example.com:3478"
]
turn_servers = [
  "turn.example.com:3478"
]

[security]
certificate_file = "/etc/icn/tls/node.crt"
key_file = "/etc/icn/tls/node.key"
```

## Connection Management

### Connection Lifecycle

```rust
pub enum ConnectionEvent {
    // New outbound connection established
    OutboundEstablished {
        peer_id: PeerId,
        endpoint: ConnectedPoint,
        connection_id: ConnectionId,
    },
    
    // New inbound connection established
    InboundEstablished {
        peer_id: PeerId,
        endpoint: ConnectedPoint,
        connection_id: ConnectionId,
    },
    
    // Connection upgraded
    ConnectionUpgraded {
        peer_id: PeerId,
        connection_id: ConnectionId,
        new_capabilities: Vec<Capability>,
    },
    
    // Connection closed
    ConnectionClosed {
        peer_id: PeerId,
        connection_id: ConnectionId,
        reason: DisconnectReason,
    },
    
    // Connection failed
    ConnectionFailed {
        peer_id: Option<PeerId>,
        endpoint: ConnectFailure,
        error: ConnectionError,
        attempt: u32,
    },
}
```

### Bandwidth Management

```rust
pub struct BandwidthManager {
    // Bandwidth allocation by peer
    pub allocations: HashMap<PeerId, BandwidthAllocation>,
    
    // Global bandwidth limits
    pub global_limits: BandwidthLimits,
    
    // Priority classes
    pub priority_classes: HashMap<Priority, BandwidthLimits>,
    
    // Throttling policy
    pub throttling_policy: ThrottlingPolicy,
}
```

## Protocol Handlers

### Protocol Registration

```rust
pub fn register_protocol_handler<T: ProtocolHandler>(
    network: &mut Network,
    handler: T,
) -> Result<ProtocolId, ProtocolError> {
    // Generate protocol ID
    let protocol_id = generate_protocol_id(&handler.protocol_name())?;
    
    // Register protocol handler with network
    network.register_protocol(
        protocol_id.clone(),
        handler.protocol_name(),
        handler.protocol_version(),
        handler,
    )?;
    
    Ok(protocol_id)
}
```

### Message Routing

```rust
pub enum MessageRouting {
    // Direct message to a specific peer
    Direct(PeerId),
    
    // Broadcast to all peers
    Broadcast,
    
    // Broadcast to peers in a specific federation
    FederationBroadcast(FederationId),
    
    // Route via DHT to responsible peers
    Dht(DhtKey),
    
    // Gossip with specific propagation parameters
    Gossip(GossipParameters),
}
```

## Glossary

| Term | Definition |
|------|------------|
| **Bootstrap Node** | A node with a well-known address used as an entry point to the network. |
| **Circuit Relay** | A node that relays traffic between two peers that cannot establish a direct connection. |
| **DHT** | Distributed Hash Table, a decentralized system for peer and resource discovery. |
| **Federation Gateway** | A node that facilitates communication between different federations. |
| **Hole Punching** | A NAT traversal technique to establish P2P connections between peers behind NATs. |
| **Kademlia** | A distributed hash table for decentralized peer-to-peer networks. |
| **libp2p** | A modular network stack for building peer-to-peer applications. |
| **Mesh Network** | A network topology where nodes connect to many other nodes forming a mesh. |
| **mDNS** | Multicast DNS, used for local network service discovery. |
| **Multiaddr** | A self-describing network address format used in libp2p. |
| **NAT** | Network Address Translation, a method of remapping IP addresses. |
| **PeerId** | A unique identifier for a peer in the network, derived from its public key. |
| **QUIC** | A transport protocol providing secure, multiplexed connections over UDP. |
| **STUN** | Session Traversal Utilities for NAT, a protocol to discover public IP address and NAT type. |
| **TURN** | Traversal Using Relays around NAT, a protocol that provides relaying of data when direct connections fail. |