---
Status: normative
Authority: architecture (authoritative on substrate/institution placement)
Canonical: no
Last Reviewed: 2026-04-17
Supersedes on routing conflicts: `docs/strategy/NYCN-Repo-Architecture-Spec.md`, `docs/strategy/NYCN-Implementation-Matrix.md`
---

# Institution Package Boundary

> Defines what belongs in ICN platform versus what belongs in an institution-specific package (e.g., NYCN). Anchors all future layered implementation decisions. **Where this doc and any NYCN strategy doc disagree about where a type goes, this doc wins.**

**Grounded against:** `main` at `fde1466a` (Program + Milestone primitives landed, PR #1548). Subsequent Tranche 1a/1b work — milestone CCL gates and `PATCH /gov/programs/{id}/status` — is in progress on open PRs and is *not* yet landed on main at the time of this doc's last review.
**Companion docs:** `KERNEL_APP_SEPARATION.md`, `../strategy/ADR-001-What-ICN-Is.md`.
**Docs governed by this one (must not contradict):** `docs/strategy/NYCN-Repo-Architecture-Spec.md`, `docs/strategy/NYCN-Implementation-Matrix.md`, `docs/strategy/NYCN-Execution-Tranches.md`.

---

## Practical rule (keep in front of every decision)

> **ICN should know what a proposal, milestone, action item, meeting, scope, allocation, and receipt are. ICN should not know what a sponsor packet, session catalog, conference track, or summit registration flow is.**

If you find yourself about to add a noun from a specific institution's workflow — sponsor tiers, speaker bios, conference sessions, registration forms, summit blocks, accessibility questionnaires, donor levels, volunteer shift patterns — **stop**. That concept does not belong in `icn-governance`, `icn-ledger`, or any other substrate crate. It belongs in an institution package (see §F).

---

## A. The Two-Layer Model

```
┌──────────────────────────────────────────────────────────┐
│  Institution Package (e.g., nycn-icn)                    │
│  Charter YAML • CCL rules • NYCN-specific templates      │
│  Seed data • External integrations • Custom views        │
├──────────────────────────────────────────────────────────┤
│  ICN Platform (this repo)                                │
│  Generic institutional objects + HTTP + sled stores      │
│  Constraint enforcement • Event bus • Capability tokens  │
├──────────────────────────────────────────────────────────┤
│  ICN Kernel (kernel crates, meaning firewall applies)    │
│  Identity • Auth • State • Compute • Comms               │
│  Time • Coordination • Naming                            │
└──────────────────────────────────────────────────────────┘
```

The boundary between the top two layers is what this document defines.

For the current in-monorepo NYCN package home, see `institutions/nycn/`. That directory should be treated as if it were already an external institution repo.

---

## B. ICN vs Institution Package

### Goes in ICN (`icn-governance` + `apps/governance`)

A type, object, or capability belongs in ICN if:
- **A second cooperative would need it with different content but the same shape.** Committee, Meeting, Program, Milestone, ActionItem — every recurring institution has these.
- It has no institution-specific vocabulary baked into enum variants or field names.
- It uses `Custom(String)` escape hatches for institution-specific extensions rather than named variants.
- Its semantics are expressible as: entities owning structures owning activities with attached operational objects.

**Currently in ICN (verified on main):**
| Object | Location | Notes |
|--------|----------|-------|
| Entity (Federation/Cooperative/Individual) | `icn-governance::entity` | Sovereign, holds treasury |
| Structure (Committee/WorkingGroup/Team/Office) | `icn-governance::structure` | Non-sovereign, entity-owned |
| Activity (Event/Program/Project/Initiative) | `icn-governance::activity` | Time-bounded, entity-owned |
| Program + Milestone | `icn-governance::program` | Cycle container with stage gates |
| ActionItem + decision bridge | `icn-governance::action_item` | Provenance-linked to proposals |
| RoleAssignment | `icn-governance::structure` | Authority as `Vec<String>` capability strings (shipped surface). Typed replacement path — `AuthorityClass` / `AuthorityGrant` / `TypedScope` / `Mandate` — is frozen in [ADR-0014](../adr/ADR-0014-constitutional-object-model.md); migration is additive and not yet landed. |
| Meeting | `icn-governance::meeting` | Landed; HTTP routes wired |
| InstitutionalParent attachment | `icn-governance::parent` | Polymorphic object attachment |
| `/gov/me/scopes` + `/gov/me/work` | `apps/governance` handlers | PR #1552; assignee-indexed work spine |
| `Activity.parent_program_id` | `icn-governance::activity` | PR #1553; cycle-over-cycle linkage |

### Goes in the Institution Package (e.g., `nycn-icn` repo)

- **Charter configuration**: the YAML/CCL document that instantiates ICN objects with NYCN-specific content (entity names, committee membership, role definitions, voting thresholds).
- **Institution-specific CCL rules**: quorum rules, term limits, budget approval chains, summit stage-gate predicates with NYCN vocabulary.
- **Seed data**: initial members, structure definitions, program templates for `summit-2026`.
- **Custom views and dashboards**: program dashboard shaped for summit operations; sponsor pipeline view with NYCN tier names.
- **External integrations**: Google Workspace sync, ny-coop-net membership import, legacy spreadsheet migration.
- **NYCN-specific event handlers**: React to `ProgramMilestoneCompleted` to trigger NYCN-specific follow-on workflows.

### The Test

> If you find yourself naming an enum variant `AnnualSummit`, `PlatinumSponsor`, `VenueLocked`, or `NYCNOrganizer` in `icn-governance`, stop. That belongs in the institution package as CCL data or configuration — not as a core type variant.

No NYCN-specific vocabulary in ICN enums. Use `ProgramKind::Custom("annual-summit")`, `SponsorTier::Custom("platinum")`. The institution package owns the semantics; ICN owns the shape.

---

## C. CCL vs Host Runtime

### CCL expresses:
- **Authorization conditions**: who can call this action given current state
- **State transition guards**: pre-conditions that must hold before a transition fires
- **Threshold rules**: quorum, consent threshold, majority percentage
- **Completion criteria assertions**: milestone predicates (`venue_confirmed AND budget_locked`)
- **Delegation bounds**: what scopes can be subdelegated and to whom
- **Obligation creation**: what action items materialize when a proposal passes
- **Expiry and renewal**: lease durations, term limits, review cycles

### Host runtime (Rust, `apps/governance`) handles:
- **Storage and indexing**: Sled stores, multi-key indexes, prefix scans
- **Transport and serialization**: HTTP handlers, JSON models, gateway events
- **Generic object lifecycle**: CRUD, status transitions, soft delete
- **Receipt and provenance**: `ArtifactReceipt`, `completed_by`, `created_by`, timestamps
- **Low-level execution machinery**: actor mailboxes, sled transactions, tokio tasks
- **Capability token issuance and verification**: bearer tokens, DID-TLS binding
- **Rendering and API surface**: OpenAPI, TypeScript SDK types, mobile gateway endpoints

### The Rule

CCL describes *what is permitted and under what conditions*. The host runtime describes *how permitted actions are actually executed and stored*. Never put storage keys in CCL. Never put semantic authorization logic in a Rust match arm.

---

## C2. Entity Primitives vs Local Purpose Metadata (the type/purpose firewall)

**The same rule that separates kernel from app applies to entity types.**

ICN has exactly **four entity primitives**. They are sovereign types and they imply different machinery:

| Primitive | What it is | What ICN guarantees |
|-----------|------------|---------------------|
| `Individual` | A DID-backed person. | Identity, authentication, self-scoped reads. |
| `Cooperative` | An operating/economic body. | `governance_domain_id` + `treasury_account` (per `CooperativeProfile`). |
| `Community` | A civic/member/participant body. | `governance_domain_id` (per `CommunityProfile`). Treasury optional. |
| `Federation` | An association of member entities. | `governance_domain_id` + `member_entities` + optional `parent_id` (per `FederationProfile`). |

These are the only legal values of `BootstrapEntityType` in `icn-governance::bootstrap` and the only legal values of the `entity_type` field on the gateway's `/v1/entities` API. The serde enum is `#[serde(rename_all = "snake_case")]` and any other variant fails to deserialize at YAML parse time. **This is the firewall**: an institution package cannot smuggle a new entity type past ICN.

**Committees and working groups are NOT entities.** They are governance `Structure` records owned by a parent entity. They carry delegated authority via `RoleAssignment.authority_scope`, not sovereignty. They cannot hold treasury, cannot join federations, cannot appear in `member_entities`.

**Events / programs / cycles (e.g. a Summit) are NOT entities.** They are `Activity` and `Program` records owned by a parent entity.

### Local institutional role goes in `operating_purpose`, not in a new type

Institution packages frequently want to mark *which kind* of cooperative or community a record represents in their local taxonomy: an "organizing cooperative", a "program participant community", a "federation member community". These are valid local distinctions — but they are **purpose metadata**, not entity primitives.

`BootstrapEntityRecord` already carries:

```rust
pub operating_purpose: Option<String>,
```

That field is the only sanctioned home for this metadata. It is opaque to ICN — the runtime stores it (or, in the current importer, plans for it; see "Known gap" below) and never branches on its value.

### Correct vs incorrect mapping

```yaml
# ❌ WRONG — invents new entity primitives, will fail to parse
entity_type: organizing_cooperative
entity_type: program_community
entity_type: committee
entity_type: federation_member_community

# ✅ RIGHT — uses ICN primitives, locales role via operating_purpose
entity_type: cooperative
operating_purpose: organizing_cooperative

entity_type: community
operating_purpose: program_participant_community
# (with a separate Activity record for the related program)

# Committee — not an entity at all:
# Goes in a `kind: structure-seed` manifest:
kind: structure-seed
parent_entity_id: nycn-organizing
structures:
  - id: committee-finance
    kind: committee
    name: Finance Committee
```

### Worked example: NYCN reference federation

```text
nycn                        entity_type: federation
                            operating_purpose: regional_cooperative_network

  nycn-organizing           entity_type: cooperative
                            operating_purpose: organizing_cooperative
                            (carries treasury_account)

    committee-steering      structure-seed { kind: committee,  parent_entity_id: nycn-organizing }
    committee-finance       structure-seed { kind: committee,  parent_entity_id: nycn-organizing }
    committee-program       structure-seed { kind: committee,  parent_entity_id: nycn-organizing }
    wg-accessibility        structure-seed { kind: working_group, parent_entity_id: nycn-organizing }

    summit-2026             activity-program-seed { parent_entity_id: nycn-organizing }

  nycn-community            entity_type: community
                            operating_purpose: network_member_community

  summit-2026-community     entity_type: community            (only if it actually governs/confers standing)
                            operating_purpose: program_participant_community
                            related_activity: summit-2026
                            (otherwise: model as activity participant cohort,
                             not as an entity at all)
```

### Why this is the firewall, not a style guide

The primitive layer is the machine. `CooperativeProfile` requiring `treasury_account` means ICN gives every cooperative a treasury account; `FederationProfile` requiring `member_entities` means ICN runs federation joins/leaves against that vector. If an institution were allowed to ship `entity_type: organizing_cooperative`, the runtime would either have to (a) treat it as `cooperative` (in which case the new type adds nothing) or (b) branch on it (in which case ICN now contains NYCN-specific code). Both options break the kernel/app separation.

`operating_purpose: <string>` lets the institution carry as much local meaning as it wants without the runtime understanding any of it. UI can read it and render labels. CCL contracts can compare it. ICN never branches on it.

### Known gap (tracked, not blocking model adoption)

The current `BootstrapOperation::CreateEntity` payload sent to the gateway's `/v1/entities` API does **not** forward `operating_purpose` at apply time. The seed schema accepts it, the validator/planner sees it, but the apply step drops it. Institution packages can author the corrected model today; the metadata round-trips through `validate` and `plan` outputs, but does not yet land on the live entity record. Plumbing this through is a small follow-up.

---

## C3. Entity-scope vocabulary (scope vs organism)

> **Core vocabulary names the scope. Institution packages name the organism.**

ICN core names **structural scopes**, not one possible inhabitant of a scope. The local institutional layer between member/cell scope and federation/commons scope is a **domain** — an `InstitutionalDomain` per `docs/spec/institutional-domain.md`. The owning entity class of that domain may be a `Cooperative`, a `Community`, a `Federation`, an `Individual`, or another governed entity class permitted by policy. Naming a generic scope after one of its possible inhabitants privileges that inhabitant and forces every other entity class to be a second-class citizen of vocabulary the runtime itself defined.

### The rule

- **Generic scope language uses:** `InstitutionalDomain`, `Domain`, `LocalDomain`, `DomainPolicy`, and "owning entity class."
- **Generic scope language does NOT use:** `Coop` or `Cooperative` as a stand-in for "the local institutional layer." A local domain may be owned by a cooperative, but it may also be owned by a community, a federation, or another governed class. The scope name must not pre-decide that.
- **`Cooperative` is reserved** for the cases that are actually about cooperatives: the cooperative entity class itself, cooperative-specific setup paths, cooperative-economy framing, the cooperative-movement context, and institution packages serving cooperative federations.
- **Institution packages may use local nouns** freely. NYCN may speak of "the cooperative." Another package may speak of "the assembly," "the council," "the care body." That is what packages are for.

### What this means in practice

| Surface | Generic ICN vocabulary | Institution-package vocabulary |
|---|---|---|
| Architecture spec sections about scope hierarchy | `LocalDomain` / `InstitutionalDomain` / "local-domain-scoped" | "cooperative" / "community" / package-specific role names |
| Compute placement classes | `LocalDomainBound` (preferred); `Coop`-prefixed forms in legacy ADRs are read as `LocalDomain` | Per-package role names |
| Storage locality | Generic spec describes the scope level; existing `DataLocality::CoopReplicated` is a Rust identifier preserved for compatibility (see "Known drift" below) | — |
| Member-shell language | "processed inside your institution" or "handled inside your organization" | Per-package phrasing |
| Owning entity classes | `Individual` / `Cooperative` / `Community` / `Federation` (per `docs/spec/institutional-domain.md` §"InstitutionalDomain (object outline)") | Local synonyms for each class |

### Why this matters

The primitive layer is the machine. If ICN core ships a generic primitive called `CoopBound` or a scope tag `Coop(coop_id)`, every consumer of that vocabulary either (a) treats community-owned and federation-owned domains as "kind of like a coop" and conflates them, or (b) special-cases them with branches the runtime should not need to know about. Either way the kernel/app separation softens: institution-specific framing leaks into generic primitive names. The fix is the same as for §C2 — name the structural concept (domain, scope, policy reference, owning entity class) and let packages carry the local noun.

### Correct vs incorrect naming

```text
# ❌ WRONG — generic scope named after one inhabitant
CoopBound
coop-scoped execution policy
Coop(coop_id)
"sent for cooperative processing"  (member shell)
"cooperative-bound executor"        (operator dashboard)

# ✅ RIGHT — generic scope named for what it actually is
LocalDomainBound
local-domain-scoped execution policy
LocalDomain(domain_id)
"processed inside your institution"  (member shell)
"local-domain-bound executor"        (operator dashboard)
```

### Known drift (tracked, not blocking this section)

Existing canon already carries the older `Coop`-flavored naming in several load-bearing places:

- `icn/crates/icn-kernel-api/src/storage.rs` — `DataLocality::CoopReplicated` is a Rust enum variant, the variant is serialized as `coop_replicated`, and ordering / numeric assignments (`= 1`) are stable. Renaming is forward work; it requires a compatibility-aware migration with serde aliases and call-site updates. Tracked as a separate refactor follow-up; not done in this doc-only PR.
- `docs/adr/ADR-0030-compute-workload-manifest-and-authority-boundary.md` — uses `Scope: Commons | Coop(coop_id) | Federation(fed_id)` as the scope tag. The ADR is accepted; its decision is preserved as-is. A short naming note appended to the ADR reads the `Coop(...)` slot as `LocalDomain(...)` for current architecture.
- `docs/adr/ADR-0031-commons-compute-admission-and-settlement-policy.md` — uses `Coop-scoped` prose for the local-domain admission/settlement case. Same disposition: preserved with a short naming note.
- Issue `#1801` (compute placement) — uses `CoopBound`, `coop-scoped execution policy`, "cooperative-bound executor," and member-shell language "sent for cooperative processing." The forthcoming `#1801` spec PR is the right place to land the corrected vocabulary; the corrected names (`LocalDomainBound`, "local-domain-scoped," "processed inside your institution") are recorded in the boundary handoff for that PR to consume.

### The test

When you propose a generic primitive or scope name, ask:

1. Does this name describe a **scope, jurisdiction, policy layer, or boundary** — or does it describe a **specific kind of institution** that lives inside that scope?
2. If a community-owned or federation-owned domain shipped tomorrow under this primitive, would the name still read accurately, or would it imply that domains are "really" cooperatives?
3. Could an institution package add a local noun that maps cleanly onto this scope without overlapping the generic name?

If you cannot answer "scope," "yes," and "yes" — the name needs revision.

---

## D. Reusable Primitive Set

These belong in ICN because every cooperative institution needs them — verified against both what NYCN requires and what a second unrelated institution (housing federation, mutual aid collective) would also need unchanged.

| Primitive | Why ICN | Institution-specific parts |
|-----------|---------|---------------------------|
| **Structure** (Committee/WG/Team/Office) | Every institution delegates authority to sub-groups | Member list, specific authorities, committee names |
| **Meeting** | Every body needs a record of deliberation with decisions and attendance | Agenda templates, meeting norms |
| **Program + Milestone** | Recurring cycles with stage gates exist across institutions | Stage names as `completion_criteria: Vec<String>` data, not enum variants |
| **ActionItem** | Work that materializes from decisions is universal | Assignment rules, priority conventions |
| **Activity + parent_program_id** | Time-bounded endeavors linked to program cycles | Activity names, cycle vocabularies |
| **RoleAssignment** | Authority grants as capability strings, scoped to structures | Capability definitions, role names |

**Stays out of ICN core** (institution-specific; no confirmed cross-institution generality; must be composed in an institution package):

| Object | Why it stays out | Where it goes instead |
|--------|-----------------|---------------------|
| **Sponsor** / funder / donor | A housing federation calls it "Funder"; a mutual-aid network calls it "Partner"; tier semantics, benefit tables, and commitment lifecycle vary per institution. | Institution package. In NYCN: Activity- or Program-linked entity with CCL-governed commitment lifecycle; tiers as CCL data, not ICN enum variants. |
| **Session / speaker** (conference content) | Speaker bios, session tracks, publication state, language availability, representation metadata are conference-specific. A housing federation does not have "sessions." | Institution package. Compose from `Activity` + `Participation` (if Participation ever lands as a generic relational primitive) + `ActionItem`. |
| **Registration / attendee** | Registration flow, accessibility questionnaires, childcare fields, dietary fields, payment state are all conference/event-specific. | Institution package. An institution that needs registration defines its own form schema and attendance model. |
| **AccessibilityState / venue accessibility score** | Scoring rubric and fields are institution- and venue-type-specific. | Institution package. Universal base (e.g., venue address, ADA-compliant yes/no) can eventually be modeled generically, but the scoring rubric is institutional. |
| **BudgetItem as sponsor-commitment** | If shaped as "program-scoped sponsor pledge with status", it is institutional. If re-shaped as a generic "program budget line" (direction, amount, status, linked entity), it *may* qualify as an ICN primitive — but **only after de-institutional redesign**. Default: stays out. | Institution package unless explicitly re-scoped as a generic program-budget primitive (requires a separate design RFC). |
| **SourceRef / InstitutionalDocument** | The primitive is legitimately general (external system link with provenance classification), but **current proposals are tangled with sponsor/session/registration semantics**. Must be designed independent of those before it can land in ICN. | Deferred. A cleanly-generic provenance primitive may land in ICN later as `icn-provenance` or similar — but not as part of any NYCN tranche. |
| **Participation (non-authority: speaker/attendee/volunteer/sponsor-contact)** | A generic subject→object→relation-kind primitive is plausible, but current proposals (`kind: Speaker/Sponsor/Attendee`) import institutional vocabulary into enum variants. | Institution package — or, if landed as ICN, must use `kind: String` (opaque tag) or a minimal generic enum, *not* institution-specific variants. |
| **ServiceRequest / Intake** | Reducible to Activity (kind=project) + ActionItem chain with a routing tag. | Compose in institution package; add an `intake` tag to Activity `Custom` kind. |
| **Contact / OrgRelationship / CRM** | CRM shape and field semantics vary too much across institutions. | Institution package data model. |
| **VolunteerAssignment** | A `RoleAssignment` with `authority_scope: ["volunteer"]` already covers this. | Use the existing `RoleAssignment`. |
| **ReviewQueue** | An `ActionItem` filter view, not a new primitive. | `/gov/me/work?status=pending&tag=review`. |
| **Institution-specific ProgramKind variants** (`AnnualSummit`, `PolicyDay`, etc.) | Tenant vocabulary leaking into a core enum. | `ProgramKind::Custom("annual-summit")`. The landed `icn-governance::program::ProgramKind` includes generic variants (`Campaign`, `Initiative`, `Series`, `Cycle`) that cross-institution workloads use directly; only institution-specific kinds map to `Custom(String)`. See the module doc for the rationale. |
| **Institution-specific MilestoneType variants** (`VenueLocked`, `BudgetLocked`, `PublicLaunchReady`, etc.) | Tenant vocabulary leaking into a core enum. | Free-form `completion_criteria: Vec<String>` on `Milestone`, optionally gated by a CCL `completion_gate` expression. The landed code uses strings, not enum variants. |

### Forbidden module paths (hard rule)

These module paths must not be created in `icn-governance`, `icn-ledger`, `icn-entity`, or any other substrate crate. If a future PR proposes one of these, it should be rejected in review with a link to this section.

- ❌ `icn-governance::sponsor`
- ❌ `icn-governance::session`
- ❌ `icn-governance::registration`
- ❌ `icn-governance::accessibility`
- ❌ `icn-governance::source_ref` *(reconsider only under a cleanly-generic `icn-provenance` design, separate from sponsor/session work)*
- ❌ `icn-governance::document` / `icn-governance::institutional_document` *(same condition as source_ref)*
- ❌ `icn-governance::budget_item` *(reconsider only under a de-institutional redesign that is not a sponsor-commitment lookalike)*
- ❌ `icn-governance::participation` with institution-shaped enum variants *(see §Stays Out table)*
- ❌ any new substrate enum variant named after a specific institution's workflow (`AnnualSummit`, `VenueLocked`, `PlatinumSponsor`, `NYCNOrganizer`, etc.)

### The working pattern (already landed, must be preserved)

`icn-governance/src/program.rs` demonstrates the correct discipline. Its module-level doc explicitly states:

> The canonical spec originally proposed richly named variants — `AnnualSummit`, `PolicyDay`, `VenueLocked` — which bake a specific institution's vocabulary into core enums. That violates the anti-ontology-laundering rule: Layer 1 primitives should be generic enough for a second institution (a housing federation, a mutual-aid collective) to consume unchanged.

> This module therefore ships a deliberately small kind enum with a `Custom(String)` escape hatch, and a free-form `completion_criteria: Vec<String>` for milestones. Institution-specific kinds and checklist semantics belong in the institution package / template layer (Layer 2) or the institution's own repo (Layer 3), not here.

**Every future substrate primitive must follow this pattern.** Narrow enum, `Custom(String)` escape, free-form strings where institutions disagree, CCL for semantics.

---

## E. Implementation Backlog (in priority order)

Items 1 and 2 are landed or in open PRs. Items 3–6 are the next platform moves.

**1. `GET /gov/me/scopes` + `GET /gov/me/work`** *(PR #1552, open)*
Returns `RoleAssignment` set and open `ActionItem`s for the authenticated DID. No new types — compose from existing stores.

**2. `Activity.parent_program_id` linkage** *(PR #1553, open)*
Optional field linking an Activity to its parent Program. Backward-compatible; enables cycle-over-cycle dashboards.

**3. Governance-to-execution bridge: `ObligationEffect` in `GovernanceEffect`** *(Tranche 2)*
When a proposal passes, the current path only handles member sanctions and charter deployment. Add:
```
GovernanceEffect::CreateObligations {
    proposal_id, domain_id, items: Vec<ObligationSpec>
}
GovernanceEffect::AdvanceMilestone { proposal_id, program_id, milestone_id }
```
Institution packages express which proposal types trigger which obligations in their CCL charter. No NYCN vocabulary in the enum variants — `ObligationSpec.title` is a free string.

**4. Work spine filter support on `/gov/me/work`** *(Tranche 2)*
`MyWorkFilterParams` (`status`, `priority`, `overdue`, `tag`) is defined but the handler ignores it. Wire it through `list_work_for_person` so callers can ask for open, high-priority, or overdue items specifically.

**5. `GET /gov/programs/{id}/dashboard` composite view** *(Tranche 2)*
Returns Program + ordered Milestone statuses + ActionItem counts by status + Meeting count. Pure composition from existing stores. No new primitives.

**6. CCL milestone completion gate** *(Tranche 3)*
`completion_criteria: Vec<String>` on Milestone is currently plain text. Add an optional CCL evaluation path: if a criterion starts with `ccl:`, evaluate the expression against current domain state before allowing milestone completion. Enables institution packages to write stage-gate logic in CCL without those predicates existing in ICN core.

---

## F. Institutional feedback and support-primitive boundary

The next set of primitives ICN core will eventually model — **institutional signals, escalation policies, action cards, governed indicators, temporary authority grants, obligation/allocation/settlement, and the support-institution layer** — extends the same boundary discipline to a new surface. Doctrine for these primitives lives in [`INSTITUTIONAL_FEEDBACK_AND_SUPPORT_PRIMITIVES.md`](./INSTITUTIONAL_FEEDBACK_AND_SUPPORT_PRIMITIVES.md). This section pins the placement rule.

### ICN core may know

These are generic shapes that any cooperative federation needs with different content. They are candidates for runtime primitives once a second institution would need each one with the same shape:

- **`InstitutionalSignal`** — a record that something at a defined scope crossed a threshold or was raised by a member and needs attention.
- **`SignalRule`** — a rule (CCL- or data-expressed) that says when a signal is raised at what severity.
- **`EscalationPolicy`** — a routing table for how a signal traverses bodies.
- **`MemberSignal`** — a signal opened by a member acting in their own DID, addressed to a body they belong to.
- **Action card view** — a derived view of standing + open work + open signals + expiring grants. Not a stored entity.
- **`Indicator`** — a measurement with definition, owner, caveats, review interval, and challenge path.
- **`TemporaryAuthorityGrant`** — scoped, expiring, receipted, post-reviewed extension of a member's authority.
- **`Obligation`** — a commitment one party makes to another (sponsor commitment, mutual-aid pledge, federation contribution). Lifecycle: open → fulfilled / partially-fulfilled / lapsed / cancelled.
- **`Allocation`** — an authorized intent to direct units toward a purpose, backed by a governance receipt.
- **`Settlement`** — the terminal record that an allocation actually moved units, with full provenance.
- **`Position`** — a snapshot of an account's units in a settlement asset at a point in time.
- **Support program** — a `Program` with a particular semantic role: education, onboarding, technical/legal/admin support, peer mentorship.
- **Service relationship** — a bilateral declaration that one entity provides a named service to another, under stated terms.
- **Receipt** — the immutable provenance record of every state transition above.

### ICN core must not know

Each of these is a **local concept** — institution-specific vocabulary that NYCN, a housing federation, and a mutual-aid network would each express differently. They belong in institution packages and are forbidden in ICN core code review:

- **Sponsor logo chase** / sponsor lifecycle vocabulary
- **Summit session catalog** / conference session model
- **Childcare checklist** / event accessibility intake
- **Speaker bio** / curated content catalog
- **Venue walkthrough** / venue accessibility scoring rubric
- **Mondragón-style named school / fund / cooperative service** (`MondragonSchool`, `Caja`, `Eroski`)
- **NYCN-specific committee workflow** (`SteeringApprovalDance`, `FinanceVetoFlow`)
- **Fundraising tier names** (`Bronze`, `Silver`, `Platinum`)
- **`OrganizingCooperative` / `SupportFederation` / `ProgramCommunity` as entity primitives** (already pinned by §C2 — restated here because the temptation re-emerges in support-institution work)

### The placement test, restated

If a local concept can be expressed as **`Program` + `Structure` + `ActionItem` + CCL + package data**, it stays in the institution package. If it requires shape that a second unrelated institution would also need, it earns consideration for an ICN-core primitive — and even then, only after a design RFC.

The graduation rule from §B applies here without exception. *"NYCN and Mondragón both want a school"* is not a graduation argument unless their schools share **shape**, not just **theme**.

---

## Boundary in One Sentence

ICN provides the generic institutional operating system — typed shapes, enforced lifecycles, provenance, capability-gated APIs. The institution package provides the charter, the vocabulary, the seed data, and the specific rules that give those shapes meaning for one community.

---

*This document is normative for future crate and route placement decisions. Update when a new primitive is proposed or when a placement decision is contested.*
