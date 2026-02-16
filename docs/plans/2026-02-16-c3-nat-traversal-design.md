# C3 NAT Traversal Design

> Issue: [#1144](https://github.com/InterCooperative-Network/icn/issues/1144)
> Status: Approved
> Date: 2026-02-16

## Problem

Nodes behind different NATs cannot connect directly. The pilot requires
connectivity across locations (home networks, CGNAT, enterprise NAT).

## Acceptance Criteria (from #1144)

| Criterion | How satisfied |
|-----------|---------------|
| Nodes behind NAT can exchange gossip | TURN relay fallback when direct fails |
| Fallback to relay when hole-punch fails | Per-peer TurnRelayProxy with existing TurnClient |
| Clear error messages for connectivity issues | NatStatus with split direct/relay errors, icnctl output |

## Key Discovery

STUN client (`stun.rs`), TURN client (`turn.rs`), NAT config (`nat.rs`),
and connection candidate types (`candidate.rs`) **already exist** in `icn-net`.
What's missing is the wiring: dial fallback, TURN data-plane relay proxy,
and operator-facing status.

## Architecture: Per-Peer TURN UDP Proxy

Quinn expects raw UDP. TURN wraps/unwraps packets in SEND-INDICATION /
DATA-INDICATION framing. A local UDP proxy translates between the two.

```text
  Quinn endpoint (new, per-peer)
       | raw UDP
  [127.0.0.1:ephemeral] <--> TurnRelayProxy task
       | TURN-framed UDP
  [TURN server:3478]
       | relay
  [peer's TURN relay addr]
```

The `TurnRelayProxy` is a tokio-spawned task that:

1. Reads outbound UDP from local socket -> wraps via
   `TurnClient::send_indication(peer_relay_addr, payload)` -> sends to TURN server
2. Reads inbound from TURN server -> `TurnClient::parse_data_indication()` ->
   unwraps -> writes to local socket for Quinn to consume

Quinn connects through a **new Endpoint** bound to the proxy's local socket.
It is unaware of TURN. The proxy is the translation boundary.

### Constraint: "Relay" = TURN data-plane relay

"Relay fallback" uses the existing `TurnClient` as an actual data-plane relay.
It is NOT "dial a known public peer and call it a relay." The TURN server
relays packets between peers who cannot reach each other directly (symmetric NAT).

## Dial Fallback Flow

```text
NetworkMsg::Dial { addr, did, peer_relay_addr, response }

1. Try direct: session_manager.dial(addr, did)
   OK -> record traversal_mode=Direct, done.
   Err(direct_err) -> continue

2. Check relay viable:
   a) Our TURN allocation: session_manager.relay_addr()
   b) Peer relay candidate: peer_relay_addr (Option<SocketAddr>)
   Either None -> return Err(direct_err)
     + hint: "peer has no relay candidate; cannot TURN-relay"
   Both present -> continue

3. Create per-peer relay proxy:
   session_manager.create_relay_proxy(did, peer_relay_addr)
   - TURN create_permission(peer_relay_addr)
   - Bind local loopback UDP socket
   - Spawn relay task
   -> ProxyHandle { local_socket, shutdown_tx }

4. Create QUIC connection through proxy:
   New quinn::Endpoint bound to proxy.local_socket
   endpoint.connect(peer_relay_addr, "localhost")
   Quinn's outbound UDP -> proxy intercepts -> TURN send_indication
   TURN DATA-INDICATION -> proxy feeds back to Quinn as inbound UDP
   OK -> store connection, spawn handler, record traversal_mode=Relayed
   Err(relay_err) -> clean up proxy, return Err with both errors

5. Record outcomes:
   last_direct_error = Some(direct_err)
   last_relay_error = None (or Some if step 4 failed)
   last_traversal_mode = Relayed (or Direct if step 1 succeeded)
```

### Where does `peer_relay_addr` come from? (Without ICE)

`peer_relay_addr` is propagated via **out-of-band configuration** or
**bootstrap registry** for pilot nodes (devnet config, known peers list,
CLI flags). Each node publishes its TURN-allocated relay address in node
configuration. Later this can graduate to candidate gossip or ICE, but
this PR does not implement candidate exchange.

## Proxy Lifecycle

- **Create**: on first relay-needed dial for `(did, peer_relay_addr)`
- **Reuse**: if proxy for that `(did, peer_relay_addr)` pair is still healthy
- **Drop on**:
  - QUIC connection close / disconnect
  - Repeated relay errors (3 consecutive failures)
  - Explicit shutdown (node stopping)
  - TTL expiry (default: match TURN allocation lifetime, ~10 min)
- **Cleanup**: `ProxyHandle.shutdown_tx` signals the relay task to exit.
  Task drops the local UDP socket on exit. SessionManager removes the
  ProxyHandle from its map.

## NatStatus Report

```rust
pub struct NatStatus {
    /// STUN-discovered public endpoint (None if STUN failed/disabled)
    pub public_endpoint: Option<SocketAddr>,
    /// TURN relay address (None if not allocated)
    pub relay_addr: Option<SocketAddr>,
    /// Number of active relay proxies (per-peer)
    pub active_relay_count: usize,
    /// Last traversal mode used for any dial
    pub last_traversal_mode: TraversalMode,
    /// Last direct connection error (if any)
    pub last_direct_error: Option<String>,
    /// Last relay connection error (if any)
    pub last_relay_error: Option<String>,
}

pub enum TraversalMode {
    Direct,
    Relayed,
    Unknown, // no dial attempted yet
}
```

## icnctl Output

`icnctl net status` prints a NAT section:

```
Network Status:
  Peers discovered: 3
  Connections active: 2

  NAT Traversal:
    Public endpoint:  203.0.113.5:4433
    TURN relay:       198.51.100.1:3478 (allocated)
    Active relays:    1
    Last traversal:   Relayed
    Last direct err:  Timeout dialing peer
    Last relay err:   none
```

## Files Changed

| File | Change | LOC |
|------|--------|-----|
| `icn-net/src/relay_proxy.rs` | NEW: TurnRelayProxy + ProxyHandle | ~120 |
| `icn-net/src/session.rs` | create_relay_proxy(), NatStatus query | ~60 |
| `icn-net/src/actor/messages.rs` | Dial fallback: direct -> relay proxy | ~50 |
| `icn-net/src/actor/mod.rs` | NetworkMsg::Dial gets peer_relay_addr, GetNatStatus msg, NatStatus fields | ~40 |
| `icn-net/src/lib.rs` | Export relay_proxy, NatStatus, TraversalMode | ~5 |
| `icnctl/src/main.rs` | net status prints NAT section | ~30 |
| Integration test | Force-fail direct, assert relay path | ~80 |
| `docs/guides/operations/nat-traversal.md` | Ops doc | ~50 |
| **Total** | | **~435** |

## Testing Strategy

### What is proven

1. **Proxy framing unit test**: Feed mock TURN DATA-INDICATION bytes into
   proxy, assert raw payload appears on local socket. Feed raw UDP outbound,
   assert SEND-INDICATION with correct `peer_relay_addr` emitted.
   *Claims*: proxy wraps/unwraps correctly.

2. **Dial fallback integration test**: Force direct dial to fail (unreachable
   TEST-NET addr `192.0.2.1:1`). Stand up in-process TURN echo stub that
   reflects indications. Assert: proxy starts, Quinn connects through it,
   at least one NetworkMessage round-trips via the proxy.
   *Claims*: fallback path exercised, Quinn traffic traverses proxy boundary.

3. **NatStatus test**: After relay fallback, verify
   `last_traversal_mode == Relayed`, `last_direct_error == Some(...)`,
   `last_relay_error == None`.

### What is NOT proven

- RFC TURN compliance (stub is "TURN-ish", not a real TURN server)
- End-to-end symmetric NAT traversal (requires real NAT topology)

For pilot validation: test against real coturn in two NATed networks.

## Operational Documentation

`docs/guides/operations/nat-traversal.md` covers:

- How to configure TURN server (`NatConfig` in node config)
- What `icnctl net status` shows and what each field means
- VPN fallback: if TURN is unavailable, use WireGuard/Tailscale as flat network
- Pilot validation procedure: coturn + two NATed hosts

## Out of Scope (explicitly deferred)

- ICE candidate negotiation
- Candidate gossip exchange (`network:candidates` topic)
- NAT type classification (RFC 3489 taxonomy)
- Hole-punching logic
- Multi-tenant proxy (shared proxy for N peers)
