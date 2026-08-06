# Network Session Identity Binding

**Status**: Active invariant
**Scope**: `icn-net` Hello path, `icn-identity` binding helpers
**Related**: [network-identity-self-exclusion.md](network-identity-self-exclusion.md),
[replay-state-restart-invariants.md](replay-state-restart-invariants.md)

## The invariant

> A connection may be attributed to remote DID **B** only if B's DID key authenticated
> the certificate presented by **that exact connection**.

A `BindingInfo` proves "DID B signed the hash of *some* certificate". That is a claim
about the past. Session attribution needs a claim about the present: that the party on
the other end of *this* TLS session holds the key for the certificate B authenticated.

Three facts are jointly required, and none of them is sufficient alone:

1. `binding_info.did == from` — the binding belongs to the DID sending the Hello.
2. B's Ed25519 key signed `binding_info.tls_cert_hash`.
3. `sha256(current peer certificate) == binding_info.tls_cert_hash`.

Facts (1)+(2) alone are replayable. Every node publishes its own `BindingInfo` in every
Hello it sends, so any party that has ever exchanged a Hello with B holds material that
satisfies them. Fact (3) is what ties the claim to the live session: presenting B's
certificate requires B's TLS private key, and the TLS 1.3 handshake independently proves
possession of it (`TofuCertificateVerifier::verify_tls13_signature`).

Fact (3) alone is also insufficient — it proves a certificate matches a binding, but not
*whose* binding it is. A peer could present its own valid binding while claiming another
DID in `from`. That is why `NetworkMessage::verify_hello` checks (1) as well as (3).

## Why this is a composition property, not a helper property

`icn-identity` has exposed `verify_binding_info(binding, peer_cert)` — which enforces
(2)+(3) — since 2025-11-13. The Hello path did not call it between 2025-12-18 and
2026-08-03; it called `verify_did_matches_binding`, which enforces (1)+(2) only. The
strict helper existed, was correct, was tested, and was not wired in.

The regression that removed it was not careless. Mandatory client-certificate
verification at the TLS layer was breaking connections, so client auth was made optional
(`client_auth_mandatory() == false`). The strict check had been written as
`if let Some(peer_cert) = connection.peer_identity()` — conditional on a certificate
being present. Once certificates became optional that condition was sometimes false, and
the check was dropped rather than made fail-closed.

**Breadcrumb**: the existence of a correct verifier is not enforcement. When auditing an
authentication property, audit the composition root — the call site that runs in
production — not the helper's test coverage.

## Certificate availability

Measured against the production TLS configuration (`create_server_config` /
`create_tofu_client_config`):

| Direction | Local role | Peer certificate | `peer_identity()` |
|---|---|---|---|
| outbound dial | QUIC client | always (server cert is mandatory) | `Some` |
| inbound accept, ICN peer | QUIC server | yes — ICN clients present one | `Some` |
| inbound accept, anonymous | QUIC server | **no** | **`None`** — handshake still succeeds |

The third row is why the absent-certificate case must fail closed. Client auth is
*offered* but not *mandatory*, so a peer that simply omits its certificate reaches the
Hello path with no certificate material. If absence were treated as "cannot verify,
therefore permit", the binding check would be bypassable by omission.

Legitimate ICN nodes are unaffected: `SessionManager::start` builds both the server and
client TLS configs from `identity_bundle.tls_cert()`, so the certificate presented in
either direction is exactly the one hashed into `BindingInfo`.

## Ordering

The binding check runs before any state keyed on the claimed DID is written. Everything
downstream inherits its guarantee:

- `peer_connections[from]` — negotiated version, capabilities, X25519 key, PQ keys
- `session connections[from]` — the DID→connection mapping used to route outbound traffic
- neighbor sets / topology membership

The X25519 key is the sharpest case. It is carried in the Hello **unsigned**, and
`try_encrypt_envelope` reads it back via `get_peer_x25519_key(to_did)` when encrypting to
that peer. Storing it under an unauthenticated DID claim means messages addressed to B
get encrypted to a key the claimant chose.

## Failed claims must not penalise the claimed DID

A Hello that fails binding verification is refused, and **nothing is recorded against the
claimed DID**. At that point the peer is unauthenticated, so `from` is a name it selected;
scoring it would let any party degrade the reputation of a peer it does not control.
Telemetry (`icn_network_hello_binding_rejected_total`) is labelled by failure class only,
never by DID, for the same reason.

## Certificate rotation

`IdentityBundle` derives `BindingInfo` from the certificate it holds
(`hash_certificate(&self.tls_cert)`) and the signature generated alongside it, so a
rotated certificate always ships with a matching binding — they cannot drift apart. A
captured old binding stops being usable once the old certificate is out of service,
because using it requires presenting the old certificate, which requires its private key.

`BindingInfo.created_at` is **not** covered by the signature and is not validated. Under
this invariant that is not load-bearing: replay is defeated by current-certificate
equality, not by freshness. Signing or validating `created_at` would only be needed to
support binding *expiry* independent of certificate lifetime, which is a separate design
question.

## What the authenticated identity is checked against

The invariant above says who a connection may be attributed to. It does not say whether that
party is who the operator *meant* to reach. Those are separate questions, and a bootstrap
entry of the form `icn://did:icn:X@HOST:PORT` raises the second one.

**A configured DID is an operator expectation, not a cryptographic pin.** When X is
configured and B authenticates, B is the peer — for the session map, for peer exchange, for
trust, for addressing. The connection is kept. What changed in #2533 is only that the
divergence is no longer silent: it is logged once per connection at `warn!`, with both DIDs
and the remote address, and counted by `icn_network_bootstrap_did_expectation_mismatch_total`.

Three properties make that comparison safe, and each is load-bearing:

| Property | Why |
|---|---|
| Compared only *after* the three checks above | Before them `from` is a name the sender picked. Comparing against a claim would let whoever answers the address decide what this node reports about its own configuration — and would report a divergence at the one moment the node cannot say who it is talking to. A connection that never authenticates produces no observation. |
| Held per physical connection, keyed by nothing | The expectation belongs to one dial. Concurrent dials to one address can carry different expectations, an address can be reused by a different node, and the expected DID is precisely the value that is wrong in the case being detected — so neither address nor DID is a sound key. It lives in `ConnectionContext` and dies with the connection. |
| Stated by the caller, never inferred from the dial argument | `dial_addr` synthesises a placeholder DID from the socket address and dials with it, so an address-only bootstrap is type-identical to a `KnownDid` one by the time the actor sees it. Inferring an expectation from "the DID we dialled with" would report every `icn://HOST:PORT` entry as misconfigured. `NetworkHandle::dial_expecting` is the only entry point that states one, and `dial_bootstrap_peers`' `KnownDid` arm is its only production caller. |

**A divergence is not misbehaviour.** Nothing is scored, nobody is banned, and neither DID is
penalised — for the same reason a failed binding does not penalise the claimed DID, and one
more besides: the peer did nothing wrong by being itself. A mismatch means configuration and
reality disagree, which is the operator's to resolve.

**Why not enforce the pin.** Refusing the connection would turn a bootstrap entry left stale
by a key rotation, a redeploy, or a copy-paste into a hard connectivity failure with no
in-band way to recover. It would also buy less than it appears to: under the invariant above,
an authenticated B *cannot* be impersonating X, so a mismatch is never impersonation — it is
"you reached a different real node than you named". Refusing is a federation-membership
policy, not an authentication fix, and no in-repo document has ever promised it. #2533 records
the full option analysis; the metric exists so that a future decision to enforce can be made
against evidence rather than intuition.

Regression coverage: `crates/icn-net/tests/bootstrap_did_expectation.rs` (the policy itself,
including a test that a strict-pin implementation would fail) and
`crates/icn-core/tests/bootstrap_did_expectation_wiring.rs` (that the bootstrap path actually
states the expectation).

## Enforcement point

One check, in `ConnectionContext::handle_hello`. Both connection directions —
inbound accept (`actor/connection.rs`) and outbound dial (`actor/messages.rs`) — funnel
into the same `handle_connection` dispatch loop and therefore the same handler, so a
single call site covers both. `handle_hello` already receives the live
`quinn::Connection`; no plumbing is required to reach the certificate.

Regression coverage: `crates/icn-net/tests/hello_current_cert_binding.rs`.
