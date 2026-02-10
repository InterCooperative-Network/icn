# NAT Traversal Design

**Status**: Design Phase (MVC Week 3-4)
**Priority**: Tier 1 Gap #2 - Blocking pilot deployment

## Problem Statement

Real cooperatives operate behind NAT/firewalls (home routers, corporate networks). Current ICN requires:
- Manually configured port forwarding, OR
- Nodes with public IP addresses

This is **unacceptable** for production deployment. Members cannot be expected to configure their routers.

## Success Criteria

- ✅ Members behind home routers connect without port forwarding
- ✅ Two NAT-ed nodes can establish direct connection (hole punching)
- ✅ Relay fallback when direct connection fails (symmetric NAT, strict firewalls)
- ✅ No degradation of security (all traffic still authenticated + encrypted)
- ✅ Minimal latency overhead (<100ms added for STUN discovery)

## Architecture Overview

### Three-Tier Connection Strategy

```
1. Direct Connection (preferred)
   └─ Public IP nodes connect directly via QUIC

2. STUN Hole Punching (most NAT scenarios)
   ├─ Query STUN server for public IP/port
   ├─ Exchange connection info via gossip
   └─ Simultaneous hole punch to establish direct QUIC

3. TURN Relay (fallback for symmetric NAT)
   ├─ Route traffic through trusted relay node
   └─ Still end-to-end encrypted (QUIC)
```

### Integration with Existing Architecture

ICN uses Quinn (Rust QUIC library), which makes NAT traversal straightforward:

**Key Insight**: QUIC's header design allows multiplexing STUN and QUIC on the same UDP socket. Quinn endpoints expose the underlying UDP socket for this exact use case.

## Technical Design

### 1. STUN Client Integration

**Crate**: Use `stun-rs` (supports STUN + TURN + ICE attributes per RFC8445/RFC8656)

**Responsibility**: Discover public IP/port for a node behind NAT

```rust
// icn-net/src/stun.rs
use stun_rs::MessageClass;

pub struct StunClient {
    server_addr: SocketAddr,  // e.g., stun.l.google.com:19302
}

impl StunClient {
    /// Discover public IP/port by querying STUN server
    pub async fn discover_public_endpoint(
        &self,
        local_socket: &UdpSocket,
    ) -> Result<SocketAddr> {
        // Send STUN Binding Request
        let request = Message::new(MessageClass::Request(BINDING));
        send_to_stun_server(local_socket, &request, self.server_addr).await?;

        // Receive STUN Binding Response
        let response = receive_from_stun_server(local_socket).await?;
        let public_addr = extract_mapped_address(&response)?;

        Ok(public_addr)
    }
}
```

**STUN Servers**: Use public STUN infrastructure initially:
- `stun.l.google.com:19302` (Google)
- `stun1.l.google.com:19302` (Google backup)
- `stun.icn.coop:3478` (ICN-operated, future)

### 2. Connection Candidate Exchange

**Challenge**: How do two NAT-ed peers exchange their public endpoints?

**Solution**: Use existing gossip protocol to announce connection info

```rust
// icn-gossip/src/connection.rs

/// Broadcast to topic: "network:candidates"
#[derive(Serialize, Deserialize)]
pub struct ConnectionCandidate {
    pub did: Did,
    pub local_addr: SocketAddr,   // Private IP
    pub public_addr: Option<SocketAddr>,  // Discovered via STUN
    pub relay_addr: Option<SocketAddr>,   // TURN relay (if available)
    pub timestamp: u64,
}
```

**Flow**:
1. Node A starts, queries STUN, discovers public endpoint
2. Node A publishes `ConnectionCandidate` to gossip topic
3. Node B receives candidates via gossip subscription
4. Node B attempts connection using:
   - Try `public_addr` first (direct or hole-punched)
   - Fall back to `relay_addr` if direct fails

### 3. Hole Punching Strategy

**UDP Hole Punching**: Send packets from both sides simultaneously to "punch" through NAT

```rust
// icn-net/src/hole_punch.rs

pub async fn attempt_hole_punch(
    endpoint: &Endpoint,
    local_public: SocketAddr,
    remote_public: SocketAddr,
    peer_did: Did,
) -> Result<Connection> {
    // Timing is critical: both peers must send simultaneously
    // Use synchronized timestamp from gossip message

    // Send hole-punch packet (empty QUIC initial)
    endpoint.connect(remote_public, "localhost")?.await
}
```

**NAT Type Compatibility**:
- Full Cone NAT: ✅ Works perfectly
- Restricted Cone NAT: ✅ Works (most common)
- Port-Restricted Cone NAT: ✅ Works with coordination
- Symmetric NAT: ❌ Requires TURN relay

### 4. TURN Relay Implementation

**Two Modes**:

**A. External TURN Server** (short-term, bootstrap):
- Use `coturn` or similar TURN server
- Hosted at `turn.icn.coop`
- Credentials distributed via bootstrap config

**B. Peer Relay Capability** (long-term, decentralized):
- Nodes with public IPs can volunteer as relays
- Trust-gated: only relay for trusted peers (trust score > 0.4)
- Resource limits: max relay bandwidth per peer

```rust
// icn-net/src/relay.rs

pub struct RelayNode {
    max_relay_bandwidth_mbps: u32,  // e.g., 10 Mbps
    min_trust_score: f64,            // e.g., 0.4 (partner tier)
    allowed_peers: HashSet<Did>,     // Explicit whitelist
}

impl RelayNode {
    /// Check if we should relay for this peer
    fn should_relay_for(&self, peer_did: &Did, trust_graph: &TrustGraph) -> bool {
        if self.allowed_peers.contains(peer_did) {
            return true;
        }

        match trust_graph.compute_trust_score(peer_did) {
            Ok(score) if score >= self.min_trust_score => true,
            _ => false,
        }
    }
}
```

### 5. Session Manager Integration

**Modify `SessionManager` to support NAT traversal**:

```rust
// icn-net/src/session.rs

impl SessionManager {
    pub async fn start(&mut self, ..., nat_config: NatTraversalConfig) -> Result<()> {
        // 1. Create QUIC endpoint as before
        let endpoint = Endpoint::server(server_config, listen_addr)?;

        // 2. If NAT traversal enabled, discover public endpoint
        if nat_config.stun_enabled {
            let stun_client = StunClient::new(nat_config.stun_server);
            let public_addr = stun_client.discover_public_endpoint(
                endpoint.local_addr()?
            ).await?;

            info!("Discovered public endpoint: {}", public_addr);

            // 3. Announce to gossip
            self.announce_connection_candidate(public_addr).await?;
        }

        // 4. Subscribe to connection candidates from peers
        self.subscribe_to_candidates().await?;

        Ok(())
    }

    /// Attempt connection with NAT traversal
    pub async fn dial_with_nat(&self, peer_candidate: ConnectionCandidate) -> Result<Connection> {
        // Try strategies in order:
        // 1. Direct connection to public_addr (if no NAT or hole punch works)
        if let Some(public_addr) = peer_candidate.public_addr {
            if let Ok(conn) = self.dial(public_addr, peer_candidate.did).await {
                return Ok(conn);
            }
        }

        // 2. Try relay if available
        if let Some(relay_addr) = peer_candidate.relay_addr {
            return self.dial_via_relay(relay_addr, peer_candidate.did).await;
        }

        bail!("All connection strategies failed")
    }
}
```

### 6. Configuration

**User-facing config in `icn.toml`**:

```toml
[nat]
enabled = true
stun_servers = [
    "stun.l.google.com:19302",
    "stun.icn.coop:3478"
]

# Optional: Enable relay capability on this node
[relay]
enabled = false
max_bandwidth_mbps = 10
min_trust_score = 0.4
```

**CLI arguments**:
```bash
icnd --nat-enable --stun-server stun.l.google.com:19302
icnd --relay-enable --relay-max-bw 10
```

## Implementation Plan

### Phase 1: STUN Discovery (Week 3, Days 1-2)
- [ ] Add `stun-rs` dependency to `icn-net/Cargo.toml`
- [ ] Implement `StunClient` in `icn-net/src/stun.rs`
- [ ] Unit tests: STUN query/response parsing
- [ ] Integration test: Discover public endpoint

### Phase 2: Candidate Exchange (Week 3, Days 3-4)
- [ ] Add `ConnectionCandidate` message to `icn-gossip`
- [ ] Create `network:candidates` gossip topic
- [ ] SessionManager announces candidate on startup
- [ ] Subscribe to peer candidates

### Phase 3: Hole Punching (Week 3, Day 5)
- [ ] Implement `attempt_hole_punch()` in `icn-net`
- [ ] Integration test: Two NAT-ed nodes connect
- [ ] Test various NAT types (full cone, restricted, symmetric)

### Phase 4: TURN Relay (Week 4, Days 1-3)
- [ ] Deploy external TURN server (`coturn`) at `turn.icn.coop`
- [ ] Implement `dial_via_relay()` using TURN allocation
- [ ] Integration test: Symmetric NAT nodes connect via relay

### Phase 5: Peer Relay Capability (Week 4, Days 4-5)
- [ ] Implement `RelayNode` with trust-gating
- [ ] Add relay configuration to bootstrap nodes
- [ ] Integration test: Node relays traffic for trusted peer

## Security Considerations

### 1. STUN Server Trust
**Risk**: Malicious STUN server could lie about public IP
**Mitigation**:
- Query multiple STUN servers, use majority vote
- Fallback to known-good Google STUN servers

### 2. Relay Node Trust
**Risk**: Relay could log/inspect traffic
**Mitigation**:
- All traffic still end-to-end encrypted (QUIC/TLS)
- Relay sees encrypted packets only
- Trust-gate relay usage (only for partners)

### 3. DoS via Relay
**Risk**: Attacker requests relay for expensive traffic
**Mitigation**:
- Bandwidth limits per peer
- Trust-gated relay (score > 0.4)
- Rate limiting relay requests

## Testing Strategy

### Unit Tests
- STUN message parsing
- Connection candidate serialization
- Trust-gated relay logic

### Integration Tests
```rust
#[tokio::test]
async fn test_nat_traversal_full_cone() {
    // Simulate two nodes behind full cone NAT
    // Verify direct connection via hole punch
}

#[tokio::test]
async fn test_nat_traversal_symmetric_relay() {
    // Simulate symmetric NAT (worst case)
    // Verify relay fallback works
}

#[tokio::test]
async fn test_relay_trust_gating() {
    // Verify relay rejects low-trust peers
}
```

### Manual Testing
- AWS NAT Gateway simulation
- Home router testing (multiple ISPs)
- Symmetric NAT testing (corporate firewalls)

## Metrics

**Prometheus metrics to add**:
```rust
icn_network_nat_type{type="full_cone|restricted|symmetric"} - Detected NAT type
icn_network_stun_queries_total - STUN queries sent
icn_network_hole_punch_attempts_total{result="success|failed"} - Hole punch attempts
icn_network_relay_connections_total - Connections via relay
icn_network_relay_bytes_total{direction="in|out"} - Relay traffic volume
```

## Documentation Updates

- [ ] `docs/deployment-guide.md`: Add NAT traversal configuration
- [ ] `docs/ARCHITECTURE.md`: Document NAT traversal architecture
- [ ] `README.md`: Update with NAT traversal capabilities

## Future Enhancements (Post-MVC)

- **ICE (Interactive Connectivity Establishment)**: Full RFC8445 support for optimal path selection
- **IPv6 Support**: Many home routers now have IPv6, no NAT needed
- **Relay Load Balancing**: Distribute relay load across multiple nodes
- **Relay Incentives**: Economic model for relay operators (credit for bandwidth)

## References

- RFC 5389: STUN (Session Traversal Utilities for NAT)
- RFC 8656: TURN (Traversal Using Relays around NAT)
- RFC 8445: ICE (Interactive Connectivity Establishment)
- [QUIC NAT Traversal Draft](https://www.ietf.org/archive/id/draft-seemann-quic-nat-traversal-01.html)
- [Tailscale: How NAT Traversal Works](https://tailscale.com/blog/how-nat-traversal-works)
- [libp2p QUIC NAT Traversal Issue](https://github.com/libp2p/specs/issues/434)
