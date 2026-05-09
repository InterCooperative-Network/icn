---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-09
---

# Identity and Cryptography Map

> For current project truth, defer to [`docs/STATE.md`](../../STATE.md), [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md), [`docs/ARCHITECTURE.md`](../../ARCHITECTURE.md), and [`source-of-truth-map.md`](source-of-truth-map.md). This map is a routing layer, not a security audit.

## Purpose

Identity and cryptography are substrate concerns in ICN. They connect members, devices, nodes, services, institutions, actions, receipts, and recovery paths.

This map exists to prevent three mistakes:

1. treating identity as only a DID string;
2. turning source-level primitives into broad production claims;
3. collapsing person, device, node, service, and institution identity into one concept.

## Identity layers

| Layer | Meaning | Current posture |
|---|---|---|
| Person / member identity | A human participant or holder. | Real primitives exist; user-facing claims need precision. |
| Device identity | A device acting for a member or operator. | Source-level support exists; UX maturity needs verification. |
| Node identity | Infrastructure node identity. | Used by network/runtime paths; not the same as a person. |
| Service identity | A service actor under institutional control. | Mostly tied to service-hosting design direction. |
| Institution identity | Cooperative, community, federation, or other entity identity. | Represented through entity/governance/federation layers. |

## Source paths

| Area | Path |
|---|---|
| Identity, DID, keystore, recovery, revocation, membership | `icn/crates/icn-identity/` |
| General cryptography facade | `icn/crates/icn-crypto/` |
| Hybrid post-quantum primitives | `icn/crates/icn-crypto-pq/` |
| Zero-knowledge proof support | `icn/crates/icn-zkp/` |
| Steward / SDIS / VUI services | `icn/crates/icn-steward/` |
| Authorization and capabilities | `icn/crates/icn-authz/` |
| Network identity binding | `icn/crates/icn-net/` |
| Member-facing standing | `icn/apps/governance/`, `icn/crates/icn-gateway/` |
| Architecture docs | `docs/ARCHITECTURE.md`, `docs/architecture/IDENTITY_MEMBERSHIP_ARCHITECTURE.md` |

## Status bands

| Subsystem | Status | Public/demo visibility | Drift risk |
|---|---|---|---|
| DID identity and DID documents | implemented but partial | visible indirectly through member standing/session flows | medium |
| Keystore and backup paths | implemented but partial | developer/operator-facing | medium |
| Multi-device identity support | implemented but partial | not current demo center | high |
| Recovery and revocation | implemented but partial | not current demo center | high |
| Steward / SDIS / VUI machinery | implemented but partial | not current demo center | high |
| General signing/encryption primitives | implemented | mostly infrastructure | low-medium |
| Hybrid post-quantum primitives | feature-gated / partial | not current demo center | high |
| Zero-knowledge proof support | feature-gated / partial | not current demo center | high |
| Hardware-backed key paths | feature-gated / needs verification | not current demo center | high |
| Member standing read model | implemented | visible in pilot UI and demo fixtures | low-medium |

## Safe claims

Safe, current claims:

- ICN uses self-held identities and signed actions as substrate primitives.
- Member standing exists as a member-facing read model.
- Identity, roles, domains, and authority scopes are part of the current institutional runtime story.
- The repo includes dedicated crates for identity, cryptography, post-quantum primitives, zero-knowledge proof support, and steward-related machinery.
- Deeper autonomy or production-security claims require evidence for the specific runtime path being described.

## Claims to avoid

Avoid broad public claims that ICN already has:

- complete mobile identity UX;
- production steward-network deployment;
- fully deployed post-quantum runtime posture;
- production zero-knowledge credential deployment;
- fully audited cryptographic stack;
- default hardware-backed identity deployment;
- implemented service identity as part of live service hosting.

## Related maps

| Map | Relationship |
|---|---|
| `runtime-surface-map.md` | Shows where identity becomes member-facing standing and action cards. |
| `network-gossip-map.md` | Covers node identity, transport binding, signed envelopes, and network trust. |
| `service-hosting-map.md` | Should cover service identity when service hosting moves beyond design direction. |
| `tool-commons-map.md` | Should cover tool identity and capability grants. |
| `stale-and-archived-map.md` | Should catch old or overbroad security/autonomy claims. |

## Follow-ups

- Verify which identity, PQ, ZKP, and hardware-backed paths are enabled by default versus optional.
- Add a deeper security/autonomy map if cryptographic autonomy becomes a public narrative focus.
- Keep person, device, node, service, and institution identity separate in future docs.
