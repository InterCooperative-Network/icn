---
Status: descriptive
Canonical: no
Last Reviewed: 2026-05-09
---

# Project Coverage Matrix

> For current project truth, defer to [`docs/STATE.md`](../../STATE.md), [`docs/PHASE_PROGRESS.md`](../../PHASE_PROGRESS.md), and [`source-of-truth-map.md`](source-of-truth-map.md). This matrix is a coverage proof: it tracks whether the major ICN project areas have an explicit map, truth anchor, status band, and next action.

## Purpose

This matrix exists so future project reviews do not depend on ad hoc subsystem discovery.

A project overview is not complete until it accounts for:

- every top-level source family in [`source-tree-map.md`](source-tree-map.md);
- every Rust workspace group in [`rust-workspace-map.md`](rust-workspace-map.md);
- every current project-index map;
- every public/member/operator surface;
- the in-repo NYCN package and the external NYCN operator repo;
- ops, MCP, skills, drift checks, CI, and deploy surfaces;
- stale/archive documents and unsafe public-surface vocabulary seams.

## Status vocabulary

Use these status bands consistently:

| Status | Meaning |
|---|---|
| `implemented` | Runtime/source evidence exists and the surface is active. |
| `implemented but partial` | Real implementation exists, but coverage/integration/maturity is incomplete. |
| `feature-gated` | Code exists behind a feature flag or optional build path. |
| `fixture-backed` | Demonstrable through committed fixtures, not live runtime state. |
| `package-local` | True inside an institution package such as NYCN, not generic ICN substrate truth. |
| `design-direction` | Architecture/RFC/docs direction exists; do not claim runtime implementation. |
| `historical` | Useful record, not current truth. |
| `unknown / needs local verification` | Source exists or has been referenced, but current integration/status has not been verified. |

## Coverage matrix

| Area | Source-of-truth anchor | Source paths | Status | Public/demo visibility | Package/privacy boundary | Drift risk | Mapped in | Next action |
|---|---|---|---|---|---|---|---|---|
| Source-of-truth hierarchy | `source-of-truth-map.md`, `docs/STATE.md`, `docs/PHASE_PROGRESS.md` | `docs/reference/project-index/`, `docs/` | implemented | indirect; affects all public claims | repo-wide | high if ignored | `source-of-truth-map.md` | Keep maps subordinate to state docs and code. |
| Phase model | `docs/PHASE_PROGRESS.md` | `docs/PHASE_PROGRESS.md` | implemented | public via website and show-readiness docs | no private data | high: Phase 2 overclaim | `current-truth-map.md`, `show-readiness-map.md` | Keep website and demo docs synced with Phase 2 status. |
| Constitutional invariants / meaning firewall | `docs/ARCHITECTURE.md`, `docs/architecture/KERNEL_APP_SEPARATION.md`, `docs/genesis.md` | `docs/architecture/`, `icn/crates/icn-kernel-api`, app PolicyOracle paths | implemented but partial | public narrative uses simplified form | generic ICN substrate | medium: old docs can blur kernel/app line | architecture docs | Consider a compact invariant map if drift recurs. |
| Safe vocabulary / regulatory framing | `show-readiness-map.md`, ADR-0032, compliance linter | website, docs, pilot-ui, CI | implemented but partial | high | public ICN surfaces must avoid unsafe framing | high: stale timebank/economic language | `show-readiness-map.md`, #1779 | Complete #1779; expand public-surface drift checks. |
| Rust workspace inventory | `rust-workspace-map.md`, `source-tree-map.md` | `icn/crates/`, `icn/apps/`, `icn/bins/` | implemented | not public-facing directly | generic ICN substrate | medium: crate count/path drift | `rust-workspace-map.md` | Keep matrix rows reconciled with workspace map. |
| Identity / DID / membership | `docs/architecture/IDENTITY_MEMBERSHIP_ARCHITECTURE.md`, `runtime-surface-map.md` | `icn/crates/icn-identity`, governance standing paths | implemented but partial | visible through standing; deeper identity mostly not public | personal/device/node/service identities must stay distinct | high: identity types can be collapsed in prose | proposed `identity-crypto-map.md` | Add identity/crypto map. |
| Cryptography / PQ / ZKP | `docs/ARCHITECTURE.md`, source exports | `icn/crates/icn-crypto`, `icn-crypto-pq`, `icn-zkp` | feature-gated / implemented but partial | not show-central | security/autonomy claims need evidence | high: easy to overclaim PQ/ZKP | proposed `identity-crypto-map.md` | Separate implemented crypto from autonomy guarantees. |
| Steward / SDIS / VUI | source exports and architecture docs | `icn/crates/icn-steward`, `icn-identity`, `icn-zkp` | implemented but partial | not public-demo central | personhood/privacy-sensitive | high: should not be broad public claim yet | proposed `identity-crypto-map.md` | Verify current runtime integration before public claims. |
| Authorization / capabilities / constraints | architecture docs, runtime surface docs | `icn-authz`, `icn-kernel-api`, apps/PolicyOracle paths | implemented | visible through standing/action scopes | generic substrate; institution meaning stays in apps | medium | architecture docs, runtime map | Add capability row details in future atlas. |
| Networking / transport | source exports, onboarding refs | `icn/crates/icn-net` | implemented but partial | not current demo proof | node/network metadata concerns | medium-high: peer connectivity can be mistaken for federation | proposed `network-gossip-map.md` | Add network/gossip map. |
| Gossip / anti-entropy / partition healing | source exports, onboarding refs | `icn/crates/icn-gossip` | implemented but partial | not current demo proof | topic/ACL/privacy-sensitive | medium-high: production-readiness overclaim | proposed `network-gossip-map.md` | Add status-banded gossip map. |
| Time / ordering / replay protection | source exports | `icn/crates/icn-time`, network/gossip paths | implemented but partial | not public central | generic substrate | low-medium | proposed `network-gossip-map.md` or future time map | Include in network/substrate map. |
| Storage / snapshots / replicas | source exports, ops runbooks | `icn-store`, `icn-snapshot`, docs/guides/operations | implemented but partial | not public central | may include sensitive persisted state | medium: durability claims need precise status | future storage/durability map | Add to coverage follow-up after network map. |
| Security / misbehavior | source exports | `icn-security`, security docs | implemented but partial | not public central | trust/reputation-sensitive | medium-high: security claims need verification | future security map | Do deeper pass before public claims. |
| Observability / monitoring | `ci-ops-deploy-map.md`, onboarding refs | `icn-obs`, `monitoring/`, deploy docs | implemented but partial | operator-facing | ops/deployment-specific | medium | `ci-ops-deploy-map.md` | Add dashboard/alert review if needed. |
| Governance runtime | `runtime-surface-map.md`, `PHASE_PROGRESS.md` | `icn-governance`, `icn/apps/governance`, gateway routes | implemented | strong; demo and public narrative | generic substrate; institution-specific meaning in packages | low-medium | `runtime-surface-map.md` | Keep show-readiness current. |
| Charters / CCL bridge | `PHASE_PROGRESS.md`, ADR-0021/0022 | `icn-ccl`, `icn/apps/charter`, `contracts/templates/` | implemented | public architecture narrative | generic templates; local meaning in packages | medium | phase docs; proposed `ccl-map.md` | Add CCL map distinguishing current vs future roles. |
| Member standing | `runtime-surface-map.md`, `PHASE_PROGRESS.md` | `/v1/gov/me/standing`, pilot-ui | implemented | visible in pilot-ui | member identity/standing data must be protected | low-medium | `runtime-surface-map.md`, #1779 | Keep pilot-ui docs synced. |
| Action cards | `runtime-surface-map.md`, ActionCard contract docs | `/v1/gov/me/action-cards`, `docs/contracts/institution-package/` | implemented but partial | visible in pilot-ui and fixtures | package semantics belong outside core | high: old website said not implemented | `runtime-surface-map.md`, #1779 | Complete #1779; extend demo via #1777. |
| Receipts / provenance | `runtime-surface-map.md`, `STATE.md` | receipt stores, governance receipts, pilot-ui receipts | implemented but partial | visible conceptually; fixture coverage incomplete | receipt payloads may encode institutional meaning | medium | runtime map, demo readiness map | Add receipt fixture after governance fixture. |
| Ledger / settlement / obligations / allocations | economics docs, runtime map | `icn-ledger`, `icn/apps/ledger`, economics docs | implemented but partial | not current demo center | regulatory-safe vocabulary required | high: old public docs use unsafe terms | future economics/current-status map | Avoid old terminology; verify current paths before claims. |
| Compute / model workloads | compute design docs, ADRs | `icn-compute`, compute docs, model workload docs | implemented but partial / design-direction | not current demo center | compute cannot decide governance | medium-high: AI/governance overclaim | future compute map | Distinguish advisory compute from governance authority. |
| Entity / federation / trust | runtime map, current truth map | `icn-entity`, `icn-federation`, `icn-trust`, governance paths | implemented but partial | public mentions must be cautious | cross-institution claims require evidence | high: live federation overclaim | current truth/show-readiness maps | Add federation distinction to network map. |
| Domain infrastructure | architecture docs | `docs/architecture/COOPERATIVE_DOMAIN_INFRASTRUCTURE.md` | design-direction | not show-central | domain-local by default; share by agreement | medium-high: can sound implemented | proposed domain/service maps | Add domain-infrastructure map. |
| Service hosting | `SERVICE_HOSTING_MODEL.md` | architecture docs, future ops/service paths | design-direction | not public capability claim | service identity/custody/export boundaries | high: implemented-service overclaim | proposed `service-hosting-map.md` | Add service hosting map. |
| Tool Commons | `COOPERATIVE_TOOL_COMMONS.md`, RFC-0017 | architecture docs, RFC docs | design-direction | not public capability claim | tools must not own institutional truth | high: app-store/SaaS drift | proposed `tool-commons-map.md` | Add Tool Commons map. |
| Gateway / OpenAPI / SDK | `runtime-surface-map.md`, `ci-ops-deploy-map.md`, AGENTS | `icn-gateway`, `docs/api`, `sdk/typescript` | implemented | developer-facing | API drift affects clients | medium | runtime and CI maps | Keep API type drift checks green. |
| React Native / mobile member shell | mobile spec, source-tree map | `sdk/react-native`, `docs/mobile/` | implemented but partial / design-direction | not complete product | accessibility/device-reach requirement | medium-high: complete-mobile overclaim | source-tree/show-readiness maps | Do dedicated mobile pass before claims. |
| Pilot UI | runtime map, demo readiness map, #1779 | `web/pilot-ui` | implemented but partial / fixture-backed | high | fixtures must be fictional; no private data | high due stale README/metadata | #1779, proposed pilot-ui map | Complete public-surface truth sync. |
| Dashboard | source-tree map | `web/dashboard` | implemented but partial | operator/admin surface | operator data boundaries | medium | source-tree map | Review before public/operator claims. |
| Website | ADR-0032, show-readiness map | `website/` | implemented but partial | high | public truth boundary | high | #1779, proposed website-truth map | Keep public pages synced to state docs. |
| ICN Learn | repo README | `InterCooperative-Network/icn-learn` | scaffold / learning surface | education/onboarding | teaches; does not define ICN | medium if treated as canonical | project family map | Keep canonical links back to ICN repo. |
| In-repo institution package | source-tree map, package boundary docs | `institutions/`, `institutions/nycn/` | package-local | not public ICN core | package semantics do not belong in kernel/core | medium | `pilot-and-nycn-map.md` | Map against external NYCN repo. |
| External NYCN operator repo | NYCN README, validation scripts, pilot map | `InterCooperative-Network/nycn` | package-local / rehearsal | organizer/operator-facing | private overlay and fixture-only boundaries | high: formal-pilot/private-data overclaim | `pilot-and-nycn-map.md` | Keep NYCN active-track language precise. |
| Ops / MCP / skills | `ci-ops-deploy-map.md`, `ops/CLAUDE.md`, skills registry | `ops/`, `.claude/skills`, scripts | implemented | developer/agent-facing | no private data in prompts/config | medium | `ci-ops-deploy-map.md`, proposed dev tooling map | Add development-tooling map. |
| CI / deploy / K3s | `ci-ops-deploy-map.md`, deploy docs | `.github/workflows`, `deploy/`, `monitoring/` | implemented but partial | operator-facing | homelab/devnet vs production boundary | high: production overclaim | `ci-ops-deploy-map.md` | Keep K3s framed as homelab/devnet proof path. |
| Docs control plane | `docs/INDEX.md`, `docs/registry.toml`, doc checker | `docs/`, `docs/scripts/` | implemented | contributor-facing | repo-wide | medium | docs-control map, source truth map | Connect website/pilot docs more strongly. |
| Stale/archive material | source-of-truth map, docs registry | `docs/archive/`, older docs, website mirror, old pilot docs | historical / mixed | dangerous if public | may contain unsafe terms/private-ish context | high | proposed `stale-and-archived-map.md` | Add stale/archive map and allowlist rules. |

## Immediate follow-ups

| Priority | Work | Tracking |
|---|---|---|
| 1 | Sync website and pilot-ui public/demo copy with current Phase 2 truth. | [#1779](https://github.com/InterCooperative-Network/icn/issues/1779) |
| 2 | Add subsystem maps for identity/crypto and network/gossip. | [#1689](https://github.com/InterCooperative-Network/icn/issues/1689) |
| 3 | Add service hosting, CCL, Tool Commons, and domain infrastructure maps. | [#1689](https://github.com/InterCooperative-Network/icn/issues/1689) |
| 4 | Add stale/archive map and public-surface drift rules. | [#1689](https://github.com/InterCooperative-Network/icn/issues/1689), #1779 follow-up |
| 5 | Prepare governance proposal/vote fixture slice. | [#1777](https://github.com/InterCooperative-Network/icn/issues/1777) |

## Coverage rules for future maps

A future map should not be accepted as complete unless it:

- names its source-of-truth anchors;
- uses the status vocabulary above;
- distinguishes runtime evidence from design direction;
- states public/demo visibility;
- states package and privacy boundaries;
- lists drift risks and non-claims;
- links to the next issue/PR when work remains.

## Non-claims

This matrix does not claim:

- Phase 2 completion;
- formal NYCN pilot status;
- production readiness;
- live federation;
- implemented service hosting;
- live Tool Commons;
- complete mobile/member UX;
- full fixture-backed demo mode.
