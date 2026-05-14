---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-09
---

# Network and Gossip Map

> For current project truth, defer to [`docs/STATE.md`](../../STATE.md), [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md), [`docs/ARCHITECTURE.md`](../../ARCHITECTURE.md), and [`source-of-truth-map.md`](source-of-truth-map.md). This map routes the distributed-systems substrate; it is not a production-readiness claim.

## Purpose

ICN's distributed substrate has several layers that are easy to collapse by accident. This map keeps them separate.

Core distinction:

```text
transport = connection
gossip = state propagation
federation = institutional relationship
```

A node connection is not a federation. A gossip topic is not a governance agreement. A federation is a governed institutional relationship that may use transport and gossip underneath it.

## Source paths

| Area | Path |
|---|---|
| Transport, peer sessions, discovery, topology, NAT/relay support | `icn/crates/icn-net/` |
| Topic gossip, anti-entropy, partition handling, storage messages | `icn/crates/icn-gossip/` |
| Time / clock / replay-adjacent support | `icn/crates/icn-time/` |
| Store, replicas, proof-of-storage related state | `icn/crates/icn-store/` |
| Snapshot and distributed snapshot support | `icn/crates/icn-snapshot/` |
| Observability | `icn/crates/icn-obs/`, `monitoring/` |
| Deployment and ports | `deploy/`, `docs/reference/project-index/ci-ops-deploy-map.md` |
| Federation layer | `icn/crates/icn-federation/`, governance/entity docs |

## Layer model

| Layer | Question answered | Status | Drift risk |
|---|---|---|---|
| Transport | Can this node connect to that peer? | implemented but partial | medium |
| Peer discovery | How are peers found? | implemented but partial | medium |
| Identity-bound transport | Is a peer connection bound to node identity? | implemented but needs precise claims | medium |
| Topology / fanout | Which peers should this node prioritize? | implemented but partial | medium |
| NAT / relay support | Can peers connect across difficult networks? | implemented but partial | high if described as production-reliable |
| Gossip topics | What state classes are exchanged? | implemented but partial | medium |
| Anti-entropy sync | How does state converge after missed messages? | implemented but partial | medium |
| Partition detection/healing | What happens after network splits? | implemented but partial | high |
| Replica/storage messages | How does storage durability propagate? | implemented but partial | medium-high |
| Observability | How do operators see network/gossip health? | implemented but partial | medium |
| Federation | What institutional relationships authorize exchange? | implemented but partial; not live production | high |

## Transport map

The transport layer belongs primarily to `icn-net`.

It includes or routes toward:

- peer sessions;
- discovery and bootstrap paths;
- identity-bound connection semantics;
- signed or authenticated message envelopes;
- replay protection and sequencing concerns;
- rate limiting;
- topology-aware neighbor selection;
- NAT traversal and relay-related support;
- version/capability negotiation;
- blob location support.

Public claim boundary:

- Safe: ICN has a real peer-networking substrate.
- Unsafe without more evidence: ICN is production-ready across arbitrary real-world networks.

## Gossip map

The gossip layer belongs primarily to `icn-gossip`.

It includes or routes toward:

- topic-based state propagation;
- topic access controls;
- causal/version tracking;
- Bloom-filter and digest-style sync;
- push/pull exchange;
- anti-entropy repair;
- partition detection and healing;
- storage and replica status messages;
- service-discovery and key-rotation related topics.

Public claim boundary:

- Safe: ICN has a real gossip/synchronization substrate.
- Unsafe without more evidence: all distributed conflict cases are solved automatically.

## Federation is higher than networking

Federation should not be described as the same thing as peer connectivity.

Correct model:

```text
secure transport
  -> gossip/sync
  -> institutional relationship
  -> governance/agreements/receipts
```

A live federation claim requires more than working network and gossip code. It requires a governed inter-institutional relationship and evidence that the relationship is active in the claimed context.

Current public boundary:

- Federation primitives exist.
- Live production federation should not be claimed.
- NYCN should not be described as a live federation member or formal pilot.

## Storage and durability connection

Network and gossip intersect with storage through:

- blob announcement and retrieval;
- replica status and requests;
- storage challenges and proofs;
- snapshot and recovery paths;
- maintenance and health checks.

These are substrate capabilities. They do not by themselves prove production durability for a public deployment.

## Observability connection

Operators need visibility into:

- peer count and session health;
- gossip throughput and lag;
- partition events;
- retry/backoff behavior;
- replica health;
- storage amplification;
- metrics and alerts.

The observability layer exists, but specific dashboard/alert coverage should be checked before making operational-readiness claims.

## Safe claims

Safe, current claims:

- ICN has a real peer-networking crate and a real gossip/synchronization crate.
- Transport, gossip, federation, and governance are separate layers.
- Network and gossip primitives are substantial but are not the current organizer/member demo proof.
- The current demo proves member/institutional proof-loop concepts, not production network readiness.

## Claims to avoid

Avoid claiming:

- production-ready networking across arbitrary environments;
- live federation deployment;
- NYCN as a live federation member;
- automatic resolution of all distributed conflicts;
- complete durability guarantees without storage/replica evidence;
- that peer connectivity alone equals institutional federation.

## Related maps

| Map | Relationship |
|---|---|
| `identity-crypto-map.md` | Node identity, signed messages, and network trust depend on identity/crypto. |
| `runtime-surface-map.md` | Runtime surfaces sit above networking/gossip. |
| `ci-ops-deploy-map.md` | Ports, deploy paths, K3s/devnet, and monitoring live there. |
| `show-readiness-map.md` | Defines what can and cannot be claimed externally. |

## Follow-ups

- Verify current production posture of NAT/relay paths before any public reliability claim.
- Review monitoring dashboards/alerts for network and gossip health.
- Add a storage/durability map if proof-of-storage and replica operations become public-facing.
- Keep federation described as an institutional relationship, not as network adjacency.
