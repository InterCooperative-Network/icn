---
Status: normative
Authority: spec / product-composition (defines the v0 composition contract that ties together the public website's truth boundary, `docs/spec/member-shell-v0.md` (#1830), `docs/spec/steward-cockpit-v0.md` (#1831), `docs/pilots/no-cli-organizer-member-rehearsal-workflow.md` (#1724), `docs/architecture/SERVICE_HOSTING_MODEL.md`, `docs/architecture/AUTH_BRIDGE_AND_DID_LOGIN.md`, `docs/architecture/PROTOCOL_SELECTION_FOR_MEMBER_SERVICES.md`, `docs/strategy/SOVEREIGN_FORGE.md`, and `docs/ops/SERVICE_GOVERNANCE_TEMPLATE.md` into a single top-level institutional shell concept; composition only — does not redefine any of those documents. The non-replacement rules and the explicit non-goals are normative; everything else is design direction at v0.)
Canonical: no
Last Reviewed: 2026-05-21
---

# ICN Headquarters v0

> **Status: spec, v0, design / composition only.** Defines "ICN Headquarters" as the top-level public-plus-logged-in institutional operating shell that composes existing ICN surfaces. No implementation. No new endpoint. No new auth system. No new deployment. No production-readiness claim. No live federation claim. No formal NYCN pilot claim. No Phase 2 completion claim. The PR introducing this doc opens a composition contract; it does not close any sibling issue and does not authorize any infrastructure change.

<!-- truth-class: normative -->

## Purpose

A cooperative substrate needs a single recognizable institutional space. Today, ICN surfaces are spread across the public website at `intercooperative.network`, the per-domain member shell (`docs/spec/member-shell-v0.md`), the per-domain steward cockpit (`docs/spec/steward-cockpit-v0.md`), the no-CLI organizer rehearsal path (`docs/pilots/no-cli-organizer-member-rehearsal-workflow.md`), the forge / repository / project record layer (`docs/strategy/SOVEREIGN_FORGE.md`, `docs/ops/FORGEJO_DEPLOYMENT_PLAN.md`), and an operations / service-hosting layer (`docs/architecture/SERVICE_HOSTING_MODEL.md`, `docs/ops/SERVICE_GOVERNANCE_TEMPLATE.md`).

**ICN Headquarters is the public and logged-in institutional operating space for ICN.** Publicly, it shows truth-bounded status, development updates, service health, the current phase and roadmap, and routes to docs / forge / community. Privately, it gives members, organizers, developers, stewards, and operators the context-aware rooms, records, action cards, notifications, and authority-bounded controls needed to run the institution.

This spec defines that composition. It does **not** redefine the member shell, the steward cockpit, the public website, the forge, the auth bridge, the service-hosting model, or any operational system. It is the contract a Headquarters-class surface must satisfy in order to call itself "ICN Headquarters."

**Headquarters composes existing ICN surfaces; it does not supersede the Member Shell, the Steward Cockpit, the public website, the service-hosting model, or the auth-bridge model.** Every contract those documents define remains in force; Headquarters routes into them, references them, and surfaces summaries of them — it never replaces them.

Physical metaphor for readers who think in rooms, not in routes:

```text
Public Window  →  Lobby  →  Member Desk
                            Governance Room
                            Workroom
                            Records Room
                            Forge Room
                            Operations Control Room
                            Communications Room
                            Vault / Privacy Posture
                            Settings / Identity
```

Headquarters is the building. Member Shell v0 is what a member sees once they are at their desk. Steward Cockpit v0 is what an operator sees inside the operations control room. Headquarters is the surface that ties the rooms together — and bans the panopticon room from being added later.

## Scope and non-goals

**In scope:**

- A composition contract that ties the public site, the per-domain member shell, the per-domain steward cockpit, the no-CLI organizer path, the forge room, the operations control room, the communications room, and the vault posture into one institutional shell concept.
- A room model that gives each major function a named, bounded space.
- Public / private boundary rules anchored to ADR-0032 (Website Truth Boundary) and ADR-0033 (Public Maturity Claims and Evidence Links).
- A domain / route doctrine for `intercooperative.network` (public truth) vs `icn.zone` (short operational / access / discovery) that does **not** turn the short domain into a second marketing site.
- An explicit non-replacement contract against `docs/spec/member-shell-v0.md` and `docs/spec/steward-cockpit-v0.md`.
- A v0 status / proof-level discipline that hooks into the proof-level taxonomy / capability status matrix tracked in `#1796`.
- Explicit non-goals (no new endpoints, no auth implementation, no DNS / K3s / VLAN mutation, no Forgejo deployment, no n8n workflow build, no Matrix launch claim, no production-readiness claim, no live-federation claim, no formal pilot claim).
- A follow-up issue suggestion list — **suggestions only**; no issues are created by this PR.

**Not in scope (preserved out of this PR):**

- Not a public-website rebuild, redesign, or new content set. Headquarters is **not** the public website; the website remains the truth boundary per ADR-0032 and Headquarters extends from it.
- Not a member-shell redefinition. The signing-confirmation contract, the offline / draft-intent labeling, the receipt rendering tiers, the accessibility gate, and the closed status vocabulary live in `docs/spec/member-shell-v0.md` and are unchanged.
- Not a steward-cockpit redefinition. The twelve operator surfaces, the fourteen operator scenarios, the member-impact summary mapping, and the operator-state vocabulary live in `docs/spec/steward-cockpit-v0.md` and are unchanged.
- Not a node-operator civic-role surface specification (`#1613`). That remains separate.
- Not a sovereign-forge canonical-cutover decision. `docs/strategy/SOVEREIGN_FORGE.md` stages the cutover; Headquarters at v0 does not advance the gates.
- Not a Forgejo deployment, a Keycloak / authentik / OIDC deployment, an n8n workflow build, a Matrix / bridge launch, a Uptime Kuma policy change, a `network-ops` mutation, or any DNS / K3s / VLAN / firewall change.
- Not a private-data ingestion path. Private partner data, named-partner fixtures, room IDs, credentials, tokens, secrets, and operational URLs stay out of the ICN repository.
- Not a NYCN-specific or named-partner framing. Institution packages localize labels in their own repositories; the generic Headquarters spec stays generic.
- Not a closure of `#1724`, `#1726`, `#1710`, `#1613`, `#1796`, `#1779`, `#1837`, or `#1873`. The PR uses `Refs:` only.
- Not a Phase 2 completion claim. `docs/PHASE_PROGRESS.md` currently lists Phase 2 as in progress, partner-bound, and not yet a formal pilot; nothing in this PR changes that.
- Not a production-readiness claim, a live-federation claim, or a formal pilot claim for any institution.

## Domain and route doctrine

> **Operational context, not ICN product doctrine.** The points below summarize safe architectural facts about how ICN domains and the segmented network are currently and prospectively used. `network-ops` was **not** read locally in this session; this section uses operator-provided context only. Nothing here mints ICN product policy on top of operational reality, and nothing here is offered as a public source of ICN truth.

- **`intercooperative.network`** is the canonical public identity / truth domain for ICN. It is the truth boundary defined by ADR-0032; Headquarters' public surface lives behind it.
- **`icn.zone`** is the short operational / app / access / QR / discovery domain. It is **not** a second marketing site. Its purpose is fast access to action surfaces; it must route, not republish. Headquarters MAY eventually host or route to authenticated action surfaces beneath `icn.zone` — conceptual examples include `/status`, `/forge`, `/dev`, `/docs`, `/join`, and `/dashboard`. **This spec does not claim those routes exist today.** They are conceptual until a separate PR proves any of them live.
- The segmented network underneath this — including the **`ICN-PRIVATE`** segment (currently realized as VLAN 30) carrying ICN private / control services, and the **`ICN-EDGE`** segment (currently scoped as VLAN 31) planned for the public edge — is **current/planned operational context**, not ICN product doctrine. Headquarters' admin / control-plane surfaces remain VPN- / Tailscale-only as long as the current operational reality holds; the public exterior does not depend on any particular VLAN number, and renumbering or replacing those segments does not change what Headquarters is.
- **`ICN-EDGE` is not the general live public edge today.** Headquarters does **not** claim deployment on it.
- **No public admin surfaces.** Admin and control-plane surfaces are private overlay; the public site never carries an admin login flow.

This domain doctrine is a **boundary**, not a deployment. No DNS record is created or changed by this spec.

## Public exterior

Unauthenticated visitors to Headquarters' public surface see the public website plus a small, truthful set of additional surfaces:

- **What is real now** — maturity-banded subsystem claims per ADR-0032. The closed band taxonomy (`strong` / `advancing` / `maturing` / `behind` / `not-yet`) is the contract; nothing on the public surface sits without a band.
- **Development updates** — phase summary derived from `docs/PHASE_PROGRESS.md` and `docs/STATE.md`. Updates name what landed, what is in progress, and what is deferred. Public update language is honesty over polish; no production-readiness implication.
- **Public roadmap and current phase** — the same banded roadmap that already lives at the website surface (`website/src/data/roadmap.json`). Headquarters does **not** invent a parallel roadmap.
- **Public service health / uptime surface** — a status page rendered against operational telemetry of the kind currently tracked in `network-ops` (e.g., `monitoring-model.yaml`). Renders **service health posture**, not internal node-state detail. Member-impact summary discipline (per Steward Cockpit v0 Design principle 9) applies: a public "healthy" claim must not contradict the institution's own member-shell reality.
- **Incidents and maintenance notices** — visible when they affect public surfaces. No private operational detail; no private incident bodies.
- **Public forge window** — a read-only window onto ICN's project-record surface (GitHub today, Forgejo when and if the canonical cutover gates land per `docs/strategy/SOVEREIGN_FORGE.md`). The window names what is canonical, what is mirror, and what is adapter.
- **Documentation and onboarding routes** — links into `docs/`, into "get involved" / "join" / community surfaces, and into the truth-boundary doc.

Each of these surfaces obeys ADR-0033 (Public Maturity Claims and Evidence Links): every banded claim carries evidence — ADR id, issue id, code path, or phase reference — so the band is mechanically checkable.

The public exterior is **not** a marketing surface dressed up as an institution. It is the institution's truth boundary, made navigable.

## Logged-in interior

An authenticated visitor to Headquarters sees a context-aware shell. Headquarters' v0 interior is **read-first** and **coordination-first**; mutation paths route into the existing per-domain shells (member shell, steward cockpit) rather than being reinvented here.

### Identity and context

- **Identity** — who the viewer is (a DID, a member of one or more domains; the auth bridge / OIDC session is projection state, not authority).
- **Active domain / organization** — which `InstitutionalDomain` (per `docs/spec/institutional-domain.md`) is currently in scope. Switcher when the viewer has standing in multiple.
- **Active role** — the structural role (Representation / Execution / Attestation per ADR-0014) the viewer is acting under in the active domain.
- **Authority scope** — the visible bound on what the viewer can do in the active domain. Lives in the member shell's standing surface and the steward cockpit's domain status surface; Headquarters renders a one-line summary plus a route into the authoritative surfaces.

### Composed surfaces

The interior is a **set of routes** into existing surfaces, not a re-implementation of them:

- **Member dashboard** — composed from the member shell's Home / Today + My Standing + Action Cards + Sync / Offline status surfaces (`docs/spec/member-shell-v0.md`). Headquarters never duplicates these; it routes.
- **Action cards** — the primary actionable primitive per ADR-0027, rendered by the member shell. Headquarters surfaces the **count** and **first item** as a context tile; full interaction happens in the member shell.
- **Notifications** — see §"Notifications and Action Cards" below.
- **Governance room** — proposals, decisions, mandates, challenge windows, the accepted-vs-applied distinction per `docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md` doctrine ("accepted is not applied"). Routes into the member shell's Decisions / Governance surface and (for stewards) into the cockpit's Governance / Process surface.
- **Workroom** — work items / action items with their authority, receipts, evidence, assignment, completion status. Renders the same plain-language receipt summaries the member shell renders; technical receipt detail lives behind a "details" affordance per the member shell contract.
- **Records / receipts room** — the viewer's receipts (member-shell Receipts surface) plus the institution's records (the cockpit's Receipt Store surface, for stewards) plus the artifact registry surface (`docs/spec/artifact-registry-and-scoped-vault.md`).
- **Forge room** — see §"Room model" below.
- **Operations control room** — for viewers with steward standing only. Routes into `docs/spec/steward-cockpit-v0.md`. Member visitors see "no steward standing in this domain" rather than the cockpit surface.
- **Communications room** — see §"Room model" below.
- **Vault / privacy posture** — private overlay posture (existence + scope + access path; never body content) per `docs/spec/artifact-registry-and-scoped-vault.md` and the steward cockpit Privacy Posture surface.
- **Settings / identity** — devices, key status, language / accessibility preferences, accommodation profile (in private overlay, never plain-text per ADR-0028 category 10).

## Relationship to Member Shell v0

`docs/spec/member-shell-v0.md` (merged #1830) is the **primary participation surface**. Headquarters MAY contain or route into the member shell, but **MUST NOT**:

- duplicate or redefine `/me/standing`,
- duplicate or redefine `/me/action-cards`,
- duplicate the ten-step signing / confirmation flow,
- duplicate receipt rendering tiers,
- duplicate offline / draft-intent / sent-waiting-for-receipt / confirmed labeling,
- duplicate the closed seven-string sync vocabulary, the seven-string execution-scope vocabulary, the action-lifecycle vocabulary, or the privacy / disclosure vocabulary,
- duplicate the twelve-category accessibility gate (per ADR-0028 + `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md`).

When a logged-in member acts, the action goes through the **member shell contract**, not a Headquarters-specific surface. Headquarters at v0 is composition; the member's act of participation routes into the existing v0-conformant shell.

## Relationship to Steward Cockpit v0

`docs/spec/steward-cockpit-v0.md` (merged #1831) is the **operator / steward complement**. Headquarters MAY contain or route into the cockpit as the **operations control room**, but **MUST NOT**:

- redefine the twelve cockpit surfaces,
- redefine the closed v0 operator-state vocabulary,
- redefine the fourteen operator scenarios,
- redefine the member-impact summary mapping (Design principle 9),
- collapse the cockpit's technical detail into Headquarters' plain-language room summaries.

No steward action surfaced in Headquarters becomes a "god-mode admin button." Every cockpit-side action that is visible from Headquarters must continue to show **authority basis**, **expected receipt / evidence class**, **member-impact summary**, and **reversibility / challenge posture where applicable** — the cockpit's contract is preserved, not relaxed.

A viewer without steward standing in the active domain MUST NOT see steward-only surfaces or actions from inside Headquarters. The cockpit's six boundary lines (vs member shell, vs node-operator civic-role surface, vs public website, vs institution-package skin, vs backend / runtime, vs surveillance / admin-control panel) are reaffirmed verbatim.

## Relationship to No-CLI Organizer / Member Workflow

`docs/pilots/no-cli-organizer-member-rehearsal-workflow.md` (`#1724`, `#1726`) is the guided, browser- and mobile-first workflow for organizers and members. Headquarters is the **long-term home** for that workflow; the CLI remains **steward / operator backend and verifier tooling**, not the organizer or member default surface.

Headquarters preserves the workflow's hard boundaries:

- **Three paths stay distinct.** Organizer, Steward / Operator, Future Member — distinct default surfaces, distinct CLI exposure, no path lift-and-shift.
- **Preview / review before mutation.** Every organizer mutation passes through a review / diff / counts surface before confirm.
- **Mutation last.** Browser / mobile-first surfaces are mutation-last; CLI may run earlier for verification but is not the default user journey.
- **Repo-safe evidence packets only.** No private partner data, no private overlay content, no credentials in any export.

This spec does **not** implement the workflow. The workflow lives where it lives.

## Relationship to Service Hosting Model

`docs/architecture/SERVICE_HOSTING_MODEL.md` names the **hosted → governed → ICN-native** progression for ICN-touched services. Headquarters surfaces governed services; it does not collapse the progression.

- A service that runs is a **hosted** service.
- A service whose admin access, backups, upgrades, route bindings, and meaningful transitions eventually need receipts / provenance is a **governed** service.
- A service that participates in ICN primitives (`ServiceIdentity`, capability-scoped actions, receipt emission, `ToolManifest` / `ToolBinding` lifecycle per RFC-0017) is an **ICN-native** service.

Headquarters at v0:

- **MUST** name each visible service with its stage in the progression.
- **MUST** treat service-local admin panels (a Forgejo admin UI, a Keycloak admin UI, a Grafana settings page, etc.) as **service-local**, not as institutional authority. Service-local admin is not Headquarters authority.
- **MUST NOT** claim a service is ICN-native until it actually emits ICN-native receipts per `docs/architecture/SERVICE_HOSTING_MODEL.md` stage 3.
- **MAY** reference the service-governance template (`docs/ops/SERVICE_GOVERNANCE_TEMPLATE.md`) so service additions follow the named questions before they become Headquarters dependencies.

Reconciliation of the Headquarters service list against the service-hosting definitions is a named follow-up, not a deliverable of this PR.

## Relationship to Auth Bridge and DID Login

`docs/architecture/AUTH_BRIDGE_AND_DID_LOGIN.md` establishes the rule:

```text
OIDC authenticates sessions.
ICN authorizes institutional power.
Receipts prove institutional transitions.
```

Headquarters reaffirms it. v0 specifics:

- **Keycloak / OIDC may be v0 session infrastructure.** A Keycloak / authentik / Ory / ZITADEL / Kanidm IdP, where one is in operational use, may carry the browser session. It does not carry authority.
- **Keycloak groups are projection state, not authority.** A Keycloak group named after an ICN role does not grant the ICN role; it reflects an external assignment. Authority lives in ICN standing.
- **OIDC group sync to ICN authority is forbidden as an authority direction.** The correct arrow is `DID → ICN authority → short-lived OIDC claims → service session`; the inverse is the panopticon shortcut and is rejected.
- **Future DID / passkey / local-device unlock path is the target.** Local key material is unlocked by a passkey, biometric, or PIN on the user's device; the device signs an action; the action produces a receipt. Biometrics / PINs are **never** identity themselves — they unlock local key material, full stop.

Headquarters at v0 does **not** implement any of the above. It states the rules so future implementation work cannot quietly invert them.

## Room model

Each room is a named, bounded space. Rooms compose existing surfaces; they do not introduce new ones.

### Public Window

- Public exterior content (see §"Public exterior").
- Updates, status, roadmap, public docs, "get involved," public forge window.
- ADR-0032 truth boundary applies. ADR-0033 evidence linking applies.
- No private operational detail. No private incident bodies. No member identities.

### Lobby

- Logged-in landing surface. "Who am I, what domain / organization am I in, what needs my attention."
- Routes into Member Desk by default; routes into Operations Control Room only if the viewer has steward standing in the active domain.
- Identity and context (see §"Identity and context").

### Member Desk

- Standing, action cards, votes, assignments, receipts — composed from `docs/spec/member-shell-v0.md` Home / Today + My Standing + Action Cards + Receipts surfaces.
- The signing / confirmation flow is the member shell's flow.
- The accessibility baseline is the member shell's twelve-category gate.

### Governance Room

- Proposals, decisions, mandates, challenge windows.
- Accepted-vs-applied distinction surfaced honestly. `accepted is not applied` is doctrine; the room shows decision state plus dispatch state plus application evidence per `docs/spec/effect-dispatch-contract.md`.
- ReconciliationStatus discipline (see `#1873`) applies: a "decision applied" claim requires execution evidence, not just acceptance.
- Member-side view routes into Member Shell Decisions / Governance; steward-side view routes into Cockpit Governance / Process.

### Workroom

- Work items / action items with authority basis, receipts, evidence, assignment, completion.
- Routes into the member shell Action Cards + Receipts surfaces and (for stewards) the cockpit Compute / Commons + Governance / Process surfaces.
- No "mark complete without receipt" affordance; the action lifecycle vocabulary from `docs/spec/member-shell-v0.md` is preserved.

### Records Room

- Documents, policies, receipts, evidence packets, signed objects.
- Member view shows the member's own receipts (`docs/spec/member-shell-v0.md` Receipts surface). Steward view adds the institution's records (`docs/spec/steward-cockpit-v0.md` Receipt Store, Storage / Artifacts / ScopedVault surfaces).
- Access path + challenge path always visible where the receipt class supports them.
- Private artifact bodies never reach the rendering layer per `docs/spec/artifact-registry-and-scoped-vault.md`.

### Forge Room

- Repositories, issues, pull / merge requests, reviews, CI, releases, RFCs, ADRs, maintainership authority.
- **GitHub is currently external adapter where true.** ICN's primary public mirror and contributor discovery surface lives at GitHub today; the website's truth-boundary doctrine treats GitHub as adapter / mirror, not as homeland (per `docs/strategy/SOVEREIGN_FORGE.md`).
- **Forgejo / forge is future durable project / work record target** once the cutover gates in `docs/strategy/SOVEREIGN_FORGE.md` and the deployment plan in `docs/ops/FORGEJO_DEPLOYMENT_PLAN.md` are met. Forgejo is **not canonical yet**.
- The Forge Room at v0 surfaces both: a GitHub adapter view (current operational reality) and a Forgejo target (not deployed by this spec).
- Maintainership authority, release-signing authority, and review authority are governed surfaces. The forge admin UI is service-local, not Headquarters authority.

### Operations Control Room

- Steward-only. Composes `docs/spec/steward-cockpit-v0.md` verbatim where it applies: service health, node status, incidents, backups, restore drills, deployments, monitors, operator-required actions.
- Uses the cockpit's closed v0 operator-state vocabulary.
- Member-impact summary discipline (Design principle 9 in the cockpit) applies to every degraded / repair / review row.
- **No public admin endpoints.** The operations control room is reachable only over VPN / Tailscale as long as the current operational reality holds; no admin surface is published.
- The operations control room is a consumer of operational telemetry of the kind currently tracked in `network-ops` (e.g., `monitoring-model.yaml`); it does **not** publish that telemetry source-of-truth, and the telemetry source is not a source of public ICN truth.

### Communications Room

- Matrix / community chat, announcements, discussion pointers, onboarding paths.
- **Matrix is real-time coordination, not governance authority.** A Matrix room cannot ratify a proposal, mutate a mandate, or close an action item; that authority lives in the Governance Room.
- **Discord-to-Matrix bridge is planning / scaffold only** unless a separate PR proves it deployed and governed. Headquarters at v0 does not claim a bridge launch.
- **No private room IDs, room names, credentials, tokens, or operational URLs are copied into this spec or any Headquarters surface in the repo.** Per `network-ops` operational discipline.

### Vault / Privacy Posture

- Private overlay posture (loaded / missing / degraded per `#1767` forward direction; existence + scope + access path per `docs/spec/artifact-registry-and-scoped-vault.md`).
- Access receipts, export receipts, redaction posture without revealing content.
- **Posture, not content.** No private object body bytes reach the rendering layer in any Headquarters room.

## Notifications and Action Cards

Notifications in Headquarters are not social-media noise. Every notification carries:

1. **What happened.** A plain-language description of the institutional event.
2. **Why the viewer is notified.** The mandate, standing, or scope tie that made this event reach this viewer.
3. **Which context / role.** The active domain, the role under which the notification applies, and the scope chain.
4. **Whether action is required.** Required-action notifications carry through to the appropriate Action Card surface; informational notifications do not pretend to require action.
5. **What receipt / evidence exists.** A pointer to the receipt or evidence packet, where one exists, with the formal record under "details" per the member shell receipt contract.

Action Cards remain the **primary actionable primitive** for members, defined by ADR-0027 and rendered per `docs/spec/member-shell-v0.md`. Headquarters does **not** redefine the ActionCard schema and does **not** invent a parallel mutation surface.

Steward-required-action surfaces are related but **not** ADR-0027 ActionCards — that schema does not cover operator scenarios (see `docs/spec/steward-cockpit-v0.md` §"Required Actions / Steward Action Cards"). The cockpit names fourteen operator scenarios as rendering analogs; the wire-stable shape is forward-direction (`#1837`, "steward required-action card contract"). Headquarters reflects whatever shape that follow-up lands; it does not pre-decide it.

## Status and proof labels

Headquarters surfaces status honestly. The proof-level taxonomy / capability status matrix tracked in `#1796` is the authority; Headquarters' status renderings must use the same vocabulary.

The expected proof-level distinctions Headquarters surfaces include (the exact list lives in `#1796`):

- **design** — the surface exists as a spec only.
- **schema** — the wire-stable shape is defined (ADR / spec / contract file) but not implemented.
- **unit-tested** — implementation has unit coverage.
- **integration-tested** — implementation has integration / fixture / multi-component coverage.
- **local proof loop** — the surface exercises a local proof loop (cargo test passes against fixture state).
- **K3s proof loop** — the surface exercises a proof loop on the K3s cluster.
- **devnet / multi-node proof** — the surface exercises a proof loop across multiple devnet nodes.
- **partner rehearsal** — the surface has been exercised with a partner in rehearsal mode (no production claim).
- **pilot production candidate** — the surface is a candidate for a formal pilot.
- **production hardened** — the surface has passed pilot hardening for production operation.

Headquarters MUST NOT use generic "working" or "done" language without a proof level. The public surface MUST NOT imply production readiness; the logged-in surface MUST NOT imply ICN-native receipt emission where the underlying service is still a hosted or governed (not ICN-native) service.

The reconciliation between Headquarters' status labels and `#1796`'s matrix is a named follow-up; the matrix is authoritative when it lands.

## v0 scope

Read-first and coordination-first:

- **Public status / development updates concept** — defined here; rendering deferred.
- **Authenticated shell concept** — defined here; rendering deferred.
- **Dashboard composition map** — the room model above.
- **Forge / status / community routes** — conceptual routes; not deployed.
- **Identity / context concepts** — defined here; the auth bridge implementation is forward-direction.
- **Proof / truth labels** — anchored to `#1796`; final taxonomy lives in that issue.
- **No dangerous controls.** No mutation surfaces beyond what already exists in the member shell and the steward cockpit.
- **No live mutations.** Headquarters at v0 does not introduce a new write path. Mutation routes into existing v0-conformant surfaces.

## Non-goals (explicit)

The following are explicitly out of scope for this PR:

- **No app implementation.** No code lands here.
- **No new endpoint.** Headquarters does not extend the gateway, the SDKs, or any service API.
- **No auth implementation.** No Keycloak / authentik / Ory / ZITADEL / Kanidm deployment, configuration change, group sync, or schema change.
- **No Keycloak deployment.**
- **No Forgejo deployment.**
- **No DNS / K3s / VLAN / network mutation.** `network-ops` is read-only operational context for this PR.
- **No n8n workflow build.** n8n remains private workflow glue for access requests, review queues, notifications, reminders, and onboarding checklists; it is not privileged authority and Headquarters does not deploy or expose it.
- **No Matrix launch claim.** Matrix is real-time coordination only; bridge planning / scaffold state remains as it is.
- **No public admin surfaces.**
- **No private data in repo.** No private room IDs, no credentials, no tokens, no private URLs, no named-partner fixtures, no private member identities.
- **No Phase 2 completion claim.**
- **No formal NYCN pilot claim.**
- **No live federation claim.**
- **No production-readiness claim.**
- **No replacement of existing member-shell or steward-cockpit specs.**

## Follow-up issue suggestions

The following issues are **suggestions**, not action items. The PR introducing this doc does **not** open them; opening them is a separate, user-driven decision.

- `spec(product): define ICN Headquarters authenticated shell route map` — the route surface for the logged-in interior, anchored to the existing per-domain surfaces.
- `web(status): define public ICN status / development updates surface` — the public-exterior status / updates content set, anchored to ADR-0032 / ADR-0033.
- `ux(hq): define member dashboard and notification model` — the Headquarters notification contract, building on §"Notifications and Action Cards" above.
- `ux(hq): define forge room adapter over GitHub / Forgejo` — the GitHub-adapter / Forgejo-target window surface.
- `ux(hq): define operations control room authority model` — the steward-only routing rule and the cockpit-composition contract.
- `docs(project-index): integrate Headquarters surfaces with proof-level taxonomy` — Headquarters status labels reconciled with `#1796`.
- `ops(hosting): reconcile Headquarters service list with service-hosting definitions` — Headquarters service list audited against `docs/architecture/SERVICE_HOSTING_MODEL.md` stages.
- `ux(hq): define communications room boundary for Matrix, bridge, and announcements` — the Matrix / bridge / announcement surface rules.

## Review checklist

Reviewers should confirm:

- [ ] Does not turn GitHub into the center. GitHub is named as current external adapter; canonical project record remains a target, not a claim.
- [ ] Does not make Matrix governance. Matrix is real-time coordination, not binding authority.
- [ ] Does not make Keycloak / OIDC authority. OIDC authenticates sessions; ICN authorizes power; receipts prove transitions.
- [ ] Does not make n8n privileged authority. n8n is private workflow glue, not authority.
- [ ] Does not expose private data. No private room IDs, credentials, tokens, URLs, member identities, partner fixtures.
- [ ] Does not claim Forge canonical before the cutover gates in `docs/strategy/SOVEREIGN_FORGE.md` are met.
- [ ] Does not claim VLAN 31 / ICN edge deployed.
- [ ] Does not overclaim production readiness, live federation, or formal pilot status.
- [ ] Does not duplicate member-shell or steward-cockpit contracts.
- [ ] Keeps CLI as steward / operator backend, not member default.
- [ ] Preserves regulatory-safe vocabulary (settlement / position / obligation / allocation / receipt / provenance — never payment / wallet / balance / currency / token / crypto / blockchain / timebank as ICN-native framing).

## Relationship to sibling work

| Concern | Where it lives |
|---|---|
| Headquarters composition (this spec) | (new) |
| Member shell v0 | `#1818` (merged #1830) |
| Steward cockpit v0 | `#1795` (merged #1831) |
| No-CLI organizer / member workflow | `#1724`, `#1726` (`docs/pilots/no-cli-organizer-member-rehearsal-workflow.md`) |
| Node operator civic-role surface in Commons Shell | `#1613` |
| Forgejo / auth bridge MVP gate | `#1710` |
| Proof-level taxonomy / capability status matrix | `#1796` |
| Public / demo truth sync | `#1779` |
| Steward required-action card contract | `#1837` |
| Accepted-is-not-applied ReconciliationStatus tests | `#1873` |
| Website truth boundary | ADR-0032 |
| Public maturity claims and evidence links | ADR-0033 |
| Service hosting model | `docs/architecture/SERVICE_HOSTING_MODEL.md` |
| Auth bridge and DID login | `docs/architecture/AUTH_BRIDGE_AND_DID_LOGIN.md` |
| Protocol selection for member services | `docs/architecture/PROTOCOL_SELECTION_FOR_MEMBER_SERVICES.md` |
| Sovereign forge strategy | `docs/strategy/SOVEREIGN_FORGE.md` |
| Forgejo deployment plan | `docs/ops/FORGEJO_DEPLOYMENT_PLAN.md` |
| Service governance template | `docs/ops/SERVICE_GOVERNANCE_TEMPLATE.md` |
| Effect dispatch contract (accepted-vs-applied chain) | `docs/spec/effect-dispatch-contract.md` (merged via #1819) |
| Institutional domain | `docs/spec/institutional-domain.md` (merged via #1820) |
| ArtifactRegistry / ScopedVault | `docs/spec/artifact-registry-and-scoped-vault.md` (merged via #1824) |
| Abuse-case hardening strategy (accepted-is-not-applied doctrine) | `docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md` |
| Organizer / member accessibility gate | `docs/design/ORGANIZER_MEMBER_ACCESSIBILITY_GATE.md` |
| Kernel / app separation | `docs/architecture/KERNEL_APP_SEPARATION.md` |
| AuthorityClass / TypedScope / Mandate | ADR-0014 |
| Authority grant minting seam | ADR-0019 |
| Bootstrap activation + standing read model | ADR-0020 |
| Institutional Effect Record canonical schema | ADR-0025 |
| Receipt and provenance proof envelope | ADR-0026 |
| ActionCard contract | ADR-0027 |
| Accessibility baseline for member interfaces | ADR-0028 |
| Compute workload manifest and authority boundary | ADR-0030 |
| Commons compute admission and settlement | ADR-0031 |

## Operational context (read-only)

> **`network-ops` was not read locally in this session.** This section uses safe operator-provided context only. `network-ops` is **not** a source of public ICN truth, and this section is **not** a re-export of it. No private URLs, IP addresses, room IDs, room names, hostnames, credentials, tokens, secrets, partner data, or named-member identities are reproduced here. The points below are architectural facts about how ICN sits inside its current homelab segmentation; they exist to keep Headquarters' rooms from contradicting current operational reality, not to make Headquarters a re-render of `network-ops`.

- `intercooperative.network` is the canonical public identity / truth domain for ICN.
- `icn.zone` is the short operational / app / access / QR / discovery domain; it is not a second marketing site. Routes beneath it that Headquarters references are conceptual unless and until a separate PR proves them live.
- ICN private / control services run on the **`ICN-PRIVATE`** segment (currently realized as VLAN 30) — current operational context, not ICN product doctrine.
- The **`ICN-EDGE`** segment (currently scoped as VLAN 31) is planned, not the general live public edge today — current/planned operational context, not ICN product doctrine.
- Matrix is real-time coordination, not governance authority.
- Forge is the durable project / work record target; Forgejo is not canonical yet.
- GitHub is currently temporary external hosting / mirror / adapter state where true.
- Keycloak / OIDC, where in use, is a temporary human directory / session projection — not ICN authority.
- n8n is private workflow glue (access requests, review queues, notifications, reminders, onboarding checklists), not privileged authority.
- Uptime Kuma / operational telemetry of the kind tracked in `monitoring-model.yaml` (in `network-ops`) is the current operational status source for service-health thinking; Headquarters consumes posture summaries from such telemetry, never the raw telemetry source-of-truth.
- Admin / control-plane services remain VPN / Tailscale only.
- No public admin surfaces.

Cross-repo dependency status: `network-ops` was **not** edited and **not** read locally during this session; this PR depends on it only as background operational context supplied by the operator.

## Non-claims (repeat block for grep clarity)

- This spec does not implement Headquarters. No code lands here.
- This spec does not define a new endpoint. It composes existing surfaces; it does not extend the gateway, the SDKs, or any service API.
- This spec does not implement authentication. No Keycloak / authentik / Ory / ZITADEL / Kanidm deployment, configuration change, group-to-authority sync, or schema change is authorized here.
- This spec does not deploy Forgejo, mutate DNS, mutate K3s, mutate VLAN configuration, mutate firewall rules, or touch any `network-ops` file.
- This spec does not build any n8n workflow.
- This spec does not launch Matrix, deploy a Matrix-Discord bridge, or claim a Matrix-based governance surface.
- This spec does not expose any public admin surface.
- This spec does not redefine `docs/spec/member-shell-v0.md`, `docs/spec/steward-cockpit-v0.md`, `docs/pilots/no-cli-organizer-member-rehearsal-workflow.md`, `docs/architecture/SERVICE_HOSTING_MODEL.md`, `docs/architecture/AUTH_BRIDGE_AND_DID_LOGIN.md`, `docs/architecture/PROTOCOL_SELECTION_FOR_MEMBER_SERVICES.md`, `docs/strategy/SOVEREIGN_FORGE.md`, `docs/ops/FORGEJO_DEPLOYMENT_PLAN.md`, or `docs/ops/SERVICE_GOVERNANCE_TEMPLATE.md`.
- This spec does not redefine ADR-0027 ActionCard, ADR-0026 receipt envelope, ADR-0028 accessibility baseline, ADR-0032 website truth boundary, ADR-0033 public maturity claims, or any merged sibling spec.
- This spec does not introduce a new receipt class.
- This spec does not preempt `#1613` (node operator civic-role surface in Commons Shell), `#1796` (proof-level taxonomy), `#1837` (steward required-action card contract), `#1873` (accepted-is-not-applied tests), `#1779` (public / demo truth sync), `#1710` (Forgejo / auth bridge MVP gate), `#1724` / `#1726` (no-CLI organizer / member workflow).
- This spec does not move, expose, preview, or cache private vault contents. Body bytes of `PrivateEvidence` artifacts never reach the rendering layer.
- This spec does not use wallet, payment, balance, currency, token, crypto, blockchain, or timebank framing for ICN-native participation. All such terms appear here only in explicit negation context (the non-goals block, the review checklist, this non-claims block, the regulatory-safe-vocabulary reminder).
- This spec does not authorize any change to the runtime, gateway, SDK, website, deploy scripts, K3s, DNS, Forgejo, Keycloak, n8n, Matrix, Uptime Kuma, or any deployed infrastructure.
- This spec does not claim Forgejo canonical, Keycloak canonical, GitHub canonical, or any specific service ICN-native at any specific stage.
- This spec does not claim VLAN 31 / `ICN-EDGE` deployed.
- This spec does not claim production readiness, a live partner federation, a formal NYCN pilot, Phase 2 completion, or operation under this contract by any real institution today.
- This spec does not close any sibling issue. The PR introducing this doc uses `Refs:` only; closure is a separate, user-driven decision against each issue's acceptance criteria.
