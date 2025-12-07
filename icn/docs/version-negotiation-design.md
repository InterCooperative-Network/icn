# Version Negotiation & Capability Exchange Design

## Overview

This document describes the version negotiation protocol for ICN, enabling nodes running different software versions to communicate safely and discover each other's capabilities.

## Goals

1. **Backward Compatibility**: Newer nodes can communicate with older nodes
2. **Forward Compatibility**: Older nodes gracefully reject incompatible newer versions
3. **Feature Detection**: Nodes announce supported capabilities
4. **Graceful Degradation**: Use common feature set when versions differ
5. **Security**: Version info is authenticated (part of signed Hello)

## Current State

**Existing Version Validation** ([icn-net/src/protocol.rs](../crates/icn-net/src/protocol.rs)):
- `PROTOCOL_VERSION = 1` (current version)
- `MIN_SUPPORTED_VERSION = 1` (oldest compatible version)
- `MAX_SUPPORTED_VERSION = 1` (newest compatible version)
- `NetworkMessage::validate_version()` checks compatibility

**Limitations:**
- No negotiation - just validation
- No capability exchange
- No per-connection version tracking
- No feature detection

## Design

### 1. Version Information Structure

```rust
/// Version capabilities announced during handshake
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionInfo {
    /// Current protocol version this node is running
    pub current_version: u32,

    /// Minimum protocol version this node supports
    pub min_supported: u32,

    /// Maximum protocol version this node supports
    pub max_supported: u32,

    /// Optional capabilities bitmap for feature detection
    pub capabilities: CapabilityFlags,

    /// Software version string (e.g., "icnd-0.1.0")
    pub software_version: String,
}

bitflags::bitflags! {
    /// Feature capability flags
    #[derive(Debug, Clone, Copy, Serialize, Deserialize)]
    pub struct CapabilityFlags: u64 {
        /// Supports end-to-end encryption (Phase 10)
        const E2E_ENCRYPTION = 0b00000001;

        /// Supports signed envelopes (Phase 9)
        const SIGNED_MESSAGES = 0b00000010;

        /// Supports graceful restart / state snapshots
        const GRACEFUL_RESTART = 0b00000100;

        /// Supports topology-aware networking
        const TOPOLOGY_AWARE = 0b00001000;

        /// Supports trust-gated rate limiting
        const TRUST_RATE_LIMITING = 0b00010000;

        /// Supports gossip pull protocol
        const GOSSIP_PULL = 0b00100000;

        /// Supports multi-device identity (Phase 11)
        const MULTI_DEVICE = 0b01000000;

        /// Supports economic safety rails (Phase 12)
        const ECONOMIC_SAFETY = 0b10000000;

        // Future capabilities can be added here
        // Reserve high bits for future use
    }
}
```

### 2. Protocol Changes

**Modify Hello Message** to include version info:

```rust
pub enum MessagePayload {
    // ... existing variants ...

    /// Hello message with version negotiation
    Hello {
        /// DID-TLS binding information
        binding_info: BindingInfo,

        /// Version and capability information
        version_info: VersionInfo,

        /// Optional topology information
        topology_info: Option<TopologyInfo>,

        /// X25519 public key for E2E encryption
        x25519_public: [u8; 32],
    },
}
```

### 3. Version Negotiation Process

**Connection Establishment Flow:**

```
Node A (v1, min=1, max=2)    Node B (v2, min=1, max=3)
         |                            |
         |---- Hello + VersionInfo -->|
         |      (version=1)           |
         |                            |
         |<--- Hello + VersionInfo ---|
         |      (version=2)           |
         |                            |
         |-- Negotiate to version 1 --|  (min of max_supported)
         |                            |
         |<-- Further messages use ---|
         |    negotiated version      |
```

**Negotiation Algorithm:**

```rust
fn negotiate_version(local: &VersionInfo, remote: &VersionInfo) -> Result<u32> {
    // Find the overlap between supported versions
    let negotiated = std::cmp::min(local.max_supported, remote.max_supported);

    // Verify it's within both nodes' supported range
    if negotiated < local.min_supported || negotiated < remote.min_supported {
        anyhow::bail!(
            "No compatible version. Local: [{}-{}], Remote: [{}-{}]",
            local.min_supported, local.max_supported,
            remote.min_supported, remote.max_supported
        );
    }

    Ok(negotiated)
}
```

### 4. Capability Detection

**Feature Usage Pattern:**

```rust
// Check if peer supports a capability
if connection.peer_capabilities().contains(CapabilityFlags::E2E_ENCRYPTION) {
    // Use encrypted messages
    network.send_encrypted(peer_did, message)?;
} else {
    // Fall back to signed-only
    network.send_signed(peer_did, message)?;
}
```

### 5. Per-Connection Version Tracking

**Add to NetworkActor state:**

```rust
/// Per-peer connection metadata
struct PeerConnection {
    did: Did,
    address: SocketAddr,

    /// Negotiated protocol version for this connection
    negotiated_version: u32,

    /// Peer's announced capabilities
    peer_capabilities: CapabilityFlags,

    /// Peer's software version string
    peer_software: String,

    /// X25519 key for E2E encryption
    x25519_key: [u8; 32],
}
```

### 6. Metrics

**Add Prometheus metrics:**

```rust
// Version distribution across peers
icn_network_peer_versions{version="1"} 45
icn_network_peer_versions{version="2"} 12

// Capability support across peers
icn_network_peer_capabilities{capability="e2e_encryption"} 57
icn_network_peer_capabilities{capability="topology_aware"} 32

// Version negotiation failures
icn_network_version_negotiation_failures_total{reason="too_old"} 2
icn_network_version_negotiation_failures_total{reason="too_new"} 0
```

## Migration Path

### Phase 1: Add VersionInfo to Hello (Backward Compatible)

1. Add `VersionInfo` struct and `CapabilityFlags`
2. Modify `MessagePayload::Hello` to include `version_info`
3. Update Hello message creation to populate version info
4. Track negotiated version per connection
5. **Backward compatibility**: Older nodes reject new Hello format (expected)

### Phase 2: Capability-Based Features

1. Check capabilities before using advanced features
2. Fall back to basic protocol when capabilities missing
3. Log capability mismatches for debugging

### Phase 3: Future Protocol Versions

When creating protocol v2:
1. Increment `PROTOCOL_VERSION = 2`
2. Set `MIN_SUPPORTED_VERSION = 1` (still support v1)
3. Set `MAX_SUPPORTED_VERSION = 2`
4. Add new capabilities to `CapabilityFlags`
5. Implement feature detection in protocol handlers

## Testing Strategy

### Unit Tests

- Version negotiation algorithm (various ranges)
- Capability flag serialization
- Incompatible version rejection

### Integration Tests

- Two nodes with same version
- Two nodes with different versions (negotiation)
- Two nodes with no overlap (rejection)
- Capability detection and fallback

### Stress Tests

- 100 nodes with mixed versions (5 different versions)
- Verify all find compatible versions
- Metrics tracking correct

## Security Considerations

1. **Version Info Integrity**: VersionInfo is part of Hello message, which should be signed
2. **Downgrade Attacks**: Attacker cannot force older version (both sides validate)
3. **Capability Lying**: Nodes can claim unsupported capabilities - protocol must handle gracefully
4. **Version Fingerprinting**: Software version strings may leak info - consider privacy implications

## Future Enhancements

1. **Capability Negotiation**: Beyond detection, actively negotiate feature sets
2. **Dynamic Capabilities**: Update capabilities during connection lifetime
3. **Semantic Versioning**: Use semver for software versions
4. **Deprecation Warnings**: Log when using deprecated features
5. **Version-Specific Handlers**: Route messages to version-specific handlers

## Examples

### Example 1: Basic Version Negotiation

```rust
// Node A (running v1)
let local_info = VersionInfo {
    current_version: 1,
    min_supported: 1,
    max_supported: 1,
    capabilities: CapabilityFlags::SIGNED_MESSAGES | CapabilityFlags::GRACEFUL_RESTART,
    software_version: "icnd-0.1.0".to_string(),
};

// Node B (running v2, backward compatible with v1)
let remote_info = VersionInfo {
    current_version: 2,
    min_supported: 1,
    max_supported: 2,
    capabilities: CapabilityFlags::all(), // Supports everything
    software_version: "icnd-0.2.0".to_string(),
};

// Negotiation succeeds at version 1
let negotiated = negotiate_version(&local_info, &remote_info)?;
assert_eq!(negotiated, 1);
```

### Example 2: Incompatible Versions

```rust
// Old node
let old = VersionInfo {
    current_version: 1,
    min_supported: 1,
    max_supported: 1,
    ...
};

// New node that dropped v1 support
let new = VersionInfo {
    current_version: 3,
    min_supported: 2,  // No longer supports v1
    max_supported: 3,
    ...
};

// Negotiation fails - no overlap
let result = negotiate_version(&old, &new);
assert!(result.is_err());
// Error: "No compatible version. Local: [1-1], Remote: [2-3]"
```

### Example 3: Capability-Based Feature Use

```rust
// Check if peer supports encryption before sending
if peer_caps.contains(CapabilityFlags::E2E_ENCRYPTION) {
    // Use encrypted channel
    let encrypted_msg = EncryptedEnvelope::encrypt(&peer_key, payload)?;
    network.send(peer_did, MessagePayload::Encrypted(encrypted_msg))?;
} else {
    // Fall back to signed-only
    let signed_msg = SignedEnvelope::sign(&my_key, payload)?;
    network.send(peer_did, MessagePayload::Signed(signed_msg))?;
}
```

## Implementation Checklist

- [ ] Define `VersionInfo` and `CapabilityFlags` structs
- [ ] Add version info to `MessagePayload::Hello`
- [ ] Implement `negotiate_version()` function
- [ ] Track negotiated version per connection in NetworkActor
- [ ] Add capability checking helpers
- [ ] Update Hello message creation/handling
- [ ] Add version negotiation metrics
- [ ] Write unit tests for negotiation algorithm
- [ ] Write integration tests for version scenarios
- [ ] Document migration guide for future versions
- [ ] Add logging for version mismatches

## References

- [Network Protocol](../crates/icn-net/src/protocol.rs) - Current protocol implementation
- [Hello Handshake](../crates/icn-net/src/actor.rs) - Connection establishment
- [Protocol Versioning](../docs/production-hardening.md#protocol-versioning) - Existing validation
