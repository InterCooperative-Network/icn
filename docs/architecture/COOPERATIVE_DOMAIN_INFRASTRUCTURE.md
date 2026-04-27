---
Status: design-direction
Authority: architecture (forward-direction; not yet normative)
Canonical: no
Last Reviewed: 2026-04-27
---

# Cooperative Domain Infrastructure

> **Status: draft / design direction / roadmap.** This document names a future architecture direction for ICN. It is **not** an implementation report. Where the text describes objects, flows, or tools that do not yet exist in the repository, those are explicitly marked as future / planned. Existing implementation is cited against current source.

## Summary

ICN should provide **cooperative institutional infrastructure**: domains, tools, storage, compute, agreements, routing, and commons resource sharing — governed by ICN primitives (charters, roles, standing, agreements, receipts) and proven by an immutable receipt envelope.

The forward frame for ICN as a whole:

> ICN should become the open cooperative infrastructure layer where institutions run their own domains, install and fork their own tools, store and govern their own data, coordinate with other institutions through explicit agreements, contribute spare infrastructure to the commons, and prove institutional action through receipts.

Cooperative Domain Infrastructure is the **missing middle layer** between ICN's primitives/runtime (already shipping) and the future user-facing tool ecosystem. It is similar in *purpose* to "IAM + a domain controller + office infrastructure", but governed by **charters, roles, standing, agreements, and receipts** rather than corporate tenant administration.

## Repository boundaries

This document lives in the canonical ICN repo (`InterCooperative-Network/icn`). Cross-repo placement of follow-on material:

| Repo | What lives there |
|---|---|
| `icn` (this repo) | Generic primitives, runtime, ADRs, RFCs, public website source, and architectural direction documents like this one. **No** institution-specific nouns. |
| `nycn` | NYCN-specific application of these designs: institution package, domain bindings example, partner workspace examples, Summit operating playbook, Drive/Sheets bridge usage. NYCN-specific operating truth. |
| `icn-learn` | Teaching surface only. Links back to canonical truth here and operating truth in `nycn`. **Does not define ICN.** |

Boundary rule (per [`INSTITUTION_PACKAGE_BOUNDARY.md`](INSTITUTION_PACKAGE_BOUNDARY.md)): generic shapes go in ICN; institution-specific nouns stay in packages.

## Why this exists

Cooperative institutions today rely on fragmented external tools and private memory: Google Drive folders, ad-hoc spreadsheets, an event ticketing platform, a separate CRM, a CMS no one owns, a Slack with no archive policy. The failure modes are familiar: state captured by vendors, drift between what was decided and what was published, no auditable trail when authority is contested, and no clean migration path when a tool changes terms.

ICN should let cooperative institutions **govern, coordinate, remember, publish, compute, and federate on their own infrastructure**. Not as a SaaS product running for them, but as a substrate they run on hardware they control or hardware they share with peers under explicit agreements.

## Layer model

```
┌────────────────────────────────────────────────────────────────────┐
│  Learning layer (icn-learn)                                        │
│   teaches the system; does not define it                           │
├────────────────────────────────────────────────────────────────────┤
│  Workstations / endpoints                                          │
│   member shells, kiosks, event devices, node operator consoles     │
├────────────────────────────────────────────────────────────────────┤
│  Hybrid commons cloud                                              │
│   resource offers, local reserve policy, commons settlement        │
├────────────────────────────────────────────────────────────────────┤
│  Live nodes                                                        │
│   per-institution daemons holding live operational state           │
├────────────────────────────────────────────────────────────────────┤
│  Institution packages (e.g. nycn)                                  │
│   concrete institutional meaning, templates, fixtures, bindings    │
├────────────────────────────────────────────────────────────────────┤
│  Specialized tool suites                                           │
│   cooperative / community / federation / municipal / NYCN-Summit   │
├────────────────────────────────────────────────────────────────────┤
│  Tool commons                                                      │
│   icn-domain-admin, icn-meetings, icn-action-cards, icn-drive, …   │
├────────────────────────────────────────────────────────────────────┤
│  Cooperative Domain Infrastructure  ← this document                │
│   institutional domains, sessions, devices, services, vaults,      │
│   workspaces, artifact registry, agreements, DNS bindings          │
├────────────────────────────────────────────────────────────────────┤
│  Institutional runtime                                             │
│   charters, governance, meetings, action items, receipts, standing │
├────────────────────────────────────────────────────────────────────┤
│  Kernel / substrate                                                │
│   identity, gossip, ledger journal, compute substrate, gateway     │
└────────────────────────────────────────────────────────────────────┘
```

The layers below "Cooperative Domain Infrastructure" exist today (see [Existing repo support](#existing-repo-support)). The layers at and above are mostly future / planned.

## Institutional domains

An **InstitutionalDomain** is the ICN authority/security/governance boundary for an institution. Examples (illustrative): `nycn`, `electric-city-food-coop`, `summit-2026`. An institutional domain owns members, roles, standing, data, tools, agreements, and receipts.

> An institutional domain is **not** a DNS domain. The institutional domain is the ICN object/scope; the DNS domain is a routable address. See [Routing and DNS bindings](#routing-and-dns-bindings).

### Future objects (not yet in the repo)

| Object | Purpose |
|---|---|
| `InstitutionalDomain` | Top-level authority/scope object owning the institution's members, devices, services, vaults, workspaces, agreements, and receipts. |
| `DomainPolicy` | Rules for the domain: who may join, what devices may enroll, what tools may run, what privacy classes exist, how data is shared, when access expires, what requires receipts, what authority is needed for tool/service actions. |
| `DomainSession` | A member or service's active authenticated/authorized session inside an institutional domain. |
| `DeviceIdentity` | A DID (or equivalent) identity for a workstation, phone, kiosk, server, node, or event check-in device. |
| `DeviceEnrollment` | The process and record by which an institution approves a device to participate in its domain. Required for future ICN workstations, kiosks, event devices, office computers, and node operator machines. |
| `ServiceIdentity` | An identity for a tool, bridge, model runner, compute service, or import adapter. A service identity gets *scoped capabilities*; it never gets blanket access. |

These objects are **planned**. The `Charter` / `RoleAssignment` / `authority_scope` plumbing already in the codebase is the substrate they will sit on.

## Routing and DNS bindings

See the focused doc: [`DOMAIN_ROUTING_AND_DNS_BINDINGS.md`](DOMAIN_ROUTING_AND_DNS_BINDINGS.md).

Quick frame:

| Concept | Example | Notes |
|---|---|---|
| Institutional domain | `nycn` | ICN authority boundary |
| ICN utility route | `nycn.icn.zone` | ICN-managed default; **utility, not canonical truth** |
| Short route | `icn.zone/n/nycn` | Human-friendly redirect |
| Custom public domain | An institution's own DNS, e.g. NYCN's public site | Institution-owned; bound to the institutional domain via receipt |

**`icn.zone` is the primary public ICN surface** for routing, identity, and short-link authority over ICN's own surface. Institutional domains rooted under it (e.g. `nycn.icn.zone`) are utility routes — not authority over the institution itself. Learning material at `learn.icn.zone` is teaching, not authority.

Verification flow (planned):

```
request custom domain → add TXT challenge → verify control →
  approve binding → provision route/cert → emit binding receipt
```

The verification record (`DnsBinding` + a future receipt envelope entry) is what makes the binding auditable.

## Domain authority

A domain controls authority over **what** can be done by **whom** with **which device or service**. The future surface combines the existing charter/role/standing primitives (already in `icn-governance`) with new device and service identity primitives:

```
DomainPolicy
   ↓ authorizes
DomainSession  ←— authenticates ←— member DID + DeviceIdentity
   ↓                                                  
   └─► action attempt
        ↓
   DomainPolicy.evaluate(session, action) → allow/deny + scoped capability
        ↓ if allowed
   action runs → receipt emitted
```

Action cards (`/me/action-cards`, ADR-0027) are how the holder *discovers* what they have authority to do; the proof loop `standing → action card → authorized action → receipt` is what makes the surface honest. Receipt-bearing source paths today: `proposal`/`vote`, `action_item`/`complete`, `meeting`/`attend` (the latter as of 2026-04-27). Reserved-but-pending: `signal_rule` (gated on icn#1631), `obligation_lifecycle` (gated on icn#1634).

## Workspaces

A `Workspace` is a **bounded collaboration space** under a domain or agreement. Future use cases (illustrative, fictional):

- a planning workspace for a multi-day convening, scoped to its organizing committee
- a finance committee workspace, scoped to finance role-holders
- an accessibility / care workspace, scoped to the care committee and care-requesting members only
- a shared local-logistics workspace co-owned by two cooperatives under a `SharedWorkspaceAgreement`

Workspaces are not chat channels and not file folders — they are scoped contexts inside which **tools, artifacts, vaults, and agreements compose**.

## Artifact registry and scoped vaults

Two distinct layers:

### `ArtifactRegistry` (planned)

Content-addressed registry for files and documents: docs, PDFs, images, receipts, meeting recordings, attachments, sponsor logos, venue maps, public reports, transcripts, exports. Stores metadata, hashes, versions, ownership, access policy, retention, and provenance.

### `ScopedVault` (planned)

Encrypted storage scoped to a domain, committee, role, member, or agreement. Examples (fictional):

| Vault | Scope |
|---|---|
| Finance vault | Finance committee role-holders only |
| Accessibility / care vault | Care committee + care-requesting member |
| Public archive | Domain-public, post-publication-receipt |
| Internal organizer vault | Operating-body role-holders |
| Personal / member-controlled vault | Single member, recoverable per domain policy |

Rule: **the smallest competent scope owns the data.** Larger scopes get proofs, summaries, indexes, or explicit shared access — not automatic possession.

## Tool commons

See the focused doc: [`COOPERATIVE_TOOL_COMMONS.md`](COOPERATIVE_TOOL_COMMONS.md).

Brief frame: ICN should support a **tool commons** — installable / forkable tools that run on institution-controlled nodes (or governed service nodes) and operate on **institution-owned state**. A tool can render, edit, summarize, import, publish, or assist. A tool **never owns institutional truth**; the domain/node does.

Core base tools (planned identifiers, illustrative):

```
icn-domain-admin       icn-meetings        icn-drive (artifact vault)
icn-member-directory   icn-action-cards    icn-docs
icn-governance         icn-tables          icn-forms
icn-calendar           icn-publish (CMS)   icn-search
icn-agreements         icn-budget          icn-signals
icn-compute-jobs       icn-directory
```

Specialized suites (cooperative / community / federation / municipal / NYCN-Summit) compose these base tools plus package-specific schemas, templates, and workflows. They are **not** isolated apps.

## Agreements

Federation does **not** mean dumping all data into one shared tenant. Inter-org collaboration happens through explicit, scoped, receipted agreements.

### Future agreement types

| Type | Purpose |
|---|---|
| `MembershipAgreement` | An entity's membership in another (cooperative joins federation, etc.) |
| `ServiceAgreement` | One institution provides a service to another |
| `SharedWorkspaceAgreement` | A bounded shared workspace with explicit scope |
| `DataSharingAgreement` | Specific records / vaults / artifact classes shared, with privacy class and expiry |
| `ComputeAgreement` | One node runs a compute workload for another under defined limits |
| `FiscalBridgeAgreement` | One institution acts as fiscal bridge for another (see ADR-0001 candidate, RFC-0013) |
| `ResourceSharingAgreement` | Spare-capacity sharing into a commons pool |
| `TemporaryAuthorityAgreement` | Time-bounded delegated authority for an event or task |

**Phrase to keep:** *Domain-local by default. Shared by explicit agreement. Receipted always.*

Today, `Charter`, `RoleAssignment`, and `authority_scope` provide the underlying authority plumbing. The agreement object itself is future buildout.

## Compute and model workloads

See the focused doc: [`MODEL_WORKLOADS_AND_DELIBERATION.md`](MODEL_WORKLOADS_AND_DELIBERATION.md).

Core rule: **models can assist institutions; they cannot govern institutions.**

Compute already exists in the substrate as constrained execution with workload manifest, fuel/resource limits, privacy/determinism classes, scope, resource profile, and optional mandate reference (see [ADR-0030](../adr/ADR-0030-compute-workload-manifest-and-authority-boundary.md), [ADR-0031](../adr/ADR-0031-commons-compute-admission-and-settlement-policy.md)). What is future:

- `ModelArtifact` registry with hashing and versioning
- explicit advisory-vs-deterministic classification on workloads
- a deliberation-packet pattern (model produces source-linked draft → governance authorizes → it becomes institutional state)

## Hybrid commons cloud

ICN can eventually become a **cooperative hybrid cloud**: each org node runs local services for its institution **and** contributes safe unused resources to a commons pool.

Future objects:

```
ResourceOffer              CommonsSettlementReceipt
ResourceContributionPolicy NodeCapabilityRegistry
LocalReservePolicy         StorageReplicationOffer
WorkloadPlacementPolicy    MediaRelayOffer
UsageReceipt               ChallengePath
```

**Rule: local sovereignty first; commons contribution second.** An office node must reserve capacity for its own domain (docs, vaults, meetings, identity, backups) **before** offering surplus to the commons.

## Workstations

Long-term endpoint direction (do not start here):

```
desktop / member shell  →  Linux install profile  →  managed workstation
   enrollment  →  live USB / event kiosk  →  immutable workstation image
   →  possible ICN distro
```

Workstation modes (future): member workstation, organizer workstation, finance workstation, public kiosk, event check-in station, node operator console, classroom / training station, offline field kit.

**Warning:** do not start by building "ICN Linux." Start by making ICN work well on ordinary devices. Then package. Then image. Then maybe distro.

## NYCN / Summit proof path

NYCN is the first institution ecosystem package and the first proof body. The 2026 New York Cooperative Summit (Schenectady, NY, 2026-10-17) is the first pressure test.

NYCN-correct facts (preserve these — they replace prior placeholder text):

| Surface | Value |
|---|---|
| Public website | The NYCN repo's documented public domain (NYCN-owned) |
| Future / default ICN utility route | `nycn.icn.zone` |
| Possible short route | `icn.zone/n/nycn` |
| Summit | 2026-10-17, Schenectady, NY |
| Current Drive / Sheets | Bootstrap / operational source material — **not** the final architecture |

NYCN will need: committees, sponsor pipeline, speaker / session planning, registration / intake, accessibility / care intake, logistics / food / tech planning, public website publishing, meeting / action loop, evaluation / debrief loop, partner workspaces, bridge imports from external sources, action cards, receipts.

**The goal is not to sync Google Drive into a better database.** The goal is for NYCN to eventually run its institutional life on ICN — meetings, decisions, action items, records, docs, tools, agreements, receipts, publishing, and federation relationships.

NYCN-specific application of this architecture lives in the NYCN repo, not here. Follow-up docs are tracked under [Follow-up TODOs](#follow-up-todos).

## Existing repo support

What is already in the codebase as substrate this design will sit on (citations are to the `icn/` workspace at the time of this doc's last review):

| Capability | Where |
|---|---|
| Live charter activation | `icn-governance` charter primitives + activation endpoint (PR #1624) |
| Person-directory overlay (DID binding) | PR #1626 |
| `/me/standing` read model | PR #1627 |
| `authority_scope` plumbed through `assign_role` | PR #1630 |
| `/me/action-cards` member endpoint | PR #1659 (closed source/action enums per [ADR-0027](../adr/ADR-0027-action-card-contract.md)) |
| Proposal/vote action card → `GovernanceDecisionReceipt` proof linkage | PR #1660 |
| Action-item / complete → `ActionItemCompletionReceipt` (ADR-0026 Layer 2) | PR #1661 |
| Meeting / attend → `MeetingAttendanceReceipt` (ADR-0026 Layer 2) | PR #1663 |
| Meeting management (schedule, agenda, attendance, minutes) | PR #1543 |
| Action items + decision-to-action bridge | PRs #1532, #1547 |
| Receipts and provenance proof envelope | [ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md) |
| Compute substrate (workload manifest, fuel/resource limits) | [ADR-0030](../adr/ADR-0030-compute-workload-manifest-and-authority-boundary.md) |
| Commons compute (admission and settlement policy) | [ADR-0031](../adr/ADR-0031-commons-compute-admission-and-settlement-policy.md) |
| Institution package boundary | [`INSTITUTION_PACKAGE_BOUNDARY.md`](INSTITUTION_PACKAGE_BOUNDARY.md) |
| Public surface and learning repo architecture | [RFC-0015](../rfcs/RFC-0015-public-surface-and-learning-repo-architecture.md) |
| NYCN package + smoke flow | NYCN repo (`institution/`, `tools/`) |
| icn-learn teaching boundary | `icn-learn` repo (`README.md`, `docs/WHERE_TRUTH_LIVES.md`) |

For the canonical implementation snapshot at any point in time, see [`docs/STATE.md`](../STATE.md).

## Missing buildout

Future / planned, not in the repo today:

- Domain identity & sessions: `InstitutionalDomain`, `DomainPolicy`, `DomainSession`
- Devices & services: `DeviceIdentity`, `DeviceEnrollment`, `ServiceIdentity`
- Spaces: `Workspace`, `ArtifactRegistry`, `ScopedVault`
- Tools: `ToolManifest`, `ToolBinding`, third-party tool ecosystem, tool install/upgrade/fork/suspend lifecycle
- Agreements: `AgreementGrant`, the eight `*Agreement` types listed above
- Routing: `DnsBinding`, verification receipts
- Publication & bridges: `PublicationReceipt`, `BridgeImportReceipt`
- Commons cloud: `ResourceOffer`, `LocalReservePolicy`, `WorkloadPlacementPolicy`, `UsageReceipt`, `CommonsSettlementReceipt`, `ChallengePath`
- Endpoint path: enrollment, kiosks, workstations, eventual ICN distro
- Action-card source paths still pending under #1646: `signal_rule` (gated on icn#1631), `obligation_lifecycle` (gated on icn#1634)

Each item should land via its own ADR or RFC. This document does not pre-commit shapes; it names the gaps.

## Non-goals

- **No** runtime implementation in this PR. This is documentation only.
- **No** public website changes from this PR.
- **No** institution-specific nouns (NYCN / Summit / partner names) introduced into ICN core.
- **No** model governance authority. Models assist; governance authorizes.
- **No** external tool as source of truth. Tools operate on institution-owned state.
- **No** ICN-as-platform-landlord. `icn.zone` is utility routing, not authority.

## Build phases

Sequential and additive — none of these phases are committed merge orders, but each implies the previous.

| Phase | What | Why |
|---|---|---|
| A | Design docs (this PR + companions) | Name the architecture before coding |
| B | Artifact / workspace proof | Smallest end-to-end slice: artifact upload → scoped access → publication receipt |
| C | Meeting / action / document loop | Existing meeting+action runtime + a docs surface above it |
| D | NYCN Summit tool slice | First specialized suite proving the package boundary holds in production |
| E | Inter-org workspace agreement | First `SharedWorkspaceAgreement` + scoped capability proof |
| F | Advisory compute / model workload | Source-linked deliberation packets + advisory class |
| G | Commons resource offer | First `ResourceOffer` + `LocalReservePolicy` enforcement |
| H | Tool package ecosystem | `ToolManifest` + install/fork/suspend lifecycle |
| I | Workstation path | Member shell, kiosk image, eventually ICN distro |

## Non-negotiable rules

- **ICN is infrastructure, not platform.** Institutions run on it; they are not tenants of it.
- **Tools do not own truth.** The domain/node does. Every tool must support clean export and migration.
- **Compute assists; governance authorizes.** Models can produce drafts, summaries, classifications, suggestions. Only an authorized institution can turn that output into institutional state.
- **Agreements scope sharing.** Cross-institution access is never automatic.
- **Local sovereignty first; commons contribution second.** A node's local domain comes before its commons offer.
- **Receipts prove institutional transitions.** If a transition matters, it produces a receipt.
- **Accessibility is first-class.** Every member-facing surface respects [ADR-0028](../adr/ADR-0028-accessibility-baseline-for-member-interfaces.md).
- **Regulatory-safe vocabulary for ICN-native primitives.** Use obligation / allocation / settlement / unit / position / settlement asset / external settlement instruction / bridge receipt. Avoid payment / wallet / balance / currency for ICN-native primitives.

## Follow-up TODOs

This PR creates the four ICN-side architectural docs as a coherent first slice. The cross-repo follow-ups intentionally live in their own PRs to keep the boundary clean:

- **`nycn` repo**:
  - `docs/architecture/COOPERATIVE_OFFICE_TOOLS.md` — NYCN application of the tool commons.
  - `docs/architecture/DOMAIN_AND_AGREEMENT_MODEL.md` — NYCN domain, Summit 2026 workspace, committees, partner agreements, public/private boundaries.
  - `docs/architecture/HYBRID_COMMONS_INFRA.md` — how NYCN/org nodes could run local services and contribute surplus.
  - `institution/tool-bindings.example.yaml` — example tool bindings (fictional only).
  - `institution/shared-workspaces.example.yaml` — example partner workspaces (fictional only).
  - `institution/domain-bindings.example.yaml` — `nycn.icn.zone`, `icn.zone/n/nycn`, NYCN public domain, possible Summit path. Marked planned/example, not live proof.
- **`icn-learn` repo**:
  - `packets/cooperative-domain-infrastructure.md` — teaching summary linking back here.
  - `packets/tool-commons.md` — teaching summary linking back to [`COOPERATIVE_TOOL_COMMONS.md`](COOPERATIVE_TOOL_COMMONS.md).
  - `packets/model-assisted-deliberation.md` — teaching summary linking back to [`MODEL_WORKLOADS_AND_DELIBERATION.md`](MODEL_WORKLOADS_AND_DELIBERATION.md).
  - Each must say it teaches ICN and links to canonical docs; it does not define ICN.

These follow-up PRs MUST NOT introduce NYCN/Summit nouns into the ICN public website, and MUST NOT add `learn.icn.zone` links to the public ICN site.

## References

- Companion architectural docs in this PR:
  - [`COOPERATIVE_TOOL_COMMONS.md`](COOPERATIVE_TOOL_COMMONS.md)
  - [`MODEL_WORKLOADS_AND_DELIBERATION.md`](MODEL_WORKLOADS_AND_DELIBERATION.md)
  - [`DOMAIN_ROUTING_AND_DNS_BINDINGS.md`](DOMAIN_ROUTING_AND_DNS_BINDINGS.md)
- Existing architecture: [`INSTITUTION_PACKAGE_BOUNDARY.md`](INSTITUTION_PACKAGE_BOUNDARY.md), [`KERNEL_APP_SEPARATION.md`](KERNEL_APP_SEPARATION.md), [`IDENTITY_MEMBERSHIP_ARCHITECTURE.md`](IDENTITY_MEMBERSHIP_ARCHITECTURE.md), [`INSTITUTIONAL_FEEDBACK_AND_SUPPORT_PRIMITIVES.md`](INSTITUTIONAL_FEEDBACK_AND_SUPPORT_PRIMITIVES.md)
- ADRs: [ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md), [ADR-0027](../adr/ADR-0027-action-card-contract.md), [ADR-0030](../adr/ADR-0030-compute-workload-manifest-and-authority-boundary.md), [ADR-0031](../adr/ADR-0031-commons-compute-admission-and-settlement-policy.md)
- RFCs: [RFC-0015](../rfcs/RFC-0015-public-surface-and-learning-repo-architecture.md)
- Implementation snapshot: [`docs/STATE.md`](../STATE.md)
