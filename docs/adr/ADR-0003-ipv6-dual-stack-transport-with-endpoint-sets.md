# ADR-0003: IPv6 Dual-Stack Transport with Endpoint Sets

**Date**: 2026-03-01
**Status**: accepted
**Tags**: networking, transport, ipv6, kernel

## Context

ICN models each peer as having exactly one SocketAddr. This is too fragile: the single address is a guess, there is no middle ground between direct and relay, and all bind defaults use 0.0.0.0 which prevents IPv6-native deployments. Cooperatives and federations operating on private mesh networks (WireGuard, site VPNs) have no way to signal their preferred internal routing path. The existing QUIC transport (quinn 0.11) is natively dual-stack, Rust's SocketAddr already handles both v4 and v6 symmetrically, and DIDs are fully address-independent — the architecture is already correctly separated. The only hard blocker is relay_proxy.rs (IPv4-only, ~80 lines of work) and the single-address data model throughout icn-net and icn-store.

## Decision

Adopt a three-phase IPv6 integration: (0) change bind defaults from 0.0.0.0/127.0.0.1 to [::]/ [::1]; (A) introduce EndpointCandidate type and refactor ConnectionCandidate to carry Vec<EndpointCandidate> with typed variants (UlaV6, GlobalV6, PublicV4, PrivateV4, Relay); (B) replace dial_with_fallback() with a Happy Eyeballs dialer that races typed candidates in priority order; (C) remove IPv4-only restrictions from relay_proxy.rs. Wire compatibility is handled via the existing CapabilityFlags system (new ENDPOINT_SETS flag) plus a MAX_SUPPORTED_VERSION bump to 2. All IPv4 behavior is preserved; IPv6 is preferred when available but never required. ULA addresses (fd::/8) serve as a routing convenience fabric for federation overlays but are never treated as identity.

## Consequences

Becomes easier: nodes on dual-stack networks prefer IPv6 automatically; federation mesh deployments use ULA for internal routing; NAT traversal becomes less necessary as native IPv6 reachability eliminates most NAT; connection reliability improves as Happy Eyeballs races multiple paths instead of sequential fallback. Becomes harder: protocol version negotiation must be tested carefully (postcard positional encoding hazard in KnownPeer); dual-stack socket behavior varies by OS (Linux IPV6_V6ONLY default differs from Windows/BSD); link-local addresses (fe80::) require scope IDs which may be lost in SocketAddr serialization round-trips.

## Alternatives Considered

Alternative 1: libp2p Multiaddr — would provide a richer addressing abstraction with protocol stacking (/ip6/.../udp/.../quic-v1). Rejected because it would require replacing the entire icn-net transport layer and introduces a large dependency. The current SocketAddr + EndpointType approach achieves the same routing goals with far less code change. Alternative 2: WireGuard overlay only — run all ICN traffic over a WireGuard mesh and keep transport as IPv4. Rejected because it requires all nodes to be on the WireGuard mesh, which blocks public-internet adoption. Alternative 3: Minimal dual-stack only (no endpoint sets) — just change config defaults and fix relay_proxy.rs. Rejected because single-address-per-peer leaves connection reliability worse than the current state; without typed endpoint candidates, there is no way to prefer ULA over public addresses or implement proper Happy Eyeballs fallback.
