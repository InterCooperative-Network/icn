---
Status: design-direction
Authority: architecture (forward-direction; not yet normative)
Canonical: no
Last Reviewed: 2026-04-27
---

# Cooperative Tool Commons

> **Status: draft / design direction / roadmap.** Names a future tool ecosystem for ICN. Items marked _planned_ are not in the repo today. Companion to [`COOPERATIVE_DOMAIN_INFRASTRUCTURE.md`](COOPERATIVE_DOMAIN_INFRASTRUCTURE.md).

## Summary

ICN should support a **tool commons**: a set of installable / forkable tools that run on institution-controlled nodes (or governed service nodes) and operate on **institution-owned state**. Tools render, edit, summarize, import, publish, or assist. They never own institutional truth.

> **Core principle:** tools do not own institutional truth. The domain/node does.

A tool acts through **scoped capabilities** granted by the domain, and emits **receipts** for meaningful institutional transitions. Tools are replaceable, forkable, and suspendable without trapping the data they operated on.

## Core base tools

The base tools below are **planned identifiers**. They are framed in terms that match the existing ICN runtime (charters, governance, meetings, action items, receipts) and the future domain infrastructure (workspaces, scoped vaults, agreements, DNS bindings).

| Tool | What it does |
|---|---|
| `icn-domain-admin` | Manage domain policy, members, roles, devices, service identities, installed tools, DNS bindings, and security posture. The administrative surface for an `InstitutionalDomain`. |
| `icn-member-directory` | People, member organizations, roles, standing, delegates, contact visibility, member profiles. Reads from the existing person-directory overlay (PR #1626) and `/me/standing` (PR #1627). |
| `icn-governance` | Proposals, votes, consent, charters, decisions, receipts. Sits on top of `icn-governance` runtime (existing). |
| `icn-meetings` | Agendas, attendance, minutes, decisions, action items, meeting artifacts, meeting receipts. Sits on top of meeting management (PR #1543) and the meeting/attend receipt seam (PR #1663). |
| `icn-action-cards` | Member-facing "what needs me?" derived queue. Action cards are not stored tasks; they are views over proposals, meetings, action items, signals, obligations (per [ADR-0027](../adr/ADR-0027-action-card-contract.md)). |
| `icn-drive` (artifact vault) | Files, folders, versioning, access policy, public/private vaults, external source references. Front-end to the future `ArtifactRegistry` + `ScopedVault`. |
| `icn-docs` | Collaborative institutional documents tied to meetings, proposals, policies, public pages, playbooks, drafts, reports, and receipts. |
| `icn-tables` | Human-editable views over typed institutional records. **Not** random spreadsheets as source of truth — typed records first, table view second. |
| `icn-forms` | Intake, applications, surveys, accessibility/care requests, evaluations, registration, member onboarding. |
| `icn-calendar` | Meetings, milestones, event schedules, deadlines, public event calendars. |
| `icn-publish` (CMS) | Public web pages generated from approved records and artifacts. Emits a `PublicationReceipt` per publish. |
| `icn-search` | Permission-aware search across docs, records, meetings, receipts, artifacts, archives. |
| `icn-agreements` | Inter-org agreements, shared workspaces, service relationships, data-sharing agreements. UI for the eight `*Agreement` types named in the parent doc. |
| `icn-budget` (resources) | Obligations, allocations, settlements, positions, resource commitments, public/internal reports. Uses regulatory-safe vocabulary throughout. |
| `icn-signals` | Feedback, concerns, institutional reflexes, escalation, indicators, challenge paths. |
| `icn-compute-jobs` | Imports, summaries, translation, transcription, indexing, report generation, model workloads. Sits on the existing compute substrate ([ADR-0030](../adr/ADR-0030-compute-workload-manifest-and-authority-boundary.md), [ADR-0031](../adr/ADR-0031-commons-compute-admission-and-settlement-policy.md)). |
| `icn-directory` (public directory) | The institution's public-facing organizational directory. |

These compose. A "Summit operating dashboard" is not a new app — it is `icn-meetings` + `icn-tables` + `icn-action-cards` + `icn-publish` + `icn-forms` bound to a package's templates.

## Specialized tool suites

A specialized suite is **mostly a bundle of base tools plus package schemas, templates, and workflows.** Not a fork. Not an isolated app. The package boundary in [`INSTITUTION_PACKAGE_BOUNDARY.md`](INSTITUTION_PACKAGE_BOUNDARY.md) applies: generic shapes in ICN; institution-specific nouns in the package.

### Cooperative suite

For worker co-ops, food co-ops, housing co-ops, service co-ops, platform co-ops:

- member CRM
- customer / client CRM
- service desk
- work scheduling
- contribution tracking
- inventory
- orders / service fulfillment
- patronage / surplus tools
- board portal
- compliance binder
- training / onboarding

### Community suite

For civic communities, mutual aid networks, neighborhood assemblies, public associations:

- community CMS
- public forum / deliberation
- participatory budgeting
- issue reporting
- resource map
- mutual aid coordination
- events / assemblies
- surveys / feedback
- emergency response
- accessibility / care desk
- public archive

### Federation suite

For networks of co-ops, communities, institutions:

- member-organization registry
- inter-org agreements
- service catalog
- support programs
- shared procurement
- commons resource pool
- clearing / settlement views (regulatory-safe vocabulary)
- grants / sponsorship pipeline
- convening suite (events at federation scale)
- standards review
- mediation / dispute resolution
- federation directory

### Municipal suite

A specialized **community suite at local scale** plus a **federation suite at regional scale**:

- public issue reporting
- public records
- forms / permits
- service requests
- public meetings
- participatory budgeting
- public works dashboard
- local / regional resource maps
- inter-municipal agreements

### NYCN / Summit suite

The first specialized suite that ICN will run end-to-end as a proof body. NYCN-specific application docs live in the NYCN repo; this is a forward summary only.

- event CMS
- registration / intake forms
- sponsor pipeline
- speaker / session planning
- schedule builder
- food / logistics / tech tables
- accessibility / care intake
- meeting / action loop
- public publishing
- evaluation forms
- post-event report generation
- external bridge imports (Drive / Sheets)
- partner workspaces by agreement

## Third-party tools

The architecture supports developers and cooperatives writing their own tools.

### Tool install flow (planned)

```
tool package submitted
  ↓
manifest declares capabilities
  ↓
institution reviews authority request
  ↓
governance / admin approves install
  ↓
tool receives ServiceIdentity
  ↓
tool accesses only granted scopes
  ↓
tool actions produce receipts
  ↓
tool can be suspended, upgraded, forked, removed
```

A `ToolManifest` declares what the tool needs; a `ToolBinding` records how a specific institution has chosen to use it. Both are future objects.

### Tool runtime modes

- **Local node service** — runs on the institution's own ICN node
- **Frontend-only view** — no service identity, just a client over signed APIs
- **Compute workload** — runs as an `icn-compute-jobs` workload with workload-manifest constraints
- **Bridge adapter** — moves data between ICN and an external system; emits `BridgeImportReceipt`
- **Shared service run by another co-op / federation** — under an explicit `ServiceAgreement`
- **Workstation app** — runs on an enrolled `DeviceIdentity`
- **Mobile app** — same enrollment story, smaller form factor
- **Package template** — not a runtime, just shapes other tools instantiate

### Anti-capture rule

A tool **must never trap institutional state**. The institution owns:

- records (typed institutional data)
- artifacts (files, media, exports)
- receipts (the proof envelope)
- vaults (encrypted scoped storage)
- permissions (charter / role / `authority_scope` data)
- export and migration path

A tool that cannot answer "how does the institution leave you cleanly?" should not be installable.

## Capability grants and service identities

Every tool runs as a **service identity**. Tools never receive blanket access. The granted capability set is bound to:

- a `DomainPolicy` (what kinds of actions the domain allows tools to take at all)
- a `ToolBinding` (what *this institution* allows *this tool* to do)
- per-action authority checks (existing `authority_scope` plumbing extended to service identities)

This is the same authority discipline that members operate under, applied to non-human actors.

## Receipts emitted by tools

A tool emits a receipt when it produces an institutional transition. Examples (planned):

| Tool action | Receipt class |
|---|---|
| Publish a public page | `PublicationReceipt` |
| Import from Google Drive | `BridgeImportReceipt` |
| Settle an obligation against a position | existing settlement-receipt path (regulatory-safe vocabulary) |
| Mark a member as enrolled | governance/onboarding receipt |
| Run a deliberation summary model job | `icn-compute-jobs` workload receipt + advisory-class flag |

Receipts are how tools prove they did what they claimed; the receipt envelope ([ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md)) is the contract.

## Boundary discipline

- Tools running outside ICN (e.g. an external CMS) **may import** but never **export** institutional truth back as canonical state. Imported data carries an explicit external-source marker on the bridge receipt.
- Tools never write to a vault scope they were not explicitly granted.
- Tools that fail or are suspended must leave the institution's records in a recoverable state. The institution's data is never in the tool's database — it is in domain-owned storage that the tool happens to operate on.
- Specialized suites do **not** fork ICN primitives. They bind generic tools.

## Existing repo support

What is in the codebase today that this design builds on:

- `/me/standing` (PR #1627) and `/me/action-cards` (PR #1659) — the member-facing surfaces a future Action Cards tool will render
- Charter / role / `authority_scope` plumbing — the policy substrate a `DomainPolicy` will sit on
- Meeting management (PR #1543), action items (PR #1532), notification digest (PR #1547) — the runtime an `icn-meetings` tool will render
- Receipt envelope (ADR-0026) — what tool-emitted transitions plug into
- Compute substrate (ADR-0030 / ADR-0031) — what `icn-compute-jobs` and model workloads run on
- Institution package boundary ([`INSTITUTION_PACKAGE_BOUNDARY.md`](INSTITUTION_PACKAGE_BOUNDARY.md)) — the rule specialized suites obey

## Missing buildout

| Object | Role |
|---|---|
| `ToolManifest` | Declares capabilities, data touched, storage needs, privacy classes, UI surfaces, compute jobs, schemas, receipts emitted |
| `ToolBinding` | Per-institution configuration; "how `nycn` uses generic `icn-tables` for sponsor pipeline" lives in the NYCN package, not in ICN |
| `ToolInstall` lifecycle | submit → review → approve → bind → run → suspend → upgrade → fork → remove |
| Tool capability registry per `InstitutionalDomain` | What tools exist, what scopes they hold, what receipts they emit |
| Service identity audit trail | Per-tool history visible in the existing receipt query path |

Each of these will land via its own ADR or RFC. This document does not pre-commit shapes.

## Non-goals

- **No** runtime implementation in this PR.
- **No** specific tool implementation.
- **No** public website changes.
- **No** institution-specific tool nouns in ICN core.
- **No** "ICN App Store" framing — tool installation is a governance act, not a marketplace transaction.

## References

- [`COOPERATIVE_DOMAIN_INFRASTRUCTURE.md`](COOPERATIVE_DOMAIN_INFRASTRUCTURE.md) — parent design
- [`MODEL_WORKLOADS_AND_DELIBERATION.md`](MODEL_WORKLOADS_AND_DELIBERATION.md) — companion: model workloads as compute jobs
- [`DOMAIN_ROUTING_AND_DNS_BINDINGS.md`](DOMAIN_ROUTING_AND_DNS_BINDINGS.md) — companion: routing
- [`INSTITUTION_PACKAGE_BOUNDARY.md`](INSTITUTION_PACKAGE_BOUNDARY.md) — what stays in ICN vs in a package
- [ADR-0026](../adr/ADR-0026-receipt-and-provenance-proof-envelope.md), [ADR-0027](../adr/ADR-0027-action-card-contract.md), [ADR-0030](../adr/ADR-0030-compute-workload-manifest-and-authority-boundary.md), [ADR-0031](../adr/ADR-0031-commons-compute-admission-and-settlement-policy.md)
