# ADR-0009: Happy Eyeballs — IPv4/IPv6 Connection Racing Within Endpoint Categories

**Date**: 2026-03-21
**Status**: accepted
**Tags**: networking, ipv6, dual-stack, connectivity
**Supersedes**: (none)

## Context

Sprint 18 completes the IPv6 connectivity track (epic #1295, phases 21-22). With
`EndpointCandidate` / `EndpointKind` now available (PR #1358, ADR-0003), nodes can
store and exchange multiple addresses per kind (local, public, relay).

In dual-stack environments, a peer may have both an IPv4 and an IPv6 local address.
Naively preferring IPv6 breaks nodes where the IPv6 path is degraded (e.g. NAT64
tunnels, misconfigured 6to4, IPv6-only interfaces with no default route). Naively
preferring IPv4 leaves IPv6-native networks underserved and slows connection setup
for nodes where IPv6 is faster.

The existing `dial_parallel()` in `nat_dial.rs` already races *endpoint categories*
(local vs public). This ADR extends that to racing *IP versions within a category*
following RFC 8305.

## Decision

Add `dial_happy_eyeballs(addrs: Vec<SocketAddr>, ...)` to `nat_dial.rs`:

1. **Sort order**: IPv6 addresses first, IPv4 second (future-proof for IPv6-only networks).
2. **Stagger**: Spawn the IPv6 dial immediately. If no success within `happy_eyeballs_delay_ms`
   (default 250ms), spawn the IPv4 dial. This matches the delay specified in RFC 8305 §5
   and used by all major browsers.
3. **Winner takes all**: Return the first successful connection, cancel the other task.
4. **Scope**: Races within a single `EndpointKind` (Local, Public, or Relay). The existing
   `dial_parallel()` continues to race *across* kinds (local vs public). Happy Eyeballs
   adds a second level of racing *within* a kind.
5. **Config**: Add `happy_eyeballs_delay_ms: u64` to `NatDialConfig` with
   `#[serde(default)]` so existing config files remain valid.

## Consequences

**Easier**:
- Dual-stack nodes connect via whichever path (IPv4 or IPv6) responds first.
- IPv6-native nodes get preference without harming dual-stack nodes with slow IPv6.
- The 250ms stagger limits overhead: worst case is one extra dial attempt per connection.

**Harder or riskier**:
- Slightly more concurrent dials during connection setup (bounded to 2 per category).
- `dial_parallel()` already handles the winner-takes-all pattern; extending it adds
  complexity that must be tested carefully.

**Out of scope (deferred)**:
- Cross-category Happy Eyeballs (local-IPv6 vs public-IPv4) — not needed now.
- Adaptive delay based on observed RTT — overkill for Sprint 18.

## Alternatives Considered

| Alternative | Why rejected |
|-------------|-------------|
| Static IPv6 preference | Breaks dual-stack nodes with degraded IPv6 paths; no recovery mechanism |
| Static IPv4 preference | Leaves IPv6-native networks underserved; wrong direction for the cooperative internet's long-term trajectory |
| No racing, try IPv6 then IPv4 sequentially | Adds full `public_dial_timeout_ms` (10s) latency when IPv6 fails; unacceptable UX |
| Race all addresses simultaneously with no stagger | More concurrent connections, wastes bandwidth, and doesn't give IPv6 preferential treatment as required by RFC 8305 |
| Per-peer IP version learning / memory | Correct long-term, but premature for Sprint 18 scope; deferred to a future ADR |
