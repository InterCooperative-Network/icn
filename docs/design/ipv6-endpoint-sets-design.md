# IPv6 Dual-Stack Transport and Endpoint Sets Design

**Status**: Accepted — Phases 21-22
**Priority**: Tier 1 — Network Connectivity
**Supersedes**: Portions of `nat-traversal-design.md` (connection strategy section)

---

## Problem Statement

ICN currently models each peer as having exactly one network address (`SocketAddr`). This creates three
compounding problems:

1. **NAT fragility**: A peer's single "best" address is a guess. When it's wrong, you fall back to relay
   with no middle ground.
2. **IPv4-only default**: All bind defaults use `0.0.0.0`, blocking adoption of IPv6-native infrastructure.
3. **No overlay routing**: Cooperatives and federations operating on private meshes (WireGuard, site VPNs)
   have no way to signal their preferred internal path.

## Goals

- Every peer advertises a **typed set** of reachable endpoints (ULA, global v6, public v4, private v4, relay)
- Connection dialing uses a **Happy Eyeballs** strategy: try best candidates in priority order with staggered
  start, first success wins
- All existing IPv4 behavior is preserved — no adoption blocking
- IPv6 is preferred when available, but never required
- ULA addresses (`fd::/8`) provide a clean internal routing fabric for federation overlays
- Identity remains DID-only — IP addresses are transport hints, never identity or authority

## Non-Goals

- Full overlay routing protocol (route exchange, path vector, BGP-like)
- Encoding governance or trust into IP addresses
- Requiring nodes to have IPv6 connectivity
- Changing the DID / capability / governance layers

---

## Architecture

### Core Type: `EndpointCandidate`

```rust
/// Classification of a network endpoint by reachability scope
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EndpointType {
    /// ULA IPv6 (fd::/8) — internal overlay, highest priority within same mesh
    UlaV6,
    /// Globally routable IPv6 (2000::/3)
    GlobalV6,
    /// Public IPv4 (internet-reachable, confirmed by STUN)
    PublicV4,
    /// Private IPv4 (RFC1918 — useful within LAN)
    PrivateV4,
    /// TURN relay fallback (last resort)
    Relay,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EndpointCandidate {
    pub endpoint_type: EndpointType,
    pub addr: SocketAddr,
    /// Lower = preferred. Defaults: UlaV6=10, GlobalV6=20, PrivateV4=30, PublicV4=40, Relay=100
    pub priority: u16,
}
```

`EndpointType` is auto-derivable from `SocketAddr`:
- `fd::/8` → `UlaV6`
- `2000::/3` → `GlobalV6`
- `10/8`, `172.16/12`, `192.168/16` → `PrivateV4`
- Other v4 → `PublicV4`
- Relay addresses require explicit tagging

### Updated `ConnectionCandidate`

```rust
pub struct ConnectionCandidate {
    pub did: Did,
    pub endpoints: Vec<EndpointCandidate>,  // sorted by priority
    pub timestamp: u64,
    pub version: u8,  // bumped to 2

    // Backward compat — populated for old nodes, deprecated
    #[serde(default)] pub local_addr: Option<SocketAddr>,
    #[serde(default)] pub public_addr: Option<SocketAddr>,
    #[serde(default)] pub relay_addr: Option<SocketAddr>,
}
```

`ConnectionCandidate` uses JSON serialization (gossip topic `network:candidates`). The `#[serde(default)]`
on old fields means old nodes silently ignore `endpoints`; new nodes prefer `endpoints` when present.

---

## Happy Eyeballs Dialer

Replaces the current `dial_with_fallback()` in `nat_dial.rs`.

**Algorithm:**

1. Sort endpoints by `priority` (ascending)
2. Start attempt for highest-priority endpoint
3. After `happy_eyeballs_delay_ms` (default: 250ms), start next endpoint attempt
4. Continue staggering until all endpoints are in flight or one succeeds
5. First successful connection wins; cancel all others
6. If all fail, return aggregate error

**Config additions to `NatDialConfig`:**

```toml
[network.nat]
ula_v6_dial_timeout_ms = 1000
global_v6_dial_timeout_ms = 5000
happy_eyeballs_delay_ms = 250
# existing fields preserved:
local_dial_timeout_ms = 2000
public_dial_timeout_ms = 5000
relay_dial_timeout_ms = 30000
```

**DID-keyed connection map**: The session map is already keyed by DID. When multiple candidates are
racing, a "already connected" check prevents duplicate connections to the same peer.

---

## Wire Protocol Compatibility

### Capability Flags

Add `ENDPOINT_SETS = 0b1_000_000_000_000` to the existing `CapabilityFlags` bitflags in `version.rs`.
Peers advertise this flag in the Hello handshake. Both sides check for the flag before sending typed
endpoint data in `KnownPeer`.

### Protocol Version

Bump `MAX_SUPPORTED_VERSION` to 2. Version 1 (current) nodes remain fully supported. Version negotiation
in the existing Hello handshake handles the transition.

### `KnownPeer` in `PeerExchangeMessage`

`KnownPeer` uses **postcard** (positional encoding). Add new fields **only at the end**, with
`#[serde(default)]`:

```rust
pub struct KnownPeer {
    // existing fields unchanged ...
    pub did: String,
    pub addresses: Vec<String>,
    pub version: String,
    pub network_name: Option<String>,
    pub observed_trust: Option<f64>,
    pub last_seen: u64,
    pub is_local: bool,
    // NEW — only sent when both peers have ENDPOINT_SETS capability:
    #[serde(default)]
    pub endpoints: Option<Vec<EndpointCandidate>>,
}
```

**Warning**: postcard uses positional encoding. The capability check gates sending the new field.
Old nodes that receive the new field will see trailing bytes; this is handled by their version
rejection path.

---

## Storage (`CachedPeer`)

`CachedPeer` is JSON-serialized in sled. Add with `#[serde(default)]`:

```rust
pub struct CachedPeer {
    // existing fields unchanged ...
    #[serde(default)]
    pub endpoints: Option<Vec<EndpointCandidate>>,
}
```

Old sled records deserialize cleanly (`endpoints = None`). No migration needed. Old `address` and
`public_address` fields are kept until a future cleanup PR removes them.

---

## mDNS Multi-Address

`discovery.rs` currently takes `iter().next()` from mDNS resolution results. Change to collect all
returned addresses and emit one `PeerInfo` per address (or aggregate into a `Vec<SocketAddr>`).
On IPv6-capable networks, mDNS naturally returns both v4 and v6 addresses for the same peer.

---

## TURN Relay Proxy IPv6

`relay_proxy.rs` has three explicit IPv6 bail-outs and an IPv4-only `xor_encode_address_v4()`.

Changes needed:
- Implement `xor_encode_address_v6()` following RFC 5766 §10.2 (128-bit XOR with magic + transaction ID)
- Update `build_send_indication()` to dispatch on address family
- Update socket binding to match the peer's address family
- Remove the three `bail!("IPv6 peer relay addresses are not yet supported")` guards

The TURN client (`turn.rs`) already has correct IPv6 XOR decode logic — the proxy just needs to match it.

---

## Default Config Changes

| Setting | Current default | New default |
|---------|----------------|-------------|
| `network.listen_addr` | `0.0.0.0:7777` | `[::]:7777` |
| `gateway.bind_addr` | `127.0.0.1:8080` | `[::1]:8080` |
| RPC bind | `127.0.0.1:<port>` | `[::1]:<port>` |
| Health server | `0.0.0.0:<port>` | `[::]:<port>` |
| Metrics server | `0.0.0.0:<port>` | `[::]:<port>` |

CORS trusted origins (`security.rs`) add `http://[::1]:*` variants alongside `http://127.0.0.1:*`.

---

## ULA Overlay Convention (Phase C — deferred)

When federations operate a shared mesh (WireGuard, site tunnels), they can assign ULA prefixes:

```
fd<org-hash-6bytes>::/48   — per organization
  :<site-id>::/64          — per site within org
    :<subnet-id>::/80      — per segment
```

Nodes on the same mesh include their ULA address in `endpoints` with type `UlaV6`. The endpoint
preference rules automatically prefer ULA over global v6 when it's reachable.

**Invariants:**
- ULA = topology routing hint only, not identity
- Endpoint sharing is trust-gated (peers only learn ULA candidates from sufficiently trusted nodes)
- No ULA address is ever treated as authority for governance or capability decisions

---

## Implementation Phases

### Phase 0 — Config defaults (1-2 days)
- Change all bind defaults from `0.0.0.0` / `127.0.0.1` to `[::]` / `[::1]`
- Add `[::1]` CORS variants
- Update deploy configs (TOML, k8s configmap, devnet)
- **No behavior change for IPv4-only deployments**

### Phase A — EndpointCandidate type (1 week)
- Add `EndpointType`, `EndpointCandidate` to `icn-net/src/candidate.rs`
- Add `EndpointType::from_socket_addr()` classifier
- Refactor `ConnectionCandidate` to carry `endpoints: Vec<EndpointCandidate>`
- Update `connection_candidate()` in `session.rs` to build endpoint vec
- Update `CachedPeer` with `endpoints: Option<Vec<EndpointCandidate>>`
- Update mDNS discovery to collect all addresses
- Add `ENDPOINT_SETS` capability flag

### Phase B — Happy Eyeballs dialer (1 week)
- Refactor `dial_with_fallback()` → `dial_happy_eyeballs(endpoints: Vec<EndpointCandidate>)`
- Update `NetworkMsg::Dial` to carry endpoints
- Update `handle_dial()` in actor
- Update `KnownPeer` with optional `endpoints` field (capability-gated)
- Add per-type timeout config to `NatDialConfig`
- Update bootstrap peer parsing for multi-address support

### Phase C — TURN relay IPv6 (1-2 days)
- Implement `xor_encode_address_v6()` in `relay_proxy.rs`
- Update `build_send_indication()` for both address families
- Update socket binding to be address-family-aware
- Remove IPv6 bail-outs
- Add IPv6 relay integration tests

---

## Success Criteria

- [ ] Two nodes on a dual-stack network connect via IPv6 by default
- [ ] IPv4-only nodes continue to work without configuration
- [ ] A peer with both ULA and public IPv4 prefers ULA when on the same mesh
- [ ] If IPv6 connection fails, fallback to IPv4 completes within `happy_eyeballs_delay_ms * n_candidates`
- [ ] TURN relay works for IPv6 relay addresses
- [ ] No regression on existing NAT traversal tests
- [ ] `cargo test --workspace --lib` passes
- [ ] Meaning firewall CI gate passes (no kernel/app boundary violations)

---

## Files Requiring Changes

| File | Change | Phase |
|------|--------|-------|
| `icn-net/src/candidate.rs` | Add `EndpointCandidate`, `EndpointType`; refactor `ConnectionCandidate` | A |
| `icn-net/src/version.rs` | Add `ENDPOINT_SETS` capability flag | A |
| `icn-net/src/session.rs` | `connection_candidate()` builds endpoint vec | A |
| `icn-net/src/discovery.rs` | Collect all mDNS addresses | A |
| `icn-store/src/peer_cache.rs` | Add `endpoints` field to `CachedPeer` | A |
| `icn-core/src/supervisor/background_tasks.rs` | Build endpoint vec in candidate announcement | A |
| `icn-core/src/supervisor/nat_dial.rs` | Happy Eyeballs dialer | B |
| `icn-net/src/actor/mod.rs` | `NetworkMsg::Dial` takes endpoints | B |
| `icn-net/src/actor/messages.rs` | `handle_dial` iterates endpoints | B |
| `icn-net/src/protocol.rs` | `KnownPeer` optional `endpoints` field | B |
| `icn-net/src/handlers/peer_exchange.rs` | Populate multiple addresses | B |
| `icn-core/src/config/network.rs` | Per-type timeout config + happy_eyeballs_delay_ms | B |
| `icn-core/src/supervisor/bridge.rs` | Bootstrap peer multi-address parsing | B |
| `icn-net/src/relay_proxy.rs` | IPv6 XOR encode + remove bail-outs | C |
| `icn-core/src/config/mod.rs` | Default `listen_addr` → `[::]:7777` | 0 |
| `icn-core/src/config/gateway.rs` | Default `bind_addr` → `[::1]:8080` | 0 |
| `icn-core/src/supervisor/init_rpc.rs` | RPC bind → `[::1]` | 0 |
| `icn-obs/src/health.rs` | Health bind → `[::]` | 0 |
| `icn-obs/src/lib.rs` | Metrics bind → `[::]` | 0 |
| `icn-gateway/src/security.rs` | CORS `[::1]` variants | 0 |
| `deploy/config/icn.toml` | Config defaults | 0 |
| `deploy/k8s/configmap.yaml` | K8s config defaults | 0 |
