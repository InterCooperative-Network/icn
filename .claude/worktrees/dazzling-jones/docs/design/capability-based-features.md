# Capability-Based Feature Gating

This document explains how to use ICN's capability-based feature gating system to build protocols that gracefully handle peers running different software versions.

## Overview

ICN implements protocol version negotiation during the Hello handshake. Each node announces:
- Its supported protocol version range (min, current, max)
- A bitmap of feature capabilities it supports
- Its software version string

The network layer automatically negotiates the highest mutually-supported protocol version and calculates the intersection of capabilities. This information is then available for application-level feature gating.

## Capability Flags

All capabilities are defined in [`icn-net/src/version.rs`](../../icn/crates/icn-net/src/version.rs):

```rust
pub struct CapabilityFlags: u64 {
    const E2E_ENCRYPTION = 0b00000001;       // Phase 10: End-to-end encryption
    const SIGNED_MESSAGES = 0b00000010;      // Phase 9: Message signing
    const GRACEFUL_RESTART = 0b00000100;     // State persistence
    const TOPOLOGY_AWARE = 0b00001000;       // Topology-aware routing
    const TRUST_RATE_LIMITING = 0b00010000;  // Trust-based rate limits
    const GOSSIP_PULL = 0b00100000;          // Gossip pull protocol
    const MULTI_DEVICE = 0b01000000;         // Phase 11: Multi-device identity
    const ECONOMIC_SAFETY = 0b10000000;      // Phase 12: Economic safety rails
}
```

## Checking Peer Capabilities

The `NetworkHandle` provides several methods for capability checking:

### 1. Check if a Peer Supports a Capability

```rust
if network_handle.peer_has_capability(&peer_did, CapabilityFlags::E2E_ENCRYPTION).await {
    // Peer supports encrypted communication
    network_handle.send_encrypted_message(/* ... */).await?;
} else {
    // Fall back to signed-only communication
    network_handle.send_message(peer_did, signed_msg).await?;
}
```

### 2. Get All Peers With a Capability

```rust
// Broadcast encrypted messages only to capable peers
let encrypted_peers = network_handle.get_peers_with_capability(
    CapabilityFlags::E2E_ENCRYPTION
).await;

for peer in encrypted_peers {
    network_handle.send_encrypted_message(&peer, /* ... */).await?;
}
```

### 3. Get Peer's Protocol Version

```rust
if let Some(version) = network_handle.get_peer_protocol_version(&peer_did).await {
    if version >= 2 {
        // Use protocol v2 features
    } else {
        // Fall back to v1 behavior
    }
}
```

### 4. Get Full Connection Info

```rust
if let Some(info) = network_handle.get_peer_connection_info(&peer_did).await {
    println!("Peer: {}", info.did);
    println!("Version: {}", info.negotiated_version);
    println!("Software: {}", info.peer_software);
    println!("Capabilities: {:?}", info.peer_capabilities.describe());
}
```

## Feature Gating Patterns

### Pattern 1: Graceful Degradation

Use newer features when available, fall back to basic functionality otherwise:

```rust
async fn send_sensitive_data(
    network: &NetworkHandle,
    recipient: &Did,
    data: &[u8],
) -> Result<()> {
    if network.peer_has_capability(recipient, CapabilityFlags::E2E_ENCRYPTION).await {
        // Best: End-to-end encrypted
        network.send_encrypted_message(
            recipient,
            keypair,
            x25519_secret,
            sequence,
            data
        ).await?;
    } else if network.peer_has_capability(recipient, CapabilityFlags::SIGNED_MESSAGES).await {
        // Good: Signed for authenticity
        let signed = SignedEnvelope::new(/* ... */)?;
        network.send_message(recipient.clone(), NetworkMessage::signed(/* ... */)).await?;
    } else {
        // Basic: No extra security (legacy peer)
        warn!("Peer {} lacks modern security features", recipient);
        network.send_message(recipient.clone(), NetworkMessage::new(/* ... */)).await?;
    }
    Ok(())
}
```

### Pattern 2: Require Capability

Reject operations if the peer doesn't support required features:

```rust
async fn request_contract_execution(
    network: &NetworkHandle,
    executor: &Did,
    contract: &Contract,
) -> Result<()> {
    // Economic safety required for contract execution
    if !network.peer_has_capability(executor, CapabilityFlags::ECONOMIC_SAFETY).await {
        anyhow::bail!(
            "Peer {} does not support ECONOMIC_SAFETY - cannot execute contracts",
            executor
        );
    }

    // Proceed with contract execution request...
    Ok(())
}
```

### Pattern 3: Broadcast to Capable Peers Only

Send feature-specific broadcasts only to peers that support them:

```rust
async fn broadcast_topology_update(
    network: &NetworkHandle,
    update: TopologyUpdate,
) -> Result<()> {
    let topology_peers = network.get_peers_with_capability(
        CapabilityFlags::TOPOLOGY_AWARE
    ).await;

    if topology_peers.is_empty() {
        info!("No topology-aware peers connected");
        return Ok(());
    }

    for peer in topology_peers {
        network.send_message(peer, update.clone().into()).await?;
    }

    Ok(())
}
```

### Pattern 4: Version-Based Protocol Changes

Use protocol version for wire-format changes:

```rust
async fn sync_ledger_entries(
    network: &NetworkHandle,
    peer: &Did,
    entries: &[Entry],
) -> Result<()> {
    let version = network.get_peer_protocol_version(peer).await.unwrap_or(1);

    let message = if version >= 2 {
        // Protocol v2: Compressed entries
        LedgerSyncMessage::V2 {
            entries: compress_entries(entries)?,
        }
    } else {
        // Protocol v1: Uncompressed
        LedgerSyncMessage::V1 {
            entries: entries.to_vec(),
        }
    };

    network.send_message(peer.clone(), message.into()).await?;
    Ok(())
}
```

## Adding New Capabilities

When adding a new feature that requires capability detection:

### 1. Define the Capability Flag

Edit [`icn-net/src/version.rs`](../../icn/crates/icn-net/src/version.rs):

```rust
pub struct CapabilityFlags: u64 {
    // ... existing flags ...

    /// Supports quantum-resistant signatures (Phase 15)
    const QUANTUM_RESISTANT = 0b100000000;
}
```

### 2. Add to Current Capabilities

Update `CapabilityFlags::current()` if the feature is implemented:

```rust
pub fn current() -> Self {
    Self::E2E_ENCRYPTION
        | Self::SIGNED_MESSAGES
        | Self::GRACEFUL_RESTART
        | Self::TOPOLOGY_AWARE
        | Self::TRUST_RATE_LIMITING
        | Self::GOSSIP_PULL
        | Self::MULTI_DEVICE
        | Self::ECONOMIC_SAFETY
        | Self::QUANTUM_RESISTANT  // Add new capability
}
```

### 3. Add to Description Method

Update `CapabilityFlags::describe()` for human-readable output:

```rust
pub fn describe(&self) -> Vec<&'static str> {
    let mut features = Vec::new();
    // ... existing checks ...
    if self.contains(Self::QUANTUM_RESISTANT) {
        features.push("quantum_resistant");
    }
    features
}
```

### 4. Use in Your Code

```rust
if network.peer_has_capability(&peer, CapabilityFlags::QUANTUM_RESISTANT).await {
    // Use quantum-resistant signatures
} else {
    // Fall back to Ed25519
}
```

## Backward Compatibility

### Legacy Nodes

Nodes that don't send `version_info` in Hello messages are treated as:
- Protocol version: 1
- Capabilities: empty set
- Software: "legacy-node"

Your code should handle this gracefully:

```rust
let caps = network.get_peer_connection_info(&peer).await
    .map(|info| info.peer_capabilities)
    .unwrap_or(CapabilityFlags::empty());

if caps.is_empty() {
    // Legacy peer - use basic protocol
} else {
    // Modern peer - check specific capabilities
}
```

### Capability Negotiation

The network layer automatically calculates the intersection of capabilities:

```
Local capabilities:  E2E_ENCRYPTION | SIGNED_MESSAGES | GRACEFUL_RESTART
Remote capabilities: E2E_ENCRYPTION | SIGNED_MESSAGES | TOPOLOGY_AWARE
                     ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Common capabilities: E2E_ENCRYPTION | SIGNED_MESSAGES
```

Only use features that appear in the common set.

## Testing Capability-Based Code

### Unit Tests

```rust
#[tokio::test]
async fn test_capability_based_routing() {
    let network = create_test_network().await;

    // Mock peer with encryption support
    let encrypted_peer = Did::from_str("did:icn:alice")?;
    mock_peer_capabilities(
        &network,
        &encrypted_peer,
        CapabilityFlags::E2E_ENCRYPTION
    ).await;

    // Mock peer without encryption
    let legacy_peer = Did::from_str("did:icn:bob")?;
    mock_peer_capabilities(
        &network,
        &legacy_peer,
        CapabilityFlags::empty()
    ).await;

    // Should send encrypted to alice
    assert!(network.peer_has_capability(&encrypted_peer, CapabilityFlags::E2E_ENCRYPTION).await);

    // Should fall back to signed for bob
    assert!(!network.peer_has_capability(&legacy_peer, CapabilityFlags::E2E_ENCRYPTION).await);
}
```

### Integration Tests

Test mixed-version networks:

```rust
#[tokio::test]
async fn test_mixed_version_network() {
    // Spawn v1 node
    let node_v1 = spawn_node_with_version(1, CapabilityFlags::SIGNED_MESSAGES).await;

    // Spawn v2 node
    let node_v2 = spawn_node_with_version(2, CapabilityFlags::E2E_ENCRYPTION | CapabilityFlags::SIGNED_MESSAGES).await;

    // Connect them
    node_v2.dial(&node_v1.addr, &node_v1.did).await?;

    // Wait for Hello exchange
    tokio::time::sleep(Duration::from_millis(100)).await;

    // v2 should detect v1's limited capabilities
    let v1_caps = node_v2.network.get_peer_connection_info(&node_v1.did).await.unwrap();
    assert_eq!(v1_caps.negotiated_version, 1);
    assert_eq!(v1_caps.peer_capabilities, CapabilityFlags::SIGNED_MESSAGES);
}
```

## Metrics

The system exports Prometheus metrics for monitoring version distribution:

- `icn_network_peer_versions{version="1"}` - Number of peers at each protocol version
- `icn_network_peer_capabilities{capability="e2e_encryption"}` - Number of peers with each capability
- `icn_network_version_negotiation_success_total{negotiated_version="1"}` - Successful negotiations
- `icn_network_version_negotiation_failures_total{reason="incompatible_version"}` - Failed negotiations

## Best Practices

1. **Always check capabilities before using optional features**
   - Don't assume all peers support modern features
   - Provide fallback behavior for legacy peers

2. **Log capability mismatches**
   - Helps diagnose version rollout issues
   - Use `info!()` for expected behavior, `warn!()` for degraded mode

3. **Test with mixed versions**
   - Write integration tests with old and new nodes
   - Verify graceful degradation paths work

4. **Document capability requirements**
   - Clearly state which capabilities a feature requires
   - Include examples of fallback behavior

5. **Use semantic versioning**
   - Protocol version for breaking wire-format changes
   - Capabilities for optional features
   - Software version for informational purposes

6. **Monitor version distribution**
   - Track metrics to understand deployment progress
   - Alert if too many peers lack critical capabilities

## Example: Gossip Protocol

The gossip protocol demonstrates capability-based feature gating:

```rust
async fn propagate_entry(
    network: &NetworkHandle,
    entry: &Entry,
    topic: &str,
) -> Result<()> {
    // Get all peers subscribed to this topic
    let subscribers = get_topic_subscribers(topic).await;

    for peer in subscribers {
        // Check if peer supports pull protocol
        if network.peer_has_capability(&peer, CapabilityFlags::GOSSIP_PULL).await {
            // Just send announcement hash (peer can pull if interested)
            send_announcement(&network, &peer, entry.hash()).await?;
        } else {
            // Legacy: Send full entry
            send_full_entry(&network, &peer, entry).await?;
        }
    }

    Ok(())
}
```

## Troubleshooting

### Peer Capability Check Returns False

**Possible causes:**
1. Peer hasn't completed Hello handshake yet
2. Peer is a legacy node
3. Peer's software doesn't support the capability

**Solution:**
```rust
if let Some(info) = network.get_peer_connection_info(&peer).await {
    println!("Peer capabilities: {:?}", info.peer_capabilities.describe());
    println!("Peer software: {}", info.peer_software);
} else {
    println!("No connection info for peer {} - handshake not complete", peer);
}
```

### Version Negotiation Fails

**Error message:** `"No compatible version. Local: [1-2], Remote: [3-4]"`

**Cause:** No overlap in supported version ranges

**Solution:**
- Check PROTOCOL_VERSION, MIN_SUPPORTED_VERSION, MAX_SUPPORTED_VERSION
- Ensure gradual version rollout (keep backward compatibility window)
- Don't drop old version support until all nodes upgraded

### Capability Set Is Empty

**Possible causes:**
1. Talking to a legacy node
2. Peer capabilities genuinely empty (very old version)

**Solution:** Handle empty capabilities gracefully with fallback behavior

## See Also

- [Protocol Version Constants](../../icn/crates/icn-net/src/protocol.rs) - Version numbers
- [Version Negotiation Logic](../../icn/crates/icn-net/src/version.rs) - Negotiation algorithm
- [Network Actor](../../icn/crates/icn-net/src/actor/mod.rs) - Hello handshake implementation
- [Prometheus Metrics](../../icn/crates/icn-obs/src/metrics/mod.rs) - Monitoring
