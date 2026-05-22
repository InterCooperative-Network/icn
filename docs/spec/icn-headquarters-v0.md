---
Status: normative
Authority: spec / product-composition (defines the v0 composition contract that ties together the public website truth boundary, `docs/spec/member-shell-v0.md` (#1830), `docs/spec/steward-cockpit-v0.md` (#1831), `docs/pilots/no-cli-organizer-member-rehearsal-workflow.md` (#1724/#1726), `docs/architecture/SERVICE_HOSTING_MODEL.md`, `docs/architecture/AUTH_BRIDGE_AND_DID_LOGIN.md`, `docs/architecture/PROTOCOL_SELECTION_FOR_MEMBER_SERVICES.md`, `docs/strategy/SOVEREIGN_FORGE.md`, `docs/ops/FORGEJO_DEPLOYMENT_PLAN.md`, and `docs/ops/SERVICE_GOVERNANCE_TEMPLATE.md` into a single top-level ICN civic shell concept; composition only — does not redefine any of those documents. The non-replacement rules and the explicit non-goals are normative; everything else is design direction at v0.)
Canonical: no
Last Reviewed: 2026-05-21
---

# ICN Civic Shell v0

> **Status: spec, v0, design / composition only.** This document defines the **ICN Civic Shell**: the top-level public-plus-logged-in institutional operating surface that composes existing ICN surfaces into one recognizable place. No implementation. No new endpoint. No new auth system. No deployment. No production-readiness claim. No live federation claim. No formal NYCN pilot claim. No Phase 2 completion claim. The PR introducing this doc opens a composition contract; it does not close any sibling issue and does not authorize infrastructure change.

<!-- truth-class: normative -->

## Naming correction

The first draft called this surface **ICN Headquarters**. That metaphor is rejected here.

ICN is not building a command center, executive office, admin console, or single place where institutional power lives. A headquarters implies central command. ICN needs the opposite: a legible civic entry point that routes people into bounded, authority-aware surfaces without collapsing the institution into an administrator's dashboard.

The correct v0 name is **ICN Civic Shell**.

A Civic Shell is the public and logged-in **wayfinding layer** for an ICN institution. It gives people a recognizable place to enter, understand, participate, build, steward, communicate, and review records. It does not become the source of authority. Authority remains in standing, mandates, governance decisions, receipts, and the underlying ICN substrate.

Future cleanup before merge should rename the file and registry/index entries from the draft `headquarters` name to `icn-civic-shell-v0.md`. This revision changes the concept first so reviewers can evaluate the corrected frame.

## Purpose

A cooperative substrate needs recognizable civic places, not scattered tools and not command headquarters. Today, ICN surfaces are spread across:

- the public website at `intercooperative.network`,
- the per-domain member shell (`docs/spec/member-shell-v0.md`),
- the per-domain steward cockpit (`docs/spec/steward-cockpit-v0.md`),
- the no-CLI organizer/member rehearsal path (`docs/pilots/no-cli-organizer-member-rehearsal-workflow.md`),
- the forge / repository / project-record layer (`docs/strategy/SOVEREIGN_FORGE.md`, `docs/ops/FORGEJO_DEPLOYMENT_PLAN.md`),
- the service-hosting and operational-governance layer (`docs/architecture/SERVICE_HOSTING_MODEL.md`, `docs/ops/SERVICE_GOVERNANCE_TEMPLATE.md`),
- and the auth/session bridge (`docs/architecture/AUTH_BRIDGE_AND_DID_LOGIN.md`).

The **ICN Civic Shell** composes those surfaces into one navigable civic operating space for ICN itself.

Publicly, it shows truth-bounded status, development updates, service-health posture, the current phase, the roadmap, public forge/project windows, docs, and onboarding routes.

Privately, it gives members, organizers, developers, stewards, and operators context-aware access to their standing, active domain, active role, authority scope, action cards, records, forge/work surfaces, communications, operations posture, and privacy posture.

This spec defines that composition. It does **not** redefine the member shell, steward cockpit, public website, forge, auth bridge, service-hosting model, or any operational system. The Civic Shell routes into them, references them, and summarizes them; it never replaces them.

## Civic-place metaphor

The Civic Shell uses a spatial metaphor because institutions are easier to understand when people know where they are and what kind of action belongs there.

```text
Public Window  →  Civic Lobby  →  Member Desk
                                 Governance Room
                                 Workroom
                                 Records Room
                                 Forge Room
                                 Operations Control Room
                                 Communications Room
                                 Vault / Privacy Posture
                                 Settings / Identity
```

The Civic Shell is **not** the government, not the co-op, not the federation, not the forge, and not the operator cockpit. It is the entry hall and wayfinding layer that makes the institution legible.

The **Forge Room** is where infrastructure, code, technical artifacts, implementation work, templates, reviews, RFCs, ADRs, and shared systems are built. It is one civic space inside the shell, not the universal center of every cooperative, community, or federation.

Different institutional forms may eventually have their own named civic places. A cooperative, a community, and a federation do not have to share one generic building metaphor. This spec only names the ICN-level shell that composes ICN's current public, member, steward, forge, communication, and operational surfaces.

## Scope and non-goals

### In scope

- A composition contract that ties the public site, per-domain member shell, per-domain steward cockpit, no-CLI organizer/member path, forge room, operations control room, communications room, and vault/privacy posture into one ICN civic shell concept.
- A bounded space model that gives each major function a named place without creating a command center.
- Public/private boundary rules anchored to ADR-0032 (Website Truth Boundary) and ADR-0033 (Public Maturity Claims and Evidence Links).
- A domain/route doctrine for `intercooperative.network` (canonical public truth) vs `icn.zone` (short operational/access/discovery), without turning the short domain into a second marketing site.
- An explicit non-replacement contract against `docs/spec/member-shell-v0.md` and `docs/spec/steward-cockpit-v0.md`.
- A v0 status/proof-label discipline that hooks into the proof-level taxonomy and capability matrix tracked in `#1796`.
- Follow-up issue suggestions only. No issues are opened by this PR.

### Not in scope

- Not a public-website rebuild, redesign, or new content set.
- Not a member-shell redefinition.
- Not a steward-cockpit redefinition.
- Not a node-operator civic-role surface (`#1613`).
- Not a sovereign-forge canonical-cutover decision.
- Not a Forgejo deployment.
- Not a Keycloak/authentik/Ory/ZITADEL/Kanidm deployment or auth implementation.
- Not an n8n workflow build.
- Not a Matrix launch or bridge deployment.
- Not a Uptime Kuma policy change.
- Not a `network-ops` mutation.
- Not a DNS, K3s, VLAN, firewall, or public-edge mutation.
- Not a private-data ingestion path.
- Not a NYCN-specific or named-partner framing.
- Not a closure of `#1724`, `#1726`, `#1710`, `#1613`, `#1796`, `#1779`, `#1837`, or `#1873`.
- Not a Phase 2 completion claim.
- Not a production-readiness claim, live-federation claim, or formal-pilot claim.

## Domain and route doctrine

> **Operational context, not ICN product doctrine.** The points below summarize safe architectural facts about how ICN domains and segmented network concepts are currently and prospectively used. `network-ops` was not read locally for the original PR. Nothing here makes `network-ops` public ICN truth.

- **`intercooperative.network`** is ICN's canonical public identity and truth domain. It remains the truth boundary defined by ADR-0032.
- **`icn.zone`** is the short operational/access/discovery domain. It is not a second marketing site. Its job is fast routing into action surfaces, not republishing ICN's public narrative.
- Conceptual routes such as `/status`, `/forge`, `/dev`, `/docs`, `/join`, and `/dashboard` are examples only until a separate PR proves any route live.
- **ICN-PRIVATE** and **ICN-EDGE** segmentation are operational context, not product doctrine. The public Civic Shell does not depend on any specific VLAN number.
- Admin and control-plane surfaces remain private overlay. The public site never carries an admin login flow.
- No DNS record is created or changed by this spec.

## Public exterior

Unauthenticated visitors see the public website plus a small, truthful set of civic status surfaces:

- **What is real now** — maturity-banded subsystem claims per ADR-0032. No unbanded "working" claims.
- **Development updates** — phase/status summaries derived from `docs/PHASE_PROGRESS.md` and `docs/STATE.md`.
- **Public roadmap and current phase** — the existing website roadmap/status model, not a parallel roadmap.
- **Public service-health posture** — public-impact health summaries only; no private node-state dump.
- **Incidents and maintenance notices** — only when they affect public or member-facing surfaces; no private incident bodies.
- **Public forge window** — a read-only project-record window. GitHub is the current external adapter where true; Forgejo is the target only when its cutover gates are met.
- **Documentation and onboarding routes** — docs, get-involved paths, community links, and truth-boundary explanation.

The public exterior is not a brochure pretending to be an institution. It is ICN's public truth surface made navigable.

## Logged-in interior

The logged-in Civic Shell is **read-first** and **coordination-first**. Mutation paths route into existing v0-conformant surfaces rather than being reinvented here.

A logged-in visitor sees:

- **Identity** — DID/member/session projection; OIDC session is not authority.
- **Active domain / organization** — the `InstitutionalDomain` currently in scope.
- **Active role** — Representation / Execution / Attestation role context where applicable.
- **Authority scope** — a visible summary of what the viewer can do, with routes into the authoritative member/steward surfaces.
- **Member dashboard** — composed from the member shell, not duplicated.
- **Action cards** — count and summary only at the shell layer; full action happens in the member shell.
- **Governance room** — proposals, decisions, mandates, challenge windows, and accepted-vs-applied status.
- **Workroom** — action items, assignments, completion status, receipts, and evidence.
- **Records room** — receipts, documents, evidence packets, artifact references, and access/challenge paths.
- **Forge room** — repositories, issues, reviews, CI, ADRs, RFCs, releases, and implementation work.
- **Operations control room** — steward-only cockpit route, never a public admin panel.
- **Communications room** — Matrix/community/chat/announcement surfaces as coordination, not governance.
- **Vault / privacy posture** — existence, scope, access path, and redaction/export posture, never private object bodies.
- **Settings / identity** — devices, key status, language/accessibility preferences, and private-overlay-bound accommodation posture.

## Relationship to Member Shell v0

`docs/spec/member-shell-v0.md` is the primary participation surface. The Civic Shell may contain or route into it, but must not duplicate or redefine:

- `/me/standing`,
- `/me/action-cards`,
- the signing/confirmation flow,
- receipt rendering tiers,
- offline/draft-intent/sent/confirmed labeling,
- closed member-facing vocabularies,
- the accessibility gate.

When a member acts, the action goes through the member shell contract. The Civic Shell provides orientation and routing, not a parallel participation protocol.

## Relationship to Steward Cockpit v0

`docs/spec/steward-cockpit-v0.md` is the steward/operator complement. The Civic Shell may route qualified viewers into it as the Operations Control Room, but must not redefine:

- the cockpit's twelve surfaces,
- closed operator-state vocabulary,
- fourteen operator scenarios,
- member-impact summary mapping,
- technical detail boundaries.

No steward action surfaced in the Civic Shell becomes a god-mode admin button. Every steward-visible action must preserve authority basis, expected receipt/evidence class, member-impact summary, and reversibility/challenge posture where applicable.

A viewer without steward standing in the active domain must not see steward-only surfaces or actions.

## Relationship to the no-CLI organizer/member workflow

The no-CLI organizer/member workflow remains the guided browser/mobile-first workflow for organizers and members. The Civic Shell is the long-term home for that experience, but does not implement it in this PR.

The shell preserves the workflow's hard boundaries:

- Organizer, steward/operator, and future-member paths stay distinct.
- Preview/review happens before mutation.
- Mutation comes last.
- CLI remains steward/operator backend and verifier tooling, not the default member path.
- Evidence packets are repo-safe by default.

## Relationship to service hosting

The Civic Shell surfaces services without laundering their stage.

- **Hosted** means a service runs.
- **Governed** means service admin, backups, upgrades, routes, and transitions are subject to institutional policy/receipts.
- **ICN-native** means the service participates in ICN primitives and emits ICN-native receipts.

The Civic Shell must name each visible service with its stage. Service-local admin panels are not institutional authority. A service is not ICN-native until it actually emits ICN-native receipts according to the service-hosting model.

## Relationship to auth bridge and DID login

The Civic Shell reaffirms the auth bridge rule:

```text
OIDC authenticates sessions.
ICN authorizes institutional power.
Receipts prove institutional transitions.
```

Keycloak, authentik, Ory, ZITADEL, Kanidm, or any other IdP may carry browser-session state where operationally used. None of them grants ICN authority by itself.

Groups are projection state, not authority. The correct direction is:

```text
DID / ICN standing / mandate → short-lived service/session claim
```

The forbidden direction is:

```text
IdP group → ICN authority
```

The long-term target is local-device key unlock: biometric/PIN/passkey unlocks the device; the device protects the private key; the private key signs the action; the network verifies the signature. Biometrics and PINs are never identity themselves.

## Civic spaces

### Public Window

Public truth, status, updates, docs, roadmap, public forge window, and onboarding. ADR-0032 and ADR-0033 apply.

### Civic Lobby

Logged-in landing space: who am I, what domain am I in, what role am I acting under, what needs attention.

### Member Desk

Standing, action cards, votes, assignments, receipts, and signing flow, all composed from Member Shell v0.

### Governance Room

Proposals, decisions, mandates, challenge windows, accepted-vs-applied status, and dispatch/application evidence.

### Workroom

Action items, assignments, work status, completion receipts, and evidence. No "mark complete without receipt" shortcut.

### Records Room

Documents, policies, receipts, evidence packets, signed objects, artifact references, access paths, and challenge paths. Private body bytes never render.

### Forge Room

Infrastructure-building space: repositories, issues, pull/merge requests, reviews, CI, releases, RFCs, ADRs, maintainership authority, and shared implementation work. GitHub is current adapter where true; Forgejo is future canonical work-record target only after cutover gates land.

### Operations Control Room

Steward-only. Composes Steward Cockpit v0. No public admin endpoints. No surveillance console. No private content previews.

### Communications Room

Matrix, chat, announcements, onboarding discussions, and bridge boundaries. Matrix is real-time coordination, not governance authority. A room cannot ratify a proposal, mutate a mandate, or close an action item.

### Vault / Privacy Posture

Private overlay posture, scoped vault posture, access receipts, export receipts, redaction state, and challenge/access path. Posture, not content.

## Notifications and action cards

Notifications in the Civic Shell are institutional notices, not social-media noise. Every notification must carry:

1. what happened,
2. why the viewer is notified,
3. which domain/role/context applies,
4. whether action is required,
5. what receipt/evidence exists where applicable.

Action Cards remain the primary actionable primitive for members, defined by ADR-0027 and rendered per Member Shell v0. The Civic Shell does not define a parallel ActionCard schema or mutation surface.

Steward required-action surfaces are related but not ADR-0027 ActionCards. The wire-stable steward required-action shape remains forward-direction under `#1837`.

## Status and proof labels

The Civic Shell must not say "working," "done," or "live" without a proof level. Status labels must align with the proof-level taxonomy and capability matrix tracked in `#1796`.

Expected distinctions include:

- design,
- schema,
- unit-tested,
- integration-tested,
- local proof loop,
- K3s proof loop,
- devnet / multi-node proof,
- partner rehearsal,
- pilot production candidate,
- production hardened.

The public exterior must not imply production readiness. The logged-in interior must not imply ICN-native receipt emission where the underlying service is only hosted or governed.

## v0 scope

The Civic Shell v0 is only a composition spec:

- public status/development-updates concept,
- authenticated shell concept,
- civic-space composition model,
- conceptual forge/status/community routes,
- identity/context concepts,
- proof/truth-label discipline,
- no dangerous controls,
- no new write path,
- no live mutation surface.

## Follow-up issue suggestions

These are suggestions only. The PR introducing this doc does not open them.

- `spec(product): define ICN Civic Shell authenticated route map`
- `web(status): define public ICN status / development updates surface`
- `ux(shell): define member dashboard and notification model`
- `ux(shell): define Forge Room adapter over GitHub / Forgejo`
- `ux(shell): define Operations Control Room authority model`
- `docs(project-index): integrate Civic Shell surfaces with proof-level taxonomy`
- `ops(hosting): reconcile Civic Shell service list with service-hosting definitions`
- `ux(shell): define Communications Room boundary for Matrix, bridge, and announcements`
- `docs(spec): rename Civic Shell file and registry/index entries away from headquarters draft path`

## Review checklist

Reviewers should confirm:

- [ ] Does not reintroduce the headquarters/command-center metaphor.
- [ ] Does not turn GitHub into the center; GitHub remains current adapter where true.
- [ ] Does not make Matrix governance.
- [ ] Does not make OIDC/Keycloak authority.
- [ ] Does not make n8n privileged authority.
- [ ] Does not expose private data.
- [ ] Does not claim Forgejo canonical before cutover gates land.
- [ ] Does not claim ICN-EDGE deployed.
- [ ] Does not overclaim production readiness, live federation, or formal pilot status.
- [ ] Does not duplicate member-shell or steward-cockpit contracts.
- [ ] Keeps CLI as steward/operator backend, not member default.
- [ ] Preserves regulatory-safe vocabulary: settlement, position, obligation, allocation, receipt, provenance. Avoids payment, wallet, balance, currency, token, crypto, blockchain, and timebank as ICN-native framing except in explicit negation.

## Non-claims

- This spec does not implement the Civic Shell.
- This spec does not define a new endpoint.
- This spec does not implement authentication.
- This spec does not deploy Forgejo, Keycloak, Matrix, n8n, or any service.
- This spec does not mutate DNS, K3s, VLANs, firewall rules, or `network-ops`.
- This spec does not expose any public admin surface.
- This spec does not move, expose, preview, or cache private vault contents.
- This spec does not redefine Member Shell v0, Steward Cockpit v0, ActionCard, receipt envelope, accessibility baseline, website truth boundary, public maturity claims, service hosting, auth bridge, or sovereign forge strategy.
- This spec does not introduce a new receipt class.
- This spec does not close sibling issues.
- This spec does not claim production readiness, live federation, formal NYCN pilot, Phase 2 completion, or operation under this contract by any real institution today.
