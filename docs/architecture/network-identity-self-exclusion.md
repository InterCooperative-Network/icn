# Network identity: the local node is never a remote peer

**Status**: active rule
**Established**: 2026-08-02 (#2506)
**Applies to**: `icn-net`, `icn-gossip`, and the supervisor wiring in `icn-core` that feeds them

## The rule

A node's local identity may legitimately appear in **local** state. It must never be admitted into
state whose semantics are *"a remote network peer"*.

| The local node MAY hold | The local node MUST NOT be |
|---|---|
| its own DID and keypair | a remote connection-map entry |
| its own advertised addresses | a discovered / dialable peer |
| its own connection candidate | a gossip peer or network-size contributor |
| its own signing sequence | a replay-guard window |
| its own outbound gossip nonce | a misbehaviour, quarantine, or ban subject |

The distinguishing test is **identity, never address**. Two nodes legitimately share a source
address behind NAT, a proxy, a relay, or a shared pod IP; rejecting a peer because its address
resembles ours would partition exactly those deployments. A remote peer is one whose
**authenticated DID differs from the local DID**. Discovery metadata alone — an address, an mDNS
record, a gossiped candidate — is never sufficient to establish that distinction, because all of it
is either self-authored or attacker-influenced.

## Why it needs stating

Every discovery source in ICN echoes the local node's own advertisement back to it:

- `network:candidates` gossip delivers our own `ConnectionCandidate` to us;
- mDNS browse resolves the service we ourselves registered on the same multicast group;
- peer exchange can hand our own entry back in a `Response` or `Announce`.

So "the local identity arrives looking like a peer" is the **normal case**, not an edge case. Absent
an explicit rule, every layer downstream treats it as remote — and each one is individually
reasonable in doing so, because none of them knows who "we" are.

## Ownership

**Primary owner — the connection layer** (`icn-net::session`). `SessionManager` holds `local_did`
and refuses it in both insertion paths (`dial`, `install_incoming_connection`). This is the
canonical guard because it is the only point where the remote DID is *authenticated* rather than
claimed: on the Hello path the DID-TLS binding is verified before installation. Refusing here closes
every downstream path by construction — no connection means no gossip peer, no replay window, no
misbehaviour subject.

`install_incoming_connection` takes the local DID as a **required parameter** rather than reading it
from ambient state, so a future caller cannot install a connection without declaring what "self" is.

**Semantic integrity — the receive path** (`icn-net::handlers::signed`). `ReplayGuard` means
"per-remote-sender sequence high-water" and `MisbehaviorDetector` means "remote peer conduct".
A self-sourced envelope is dropped before either is touched. This is *not* a blanket "skip security
checks for our own DID": the message is discarded rather than trusted, and the scope is the network
receive path, where a self-sourced envelope can only be a self-connection.

**Early exit — discovery sources** (`CandidateCache`, `Discovery`, peer exchange). These prevent
pointless work rather than providing the guarantee; live, they were ~90% of one node's connection
handling. `CandidateCache` requires its owning DID at construction and has no `Default`, so a cache
that cannot recognise its own node is not representable.

## Deliberately not guarded

`MisbehaviorDetector` was **not** given a local-DID bypass. A blanket `if did == self { ignore }`
there would suppress genuine local faults — a corrupted keystore, a cloned state directory, a
compromised component signing bad traffic. With the connection guard in place the detector cannot
see the local DID through the network loop at all; if it ever does, that is a real invariant
violation and should stay visible. The receive-path guard logs at `WARN` for the same reason.

`AntiEntropy` legitimately keeps a `PeerSyncManager` entry keyed by the local DID to hold its own
outbound digest nonce. That entry stays; what changed is that `remote_peer_count` excludes it, so
network size means "other nodes" rather than "map entries".

## When adding networking code

Ask: *does this structure mean "remote peer"?* If yes, it needs the local DID and must refuse it —
and prefer requiring the identity at construction or in the signature over looking it up, so the
check cannot be forgotten.

Related: #2504 (restart/rejoin), #2505 (stale connection replacement), #2510 (durable signing
sequence), #2509 (candidate re-announcement), #2512 (banned-peers metric).
