# ICN API Versioning Guide

**Document**: ICN-DOC-VERSIONING-01
**Last Updated**: 2025-12-04
**Status**: Complete

This guide covers versioning strategies for all ICN interfaces: the REST Gateway API, RPC protocol, and P2P network protocol.

---

## Table of Contents

1. [Overview](#1-overview)
2. [Gateway REST API Versioning](#2-gateway-rest-api-versioning)
3. [RPC Protocol Versioning](#3-rpc-protocol-versioning)
4. [P2P Protocol Versioning](#4-p2p-protocol-versioning)
5. [Capability-Based Features](#5-capability-based-features)
6. [Migration Strategies](#6-migration-strategies)
7. [Deprecation Policy](#7-deprecation-policy)
8. [Version Compatibility Matrix](#8-version-compatibility-matrix)

---

## 1. Overview

ICN uses semantic versioning (SemVer) for releases and maintains three separate versioned interfaces:

| Interface | Current Version | Scope |
|-----------|-----------------|-------|
| Gateway REST API | v1 | External applications |
| RPC Protocol | 1.0 | Node-to-node admin |
| P2P Protocol | 1 | Network layer |

**Versioning Philosophy**:
- **Backward compatibility** is prioritized for all interfaces
- **Breaking changes** require major version bumps
- **Additive changes** (new endpoints, fields) are non-breaking
- **Deprecation** precedes removal by at least 2 minor versions

---

## 2. Gateway REST API Versioning

### 2.1 URL Path Versioning

All Gateway API endpoints use URL path versioning with the `/v1/` prefix:

```
http://localhost:8080/v1/coops
http://localhost:8080/v1/ledger/{coop_id}/balance/{did}
http://localhost:8080/v1/compute/submit
```

**Why path versioning?**
- Clear and explicit in requests
- Easy to route at load balancer level
- Simple to test with curl/httpie

### 2.2 Version Lifecycle

| State | Description | Duration |
|-------|-------------|----------|
| **Current** | Actively developed, full support | Until next major |
| **Supported** | Bug fixes, security patches only | 12 months after deprecation |
| **Deprecated** | Scheduled for removal | 6 months notice |
| **Removed** | No longer available | - |

### 2.3 Adding New API Version

When adding `/v2/` endpoints:

1. **Create new API module**:
   ```rust
   // icn-gateway/src/api/v2/mod.rs
   pub mod coops;
   pub mod ledger;
   pub mod compute;
   ```

2. **Register both versions**:
   ```rust
   // server.rs
   App::new()
       .service(web::scope("/v1").configure(v1::config))
       .service(web::scope("/v2").configure(v2::config))
   ```

3. **Maintain parallel implementations** during transition period

### 2.4 Response Format Changes

**Non-breaking additions** (allowed within same version):
- New optional fields in responses
- New optional query parameters
- New endpoints

**Breaking changes** (require version bump):
- Removing fields from responses
- Changing field types
- Changing endpoint paths
- Removing endpoints

**Example: Adding a field**
```json
// v1 original response
{
  "id": "coop-123",
  "name": "Food Coop"
}

// v1 updated response (backward compatible)
{
  "id": "coop-123",
  "name": "Food Coop",
  "created_at": "2025-01-15T10:30:00Z"  // New optional field
}
```

### 2.5 Request Validation

New **optional** request fields can be added without breaking clients:

```json
// v1 original request
{
  "id": "coop-123",
  "name": "Food Coop"
}

// v1 updated - server accepts both
{
  "id": "coop-123",
  "name": "Food Coop",
  "description": "A local food cooperative"  // Optional, ignored if missing
}
```

---

## 3. RPC Protocol Versioning

### 3.1 JSON-RPC Method Naming

RPC methods follow the pattern: `namespace.method`:

```json
{
  "jsonrpc": "2.0",
  "method": "network.peers",
  "params": {},
  "id": 1
}
```

**Current namespaces**:
- `auth.*` - Authentication methods
- `network.*` - Network operations
- `gossip.*` - Gossip protocol
- `ledger.*` - Ledger operations
- `compute.*` - Compute tasks
- `governance.*` - Governance domains/proposals
- `identity.*` - Identity management

### 3.2 Adding RPC Methods

New methods can be added without breaking existing clients:

```rust
// Add new method in server.rs
"compute.cancel" => self.handle_compute_cancel(params).await,
```

### 3.3 Evolving Method Parameters

**Non-breaking** (add with defaults):
```json
// Before
{"method": "compute.submit", "params": {"code": "...", "fuel_limit": 1000}}

// After (priority is optional, defaults to "normal")
{"method": "compute.submit", "params": {"code": "...", "fuel_limit": 1000, "priority": "high"}}
```

**Breaking** (requires new method):
```json
// Don't change semantics - create new method instead
{"method": "compute.submit_v2", "params": {...}}
```

### 3.4 RPC Error Codes

Standard JSON-RPC error codes are used:

| Code | Meaning |
|------|---------|
| -32700 | Parse error |
| -32600 | Invalid request |
| -32601 | Method not found |
| -32602 | Invalid params |
| -32603 | Internal error |
| -32001 | Authentication required |
| -32002 | Authorization denied |
| -32003 | Rate limited |

Custom error codes use -32100 to -32199 range.

---

## 4. P2P Protocol Versioning

### 4.1 Protocol Version Negotiation

ICN nodes negotiate protocol versions during the Hello handshake:

```rust
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
```

**Constants** (defined in `icn-net/src/protocol.rs`):
```rust
pub const PROTOCOL_VERSION: u32 = 1;      // Current
pub const MIN_SUPPORTED_VERSION: u32 = 1; // Minimum we accept
pub const MAX_SUPPORTED_VERSION: u32 = 1; // Maximum we support
```

### 4.2 Negotiation Algorithm

Nodes negotiate to the highest mutually supported version:

```rust
fn negotiate_version(local: &VersionInfo, remote: &VersionInfo) -> Result<u32> {
    // Find the overlap between supported versions
    let negotiated = std::cmp::min(local.max_supported, remote.max_supported);

    // Verify it's within both nodes' supported range
    if negotiated < local.min_supported || negotiated < remote.min_supported {
        bail!("No compatible version");
    }

    Ok(negotiated)
}
```

**Example**:
```
Node A: min=1, max=2, current=2
Node B: min=1, max=1, current=1

Negotiated: min(2, 1) = 1 ✓ (both support v1)
```

### 4.3 Handshake Sequence

```
Client                                Server
   |                                     |
   |---- Hello + VersionInfo ----------->|
   |                                     | (validate version)
   |<--- Hello + VersionInfo ------------|
   |                                     |
   | (Both nodes compute negotiated version)
   |                                     |
   |<=========== Connection OK ==========>|
```

### 4.4 Incrementing Protocol Version

When to increment the protocol version:

| Change Type | Action |
|-------------|--------|
| New message type | No change (additive) |
| New field in message | No change (additive) |
| Remove field | Increment version |
| Change field semantics | Increment version |
| Change message format | Increment version |

**Steps to increment**:

1. Update constants:
   ```rust
   pub const PROTOCOL_VERSION: u32 = 2;
   pub const MAX_SUPPORTED_VERSION: u32 = 2;
   // Keep MIN_SUPPORTED = 1 for backward compat
   ```

2. Handle both versions in message parsing:
   ```rust
   match negotiated_version {
       1 => parse_v1_message(bytes),
       2 => parse_v2_message(bytes),
       _ => Err("Unsupported version"),
   }
   ```

3. Update tests and documentation

---

## 5. Capability-Based Features

### 5.1 Capability Flags

Beyond version numbers, ICN uses capability flags for fine-grained feature detection:

```rust
pub struct CapabilityFlags: u64 {
    const E2E_ENCRYPTION     = 0b00000001;  // Phase 10
    const SIGNED_MESSAGES    = 0b00000010;  // Phase 9
    const GRACEFUL_RESTART   = 0b00000100;  // Track B1
    const TOPOLOGY_AWARE     = 0b00001000;  // Phase 16C
    const TRUST_RATE_LIMITING = 0b00010000; // Phase 7
    const GOSSIP_PULL        = 0b00100000;  // Phase 7
    const MULTI_DEVICE       = 0b01000000;  // Phase 11
    const ECONOMIC_SAFETY    = 0b10000000;  // Phase 12
}
```

### 5.2 Feature Detection Pattern

Check peer capabilities before using optional features:

```rust
let common = common_capabilities(&local_version, &peer_version);

if common.contains(CapabilityFlags::E2E_ENCRYPTION) {
    // Use encrypted envelope
    send_encrypted_message(peer, payload);
} else {
    // Fall back to signed-only envelope
    send_signed_message(peer, payload);
}
```

### 5.3 Adding New Capabilities

1. Add flag constant:
   ```rust
   const NEW_FEATURE = 0b100000000; // Next available bit
   ```

2. Update `current()` when feature is implemented:
   ```rust
   impl CapabilityFlags {
       pub fn current() -> Self {
           Self::E2E_ENCRYPTION
               | Self::SIGNED_MESSAGES
               | ...
               | Self::NEW_FEATURE  // Add when ready
       }
   }
   ```

3. Document the capability in this guide

---

## 6. Migration Strategies

### 6.1 Rolling Upgrades

For networks with mixed versions:

1. **Upgrade executors first** - They need newest features
2. **Upgrade coordinators** - Intermediate nodes
3. **Upgrade leaf nodes last** - Members, gateways

During rolling upgrade:
- Maintain backward compatibility window (e.g., support N and N-1)
- Monitor metrics for version distribution
- Set deadline for upgrade completion

### 6.2 Data Migration

When storage formats change:

```bash
# Check current version
icnctl version --data-dir ~/.icn

# Run migration
icnctl migrate --from 0.1.x --to 0.2.x

# Verify migration
icnctl verify --data-dir ~/.icn
```

**Migration principles**:
- Always backup before migration
- Migrations are one-way (no downgrade path)
- Test migrations on copy first

### 6.3 API Migration Checklist

For Gateway API version transitions:

- [ ] Create new version endpoints
- [ ] Update OpenAPI spec
- [ ] Update TypeScript SDK
- [ ] Add deprecation warnings to old endpoints
- [ ] Update documentation
- [ ] Communicate timeline to API consumers
- [ ] Monitor usage of deprecated endpoints
- [ ] Remove old endpoints after sunset period

---

## 7. Deprecation Policy

### 7.1 Deprecation Process

1. **Announce** - Document deprecation in CHANGELOG, add header to responses
2. **Warn** - Log warnings when deprecated features used
3. **Monitor** - Track usage of deprecated features
4. **Remove** - After sunset period, remove feature

### 7.2 Deprecation Signals

**REST API**: Add response header
```http
Deprecation: Sun, 01 Jun 2025 00:00:00 GMT
Sunset: Sun, 01 Dec 2025 00:00:00 GMT
Link: </v2/coops>; rel="successor-version"
```

**RPC**: Return warning in response
```json
{
  "jsonrpc": "2.0",
  "result": {...},
  "id": 1,
  "_warning": "Method 'compute.submit' is deprecated. Use 'compute.submit_v2' instead."
}
```

**P2P**: Log warning, emit metric
```rust
metrics::counter!("icn_protocol_deprecated_messages_total").increment(1);
tracing::warn!(
    method = %msg.method,
    peer = %peer_did,
    "Received deprecated message type"
);
```

### 7.3 Timeline Guidelines

| Severity | Notice Period | Support Period |
|----------|---------------|----------------|
| Minor (field removal) | 1 minor release | 2 minor releases |
| Major (endpoint removal) | 2 minor releases | 6 months |
| Critical (security fix) | Immediate | Best effort |

---

## 8. Version Compatibility Matrix

### 8.1 Current Compatibility

| ICN Version | Protocol | Gateway API | RPC | Min Compatible |
|-------------|----------|-------------|-----|----------------|
| 0.1.x | 1 | v1 | 1.0 | 0.1.0 |
| 0.2.x (planned) | 1-2 | v1 | 1.0 | 0.1.0 |

### 8.2 Feature Availability by Version

| Feature | Introduced | Protocol | Capability Flag |
|---------|------------|----------|-----------------|
| Basic networking | 0.1.0 | 1 | - |
| Signed messages | 0.1.0 | 1 | SIGNED_MESSAGES |
| E2E encryption | 0.1.0 | 1 | E2E_ENCRYPTION |
| Graceful restart | 0.1.0 | 1 | GRACEFUL_RESTART |
| Trust rate limiting | 0.1.0 | 1 | TRUST_RATE_LIMITING |
| Multi-device | 0.1.0 | 1 | MULTI_DEVICE |
| Economic safety | 0.1.0 | 1 | ECONOMIC_SAFETY |
| Topology awareness | 0.1.0 | 1 | TOPOLOGY_AWARE |

### 8.3 Testing Compatibility

Run compatibility tests before deployment:

```bash
# Test against older version
./scripts/test-compatibility.sh --old-version 0.1.0 --new-version 0.2.0

# Test protocol negotiation
cargo test -p icn-net test_negotiate

# Test API backward compatibility
cargo test -p icn-gateway test_v1_compat
```

---

## References

- [ARCHITECTURE.md](ARCHITECTURE.md) - System architecture
- [capability-based-features.md](capability-based-features.md) - Detailed capability guide
- [deployment-guide.md](deployment-guide.md) - Deployment procedures
- [CHANGELOG.md](../CHANGELOG.md) - Version history

---

## Changelog

| Date | Change |
|------|--------|
| 2025-12-04 | Initial document creation |
