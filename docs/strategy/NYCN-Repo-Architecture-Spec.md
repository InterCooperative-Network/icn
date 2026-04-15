# NYCN × ICN Repo-Shaped Architecture Spec

**Status**: Spec (docs-only, grounded against `main` at `37ec91e0` — 2026-04-14)
**Supersedes**: portions of `NYCN-Institutional-Design.md` (see §0.3)
**Companion docs**:
- `NYCN-Implementation-Matrix.md` — per-object gap table (exists / partial / missing)
- `NYCN-Execution-Tranches.md` — merge-order + dependency plan

---

## 0. Preamble

### 0.1 Purpose

Turn the NYCN/NYCS institutional design into a **repo-shaped technical map**: which crates, which modules, which stores, which events, which HTTP routes. Ground every claim in code that exists on `main` today, on open PRs, or in committed design docs — and clearly distinguish the three.

This is **not** a re-derivation of the institutional model. The institutional model was locked on 2026-04-14 (layered ontology: sovereign entities vs. internal structures vs. time-bounded activities). This document is the mechanical follow-through.

### 0.2 Ground truth as of writing

Verified directly against the repo on 2026-04-14:

| Fact | Source | Value |
|------|--------|-------|
| Main HEAD | `git log origin/main` | `37ec91e0 feat(governance): institutional structure + event model (Tranche 2, part 1) (#1540)` |
| PR #1532 decision→action bridge | `gh pr view 1532` | **MERGED** |
| PR #1533 consent mode | `gh pr view 1533` | **MERGED** |
| PR #1540 structure + activity + parent | `gh pr view 1540` | **MERGED** |
| PR #1542 security audit fix | `gh pr view 1542` | **MERGED** |
| PR #1543 meeting management | `gh pr view 1543` | **OPEN, mergeable, CI green except benchmark regression (non-blocking with `perf-regression-ok`)** |
| `feat/notification-digests` | `git branch` | Local + remote, **no PR opened** |
| `docs/strategy/NYCN-*` docs | filesystem | `Charter-Draft.yaml`, `Implementation-Plan.md`, `Institutional-Design.md`, `Institutional-Strategy.md`, `Sprint-Plan.md` (NOT the `Bootstrap-Runbook.md` / `Schema-Mapping.md` the prior session claimed — those do not exist) |

### 0.3 Correction to prior design docs

`NYCN-Institutional-Design.md` (committed 2026-04-13) maps committees (finance, content, logistics, marketing, steering) and `nycn-summit-2026` to `Community(CommunityProfile)` entities. **This is superseded.** The locked layered ontology (2026-04-14, merged as #1540) states:

- **Entities** (sovereign, join federations, hold independent treasury): NYCN federation, `nycn-organizers` co-op, member co-ops.
- **Structures** (non-sovereign, owned by a parent entity, delegated authority): committees, working groups, backbone group, host pods. `icn-governance::structure::Structure { kind: Committee | WorkingGroup | Team | Office }`.
- **Activities** (time-bounded, owned by a parent entity): the summit cycle, regional meetup series, policy-advocacy day, sponsor campaigns. `icn-governance::activity::Activity { kind: Event | Program | Project | Initiative }`.

Operational objects (action items, meetings, documents) attach to any of the three layers via `icn-governance::parent::InstitutionalParent`.

Concretely, the NYCN topology becomes:

```
nycn                          Entity: Federation
├── nycn-organizers           Entity: Cooperative (parent_id: "nycn")
│   ├── nycn-backbone         Structure { kind: Committee, parent_entity_id: "nycn-organizers" }
│   ├── nycn-finance          Structure { kind: Committee, parent_entity_id: "nycn-organizers" }
│   ├── nycn-content          Structure { kind: Committee, parent_entity_id: "nycn-organizers" }
│   ├── nycn-logistics        Structure { kind: Committee, parent_entity_id: "nycn-organizers" }
│   ├── nycn-marketing        Structure { kind: Committee, parent_entity_id: "nycn-organizers" }
│   └── nycn-accessibility-wg Structure { kind: WorkingGroup, parent_entity_id: "nycn-organizers" }
└── summit-2026               Activity { kind: Event, parent_entity_id: "nycn-organizers" }
    (references committees via Activity.structures: Vec<StructureId>)
```

The older design doc's entity tree should be edited in a follow-up docs PR to match this.

### 0.4 Decisive test

> Can a new organizer enter the system mid-cycle, switch into the right scope, see the summit's current phase, understand what was decided, know what is blocked, receive their obligations, trace why they exist, and continue the work without needing private oral history?

Every architectural choice below is evaluated against that question.

---

## 1. Current-state audit

### 1.1 What is already implemented on `main`

Grounded in direct inspection of `icn/crates/icn-governance/src/` and `icn/apps/governance/src/`:

| Capability | Code location | Notes |
|-----------|---------------|-------|
| Governance domain + proposals + voting | `icn-governance::{domain,proposal,vote,tally,profile}` | Mature |
| Consent decision mode (majority / consent, `max_objections`) | `icn-governance::config`, `profile.rs` | #1533 merged |
| Action items (CRUD, status, priority, notes, filter, sled store) | `icn-governance::action_item` (1281 LOC) | Full store trait + `SledActionItemStore` + assignee index |
| Decision→action bridge: accepted proposals auto-materialize `ActionItemSpec`s with provenance | `icn-governance::proposal::ActionItemSpec`, `apps/governance/src/actor.rs` | #1532 merged |
| Institutional Structure (Committee / WorkingGroup / Team / Office) + sled store | `icn-governance::structure` (548 LOC), `apps/governance/src/manager.rs::SledStructureStore` | #1540 merged |
| Role Assignment (`structure_id`, `person_did`, `role: String`, `authority_scope: Vec<String>`, start/end) | `icn-governance::structure::RoleAssignment` | #1540 merged. Authority is capability-string based, not a typed enum. |
| Activity (Event / Program / Project / Initiative) + sled store | `icn-governance::activity` (335 LOC), `apps/governance/src/manager.rs::SledActivityStore` | #1540 merged |
| Polymorphic `InstitutionalParent` attachment for action items | `icn-governance::parent`, `action_item::ActionItem.parent` | #1540 merged |
| Federation charter / charter store | `icn-governance::{charter, charter_store}` | Mature |
| Steward / steward store (SDIS) | `icn-governance::{steward, steward_store}` | Mature |
| Delegation | `icn-governance::delegation` (1717 LOC) | Mature |
| Discussion / appeal / amendment | `icn-governance::{discussion, appeal, amendment}` | Mature |
| Gateway events (structure/activity/proposal lifecycle) | `icn-gateway::events::GatewayEvent` | |
| HTTP surface for governance | `apps/governance/src/http/{configure,handlers,models}.rs` | Proposals, action items, structures, activities wired |

### 1.2 What is on open / unlanded branches

| Capability | Branch | Status |
|-----------|--------|--------|
| **Meeting** primitive (564 LOC model + store + 9 HTTP endpoints + gateway events) | `feat/meeting-management` / **PR #1543** | OPEN, mergeable, CI green except non-blocking benchmark regression. Review comments from codex + copilot-reviewer; no blocking human review yet. |
| **Notification digest** endpoint (`DigestSummary`, `PendingVoteDigest`, `OverdueItemDigest`, `list_by_assignee` default + sled assignee secondary index, `GET /digest` handler) | `feat/notification-digests` | Local + remote branch, 1 commit ahead of main, **NO PR yet opened**. Upcoming-meetings stubbed at 0 because #1543 wasn't merged. Assignee secondary index not backfilled. |

### 1.3 What exists only as strategy / design docs

- `docs/strategy/NYCN-Institutional-Strategy.md`
- `docs/strategy/NYCN-Institutional-Design.md` (partially superseded — see §0.3)
- `docs/strategy/NYCN-Implementation-Plan.md` (5-phase plan; phases 1/2 shipped, phase 3 in PR, phase 4 on unpushed-PR branch, phase 5 unstarted)
- `docs/strategy/NYCN-Sprint-Plan.md` (3-lane sprint, largely executed)
- `docs/strategy/NYCN-Charter-Draft.yaml` (CCL draft for NYCN federation charter)

### 1.4 What is missing entirely

No code or open branch addresses:

1. **Program/cycle container** as a first-class domain concept beyond `Activity`. Activity is enough for the summit as an *event*; it does **not yet carry milestones, stage gates, closure state, or cycle-handoff hooks**.
2. **Program milestones** with machine-readable required-check predicates (`venue_locked`, `budget_locked`, `public_launch_ready`, `closure_complete`).
3. **Sponsor pipeline**: no `Sponsor` type, no status lifecycle, no pipeline view.
4. **Budget items / financial obligations** at the program level (treasury exists at the entity level, but no per-program earmarking).
5. **Sessions / speakers**: no `Session` primitive, no publication state, no language-needs tracking.
6. **Registrations** with structured accessibility/childcare/language/dietary fields.
7. **Accessibility state model**: no typed accessibility scoring for venues/programs.
8. **Institutional document / memory store**: no typed `InstitutionalDocument` with version lineage, authorship, content addressing, or meeting/proposal linkage. (Phase 5 of the implementation plan; unstarted.)
9. **External-source provenance edges**: no `SourceRef` type to link Google Docs, Sheets, mailing lists, ny-coop-net rows, legacy repos with `linked | mirrored | imported | derived` provenance classification.
10. **Delegation of role-bound capabilities** with explicit grantor/grantee/scope/authority/time-window records distinct from the `delegation.rs` crate (which is about governance delegation, not operational scope delegation).
11. **Digest partitioning** by scope/role; current digest is DID-level only.
12. **Cycle-comparison / year-over-year view** derived from activity records.
13. **Organizer onboarding pipeline** (interest → orientation → role → first task).

### 1.5 Meaning-firewall status

`icn-governance` is an **app-layer** crate by convention (it implements `PolicyOracle`, it imports `icn-identity::Did`). It is legitimately allowed to hold domain types. What the firewall forbids is **kernel** crates importing domain types. Nothing in this architecture proposes kernel imports of NYCN-specific anything. Every NYCN concept lives inside `icn-governance` (domain primitive) or `apps/governance` (persistence + HTTP).

The only firewall risk to watch: **do not hardcode NYCN-specific string constants into kernel crates.** Use NYCN as a test fixture, not a dependency.

---

## 2. Bounded domain mapping

Eleven bounded domains were identified in the institutional review. Map each to the concrete ICN ownership:

| Domain | Owning crate(s) | Status | Notes |
|--------|----------------|--------|-------|
| Identity & participation | `icn-identity`, `icn-entity`, `icn-governance::membership`, `icn-governance::structure::RoleAssignment` | ✅ sufficient for NYCN bootstrap | Membership + role-assignment cover organizers, committee members, partner-org membership |
| Institutional structure | `icn-governance::structure` + `apps/governance/src/manager.rs::SledStructureStore` | ✅ covers committees / working groups / backbone / host-pods | Do **not** promote committees to entities |
| Governance | `icn-governance::{proposal, vote, tally, profile, config, discussion, appeal, amendment}` + `apps/governance/src/actor.rs` | ✅ mature, incl. consent mode + decision→action bridge | |
| **Program / cycle** | **Proposed: `icn-governance::program` (NEW)** on top of `icn-governance::activity` | ⚠️ Activity exists, Program container + milestones missing | See §5 |
| Meeting | `icn-governance::meeting` + `apps/governance/src/manager.rs::SledMeetingStore` | 🚧 on PR #1543 | Merge, validate with an NYCN scenario test, then build on it |
| Work execution | `icn-governance::action_item` + decision-bridge | ✅ with `InstitutionalParent` attachment | `meeting_id: Option<MeetingId>` field added on #1543 |
| Communications / digests | `apps/governance::manager::{DigestSummary, PendingVoteDigest, OverdueItemDigest}` on `feat/notification-digests` | 🚧 unpushed PR, upcoming-meetings stub | Must be redone to consume #1543's meeting store once merged, and eventually partitioned by scope |
| Funding / resource | Treasury: `icn-ledger` (mature). **Sponsor / budget items: MISSING** | ❌ missing | Propose `icn-governance::sponsor` + `icn-governance::budget_item` (or place in a dedicated `apps/programs` app if the scope grows) |
| Content / session | **MISSING** | ❌ missing | Propose `icn-governance::session` |
| Registration / attendee | **MISSING** | ❌ missing | Propose `icn-governance::registration` with structured accessibility fields |
| Archive / provenance | `icn-governance::proof` (governance receipts), `icn-store` (sled). **InstitutionalDocument: MISSING** | ⚠️ provenance exists for decisions; institutional docs missing | Phase 5 of implementation plan |

**Design principle carried from the institutional review**: each domain gets its own module inside `icn-governance`, with a paired `Sled<X>Store` in `apps/governance/src/manager.rs` and an HTTP surface in `apps/governance/src/http/`. This matches the pattern already established by `structure`, `activity`, `action_item`, and (in PR) `meeting`. Do not invent new crate boundaries until a domain has justified it (e.g. sponsor + budget + session may eventually move to an `apps/programs` app once they reach ~3k LOC and their own oracle semantics emerge).

---

## 3. Canonical object mapping

Every object from the institutional review, mapped to repo reality.

| Object | Canonical home | Status | Notes |
|--------|---------------|--------|-------|
| `Entity` | `icn-entity` (existing) | ✅ | `Federation`, `Cooperative`, `Community`, `Individual` — used for NYCN, nycn-organizers, member co-ops only |
| `Person / Actor` | `icn-identity::Did` | ✅ | |
| `Participation` | `icn-governance::membership` + `RoleAssignment` | ✅ for member/role cases | Sponsor / speaker / attendee participation is MISSING (see §3 below) |
| `Structure` | `icn-governance::structure::Structure` | ✅ | `kind: Committee/WorkingGroup/Team/Office` |
| `RoleAssignment` | `icn-governance::structure::RoleAssignment` | ✅ | Scope-aware; authority represented by `authority_scope: Vec<String>` capability strings + freeform `role` — **not** a typed authority-level enum |
| **Program** | proposed `icn-governance::program::Program` | ❌ missing | Wraps `Activity` with milestones, phase state, closure hooks |
| `Activity` | `icn-governance::activity::Activity` | ✅ | Use as building block for Program |
| `Meeting` | `icn-governance::meeting::Meeting` | 🚧 PR #1543 | |
| `AgendaItem` | `icn-governance::meeting::AgendaItem` | 🚧 PR #1543 | |
| `Proposal / Decision` | `icn-governance::proposal::Proposal` | ✅ | `action_items_on_accept` already wired |
| `ActionItem` | `icn-governance::action_item::ActionItem` | ✅ | Carries `parent`, `linked_proposal`, `meeting_id` (on PR #1543) |
| **Campaign / Initiative** | `Activity { kind: Initiative }` or `Activity { kind: Campaign }` | ⚠️ partial | `ActivityKind` enum has `Initiative` but no `Campaign` variant. Add if needed. |
| **Sponsor record** | proposed `icn-governance::sponsor::Sponsor` | ❌ missing | |
| **Budget item** | proposed `icn-governance::budget_item::BudgetItem` | ❌ missing | |
| **Session / speaker** | proposed `icn-governance::session::Session` | ❌ missing | |
| **Registration** | proposed `icn-governance::registration::Registration` | ❌ missing | |
| **SourceRef / document link** | proposed `icn-governance::source_ref::SourceRef` (or lift to `icn-provenance` if cross-domain) | ❌ missing | |
| **Digest packet** | `apps/governance::manager::DigestSummary` | 🚧 on `feat/notification-digests` | Needs rework to consume meeting store once #1543 lands |
| **InstitutionalDocument** | proposed `icn-governance::document::InstitutionalDocument` | ❌ missing | Phase 5 of implementation plan |
| **Milestone** | proposed `icn-governance::program::Milestone` | ❌ missing | Tied to Program |
| **Delegation (operational)** | `icn-governance::delegation` (governance-flavored) | ⚠️ partial | Existing `delegation.rs` is for governance voting delegation. Operational role-delegation is a different shape; may be a new module `role_delegation.rs` or fields on `RoleAssignment` |

### 3.1 Participation gap

The institutional model needs participation records that are *not* governance membership:

- Attendee → Program
- Speaker → Session
- Sponsor-contact (person) → Sponsor record
- Volunteer → Program or Structure

Two options:
1. **Extend `RoleAssignment`** with more `role_type` values (but this overloads governance semantics onto non-governance relationships).
2. **Add a separate `Participation` type** in `icn-governance::participation` that is *relational* (subject → object → type) without granting authority.

**Recommendation**: option 2. Governance-bearing participation stays in `RoleAssignment`. Non-authority participation (sponsor, speaker, attendee, volunteer) lives in a new lightweight `Participation` module. This preserves the meaning firewall inside the app layer — authority-granting relationships remain auditable and distinct from data-only relationships.

---

## 4. Authority and scope model

### 4.1 What currently supports scoped authority

`RoleAssignment` (in `icn-governance::structure`) carries:
- `structure_id` — the structure the assignment belongs to
- `person_did` — who holds the assignment
- `role: String` — a freeform role label (e.g. `"coordinator"`, `"facilitator"`, `"note_taker"`, `"member"`)
- `authority_scope: Vec<String>` — delegated capability strings within that role (narrower than or equal to the structure's scope)
- `start_date` / `end_date` — time-bounded
- `assigned_by_decision: Option<ProposalId>` — provenance link if proposal-backed

Combined with `MembershipCapability` on entity membership, a user can already hold a structure-specific governance role with delegated capability scopes — e.g., "Matt is assigned as finance-committee lead on `nycn-finance` with `authority_scope: ["propose", "approve-budget-<=5000"]`". **This is the primitive scoped-authority plumbing.**

What is missing:
- **Active-scope selection** at the HTTP/session layer. The gateway currently takes JWTs with `coop_id` and `scopes`. It does not yet have a first-class "act-as-structure-X-with-role-Y" request-context mechanism resolved against the `RoleAssignment` store.
- **Operational role delegation** for temporary handoff ("Matt delegates venue-selection to Priya for 2 weeks"), beyond the delegated capabilities already on an assignment.

### 4.2 Proposed additions

1. **`SessionScope` in gateway request context** — resolved from DID + active structure/role claim against the `RoleAssignment` store. Belongs in `icn-gateway::auth` / wherever JWT claims are validated.
2. **`RoleDelegation`** — new `icn-governance::role_delegation` module. Fields: `grantor`, `grantee`, `scope_ref`, `capabilities: Vec<String>` (same capability-string vocabulary as `authority_scope`), `starts_at`, `ends_at`, `reason`, `revoked_at`. Distinct from `icn-governance::delegation` (which is vote-delegation).

### 4.3 Backbone authority

The backbone group is one of the most politically load-bearing concepts in NYCN and one of the least supported today. Model it as:

```rust
Structure {
    id: StructureId::from_raw("nycn-backbone"),
    parent_entity_id: "nycn-organizers",
    kind: StructureKind::Committee,
    // metadata: { "authority_class": "backbone", "requires_ratification_for": ["...categories..."] }
}
```

Role holders on the backbone structure carry capability strings like `"backbone-authority"` in their `authority_scope`. Authority-rule enforcement is then a **PolicyOracle decision** keyed off those capability strings — not hard-coded match arms, not a new enum. Specifically: when a proposal is opened in a backbone-scoped domain with a type that requires full-network ratification, the oracle refuses to finalize unilaterally and flags it as requiring a downstream ratification proposal in the parent entity's governance domain. This keeps the meaning firewall intact (CCL/oracle config expresses which proposal types require ratification; the code does not hard-code NYCN categories) and makes the rule set data-driven.

---

## 5. Program / cycle model

**Decision**: `Activity` is not enough. `Activity { kind: Event }` captures the summit-as-thing; it does not capture the summit-as-cycle-with-stage-gates. Promote to a `Program` primitive that wraps `Activity`.

### 5.1 Proposed module: `icn-governance::program`

```rust
pub struct ProgramId(pub String);

pub enum ProgramKind {
    AnnualSummit,       // summit-2026
    Campaign,           // sponsor outreach push
    Initiative,         // organizer recruitment drive
    Series,             // regional meetup series
    PolicyDay,          // advocacy day
}

pub enum ProgramStatus {
    Draft,
    ActivePlanning,
    PublicLaunch,
    InExecution,
    Closed,
    Archived,
}

pub struct Program {
    pub id: ProgramId,
    pub parent_entity_id: String,
    pub activity_id: Option<ActivityId>,  // optional link for legacy Activity-only consumers
    pub kind: ProgramKind,
    pub name: String,
    pub year: Option<u32>,
    pub status: ProgramStatus,
    pub start_at: Option<Timestamp>,
    pub end_at: Option<Timestamp>,
    pub parent_program_id: Option<ProgramId>,
    pub milestones: Vec<Milestone>,
    pub metadata: serde_json::Value,
    pub created_at: Timestamp,
    pub closed_at: Option<Timestamp>,
}

pub struct Milestone {
    pub id: MilestoneId,
    pub program_id: ProgramId,
    pub milestone_type: MilestoneType,
    pub required_checks: Vec<CheckRef>,
    pub status: MilestoneStatus,
    pub completed_at: Option<Timestamp>,
    pub completed_by: Option<Did>,
}

pub enum MilestoneType {
    StrategyLocked,
    VenueLocked,
    BudgetLocked,
    PublicLaunchReady,
    EventReady,
    ClosureComplete,
    Custom(String),
}
```

### 5.2 Why not just fatten `Activity`?

1. **Lifecycle divergence**: Activities have 4 statuses (`Planned/Active/Completed/Cancelled`). Programs have 6+ including `PublicLaunch` and `InExecution` which are operationally distinct from "active". Overloading `ActivityStatus` degrades signal.
2. **Stage-gate semantics**: Milestones with machine-readable required-check predicates are a Program concept, not a generic Activity concept. A regional meetup series (also an Activity) doesn't want milestones; it wants recurrence.
3. **Cycle-comparison**: `parent_program_id` lets `summit-2026` point to `summit-2025` for year-over-year views. This is awkward as a generic Activity self-reference.
4. **Sprint-plan coherence**: Several future objects (sponsor, budget_item, session, registration) naturally hang off a Program, not a generic Activity.

**Activity remains the right level for**: meetings' parent context, one-off workshops, ad-hoc projects that don't need staging.
**Program is the right level for**: recurring cycles with closure/handoff and stage-gated readiness.

### 5.3 Storage + HTTP

- `apps/governance/src/manager.rs::SledProgramStore` — follows the `SledActivityStore` pattern.
- `apps/governance/src/http/`:
  - `POST /programs`
  - `GET /programs`
  - `GET /programs/{id}`
  - `POST /programs/{id}/milestones`
  - `PATCH /programs/{id}/milestones/{mid}` (complete / fail / reset)
  - `POST /programs/{id}/close`
  - `GET /programs/{id}/dashboard` — derived view; see §9

### 5.4 Events

Add to `icn-gateway::events::GatewayEvent`:
- `ProgramOpened`
- `ProgramClosed`
- `MilestoneCompleted`
- `MilestoneBlocked`

---

## 6. Event-driven flow mapping

ICN already has `GatewayEvent` emission on **proposal lifecycle** (domain create, proposal create/open/close, vote cast). It does **not** yet emit events for action-items, structures, or activities. The NYCN integration requires both adding variants **and** wiring the emission sites — not just consumer materializers.

Current governance-related variants on `main` (verified in `icn/crates/icn-gateway/src/events.rs`):

- `GovernanceDomainCreated`
- `GovernanceProposalCreated`
- `GovernanceProposalOpened`
- `GovernanceProposalClosed { outcome: String, ... }`
- `GovernanceVoteCast`

### 6.1 Governance → task chain (partial — consumer side wired, event side missing)

State-change side **already works on `main`** (the decision→action bridge materializes action items in `apps/governance/src/actor.rs` after a proposal is accepted). The **event emission** for downstream consumers is incomplete:

```
proposal.accepted (state change)
  → action_item records created (✅ on main, via ActionItemSpec materialization)
  → GatewayEvent::GovernanceProposalClosed { outcome: "accepted", ... }  (✅ on main)
  → GatewayEvent::GovernanceActionItemCreated (per item)                  (❌ not emitted; must be added)
```

Gap 1: no per-action-item event is emitted. Consumers that need to react on materialization (digests, external integrations) cannot today — they must poll stores.
Gap 2: no `digest.generated` event. Add when digest service gets a real task loop.

**Recommended placement**: add `GovernanceActionItemCreated { item_id, domain_id, linked_proposal, assignee }` as part of the Tranche 0b (notification-digests) PR, so the digest architecture can converge on event-driven materialization from the start.

### 6.2 Meeting closure chain (after #1543 merges)

```
meeting.held  (status = Held)
  → notes linked (document ref; see Phase 5)
  → decisions recorded (proposal references on agenda items)
  → action_items created (from AgendaItem.generated_action_items on close)
  → post_meeting_digest.generated (new)
```

Requires a `POST /meetings/{id}/close` handler that:
1. Validates meeting is `InProgress`.
2. For each `AgendaItem` with unresolved state, optionally generates an `ActionItem` with `parent = InstitutionalParent::Activity(...)` or `Structure(...)` and `meeting_id` set.
3. Emits `GatewayEvent::MeetingEnded`.
4. Optionally triggers a digest packet generation addressed to attendees.

### 6.3 Registration → operations chain

```
registration.confirmed
  → attendee_count += 1 on program
  → accessibility_requirements tallied
  → logistics digest update
```

Not implementable until `Registration` primitive exists.

### 6.4 Sponsor pipeline chain

```
sponsor.status_changed(confirmed)
  → budget_item.created (income, amount, program)
  → recognition_obligation.created (action item, parent = program, assignee = marketing-committee lead)
  → finance_digest entry
```

Not implementable until `Sponsor` + `BudgetItem` exist.

### 6.5 Meaning-firewall note on events

Gateway events are **domain-shaped** but must remain kernel-neutral. Adding `SponsorConfirmed` to `GatewayEvent` is fine. What is **not** fine: embedding NYCN-specific payload fields ("nycn_sponsor_tier": "platinum") in the event. Sponsor tiers belong in the `Sponsor` record, not in the event envelope.

---

## 7. Stage gates as machine-readable milestones

Already described in §5. Key implementation points:

- `CheckRef` should be an enum pointing to concrete predicates (`VenueSelected`, `HotelAccessReviewEntered`, `TransitAccessReviewEntered`, `AccessibilityScoreEntered`, `PublicCopyPresent`, …).
- Evaluating a milestone = running all its `required_checks` against the program state + linked records.
- **Milestones do not auto-complete.** A user with appropriate scope (role on the owning structure or program lead) completes them explicitly, with the evaluator confirming all required checks pass.
- Gating transitions (Draft → ActivePlanning → PublicLaunch → InExecution → Closed) on required milestone completion enforces stage-gate discipline without hardcoding NYCN's specific process.

---

## 8. Query / organizer UX model

### 8.1 Derived views required

| View | Inputs | Backing endpoint | Current status |
|------|--------|-----------------|----------------|
| `MyActionQueueView` | `ActionItem` by assignee, status, due | `GET /digest` (partial) | 🚧 on `feat/notification-digests`; needs completion |
| `MyScopeView` | `RoleAssignment` by actor | **MISSING** (no `GET /me/scopes` handler) | ❌ |
| `ProgramDashboardView` | `Program` + linked `Milestone`s + `ActionItem` counts + `Sponsor` counts + `Registration` counts | **MISSING** | ❌ (depends on Program + others) |
| `CommitteeOperationalView` | `Structure` + recent meetings + open action items | partially buildable from existing stores | ⚠️ needs composition handler |
| `SponsorPipelineView` | `Sponsor` by status | depends on Sponsor | ❌ |
| `AccessibilityStatusView` | `Registration.accessibility_notes` + program venue accessibility fields | depends on Registration + venue model | ❌ |
| `PostMeetingSummaryView` | `Meeting` + linked decisions + generated action items | depends on #1543 | 🚧 |
| `CycleComparisonView` | `Program` + `parent_program_id` → previous cycle's objects | depends on Program | ❌ |
| `OrganizerOnboardingView` | Onboarding pipeline state | **MISSING** (requires onboarding subsystem) | ❌ |

### 8.2 Implementation pattern

Each view = a struct in `apps/governance/src/http/views.rs` (new file) + a `GET` handler that composes multiple store reads. **Do not** build a separate "view store" with its own persistence; views are computed-from-canonical. If performance becomes a concern, add cached materializers behind feature flags.

### 8.3 Where the mobile SDK ties in

`docs/mobile/icn-mobile-ux-spec-v1.md` already anchors mobile surfaces to gateway API. The NYCN-facing mobile views (My Work, My Scope, Upcoming Meetings, Program Dashboard) should consume these same endpoints. Do not branch mobile-specific endpoints.

---

## 9. Communications and digest architecture

### 9.1 Current state

`feat/notification-digests` gives us:
- `DigestSummary { pending_votes: Vec<PendingVoteDigest>, overdue_items: Vec<OverdueItemDigest>, upcoming_meetings: Vec<_> (stub) }`
- `GovernanceManager::generate_digest(did) -> DigestSummary`
- `GET /digest?did=...` handler
- `list_by_assignee` on `ActionItemStoreBackend`
- Assignee secondary index in `SledActionItemStore`

### 9.2 Required rework before shipping

Blocking issues identified in the review:

1. **Upcoming meetings stub** — wire against `SledMeetingStore` once #1543 merges. The digest branch was created before meetings landed, so this field currently returns `Vec::new()`.
2. **Assignee index not backfilled** — new items get indexed, existing items do not. Add a one-shot migration in `SledActionItemStore::ensure_assignee_index()` called on store init that walks all items once if an index version marker is absent.
3. **No scope partitioning** — current digest is DID-level. Phase 4.5: partition by structure/program scope so a user can get "my finance committee digest" distinct from "my summit-2026 digest".
4. **No delivery** — digest is pull-only. That's fine for v1, but add a `DigestDelivery` record (not yet implemented, not blocking) for later email/webhook integration.

### 9.3 Digest types to expand to

v1 (current PR scope): personal work digest
v1.1 (follow-up): committee digest, program digest, pre-meeting packet, post-meeting summary
v2 (later): decision announcement, sponsor follow-up, organizer onboarding sequence

### 9.4 Storage rule

Digests are **ephemeral derivations** unless explicitly persisted. `DigestSummary` is a response shape, not a stored record. If we later need "show me last week's digest as delivered", introduce a `DigestPacket` record with `generated_at`, `scope_ref`, `content_hash`, `delivery_state`. Do not prematurely store digests.

---

## 10. Accessibility and representation as structured data

### 10.1 Proposal: `icn-governance::accessibility`

A lightweight module that defines:

```rust
pub struct VenueAccessibility {
    pub transit_score: Option<AccessibilityScore>,
    pub hotel_walkability_score: Option<AccessibilityScore>,
    pub childcare_fit: ChildcareFit,
    pub interpretation_supported: Vec<LanguageCode>,
    pub sobriety_friendly: bool,
    pub navigation_clarity_score: Option<AccessibilityScore>,
    pub restroom_inclusion_notes: Option<String>,
    pub family_support_notes: Option<String>,
    pub environmental_quality_notes: Option<String>,
}

pub enum AccessibilityScore { Poor, Fair, Good, Excellent, Unknown }
pub enum ChildcareFit { None, OnSite, NearSite, NotEvaluated }
```

This struct is embedded on `Program` (for program-level fit) and referenced from venue records (TBD — venues may just live in `Program.metadata` initially).

### 10.2 Registration-side accessibility fields

When `Registration` is implemented, it carries:
- `dietary_needs: Vec<String>`
- `childcare_needs: Option<ChildcareNeed>`
- `language_needs: Vec<LanguageCode>`
- `accessibility_notes: Option<String>` (free text for requests the structured fields don't cover)

The `AccessibilityStatusView` joins registration fields with venue fit to surface gaps.

### 10.3 Representation tracking

Add optional fields on `Session`:
- `sector: Option<String>` (worker-coop, ag, housing, solidarity-econ, platform, …)
- `geography: Option<String>`
- `language: LanguageCode`
- `cooperative_type: Option<String>`

Plus a derived representation dashboard that aggregates these. Early implementation can be a `GET /programs/{id}/representation` handler that buckets confirmed sessions.

---

## 11. External tool integration and migration boundaries

### 11.1 `SourceRef` primitive

Proposed module `icn-governance::source_ref`:

```rust
pub enum SourceSystem {
    GoogleDoc,
    GoogleSheet,
    DriveFile,
    LegacyRepo,
    MailingList,
    NyCoopNetRow,
    ManualEntry,
    Other(String),
}

pub enum ProvenanceType {
    Linked,     // reference only
    Mirrored,   // synced copy in ICN
    Imported,   // structured extraction
    Derived,    // generated from other canonical records
}

pub struct SourceRef {
    pub id: SourceRefId,
    pub source_system: SourceSystem,
    pub external_identifier: String,
    pub title: Option<String>,
    pub uri: Option<String>,
    pub captured_at: Timestamp,
    pub checksum: Option<String>,
    pub provenance_type: ProvenanceType,
}
```

Every canonical object that was imported from an external system should carry `source_refs: Vec<SourceRef>` (or equivalent).

### 11.2 ny-coop-net migration

`~/projects/ny-coop-net` (if present) is the legacy Postgres-backed operational app. The migration pattern:

1. For each table in ny-coop-net, map rows to canonical ICN objects (entities, participations, programs, sponsors, etc.).
2. Each materialized ICN record carries a `SourceRef { system: NyCoopNetRow, external_identifier: "<table>:<row_pk>" }`.
3. Build a one-shot importer binary (`bins/icn-nycn-import` or a script in `scripts/`) that reads ny-coop-net and writes ICN records idempotently (re-running should not double-import).
4. Do NOT write directly to ICN sled stores. Use the HTTP API so that the importer exercises the same validation path.

### 11.3 Google Docs / Sheets

- Link-only in v1 via `SourceRef { system: GoogleDoc, uri: "..." }`.
- Mirroring and structured import are Phase 5 (document primitive) territory.

### 11.4 Mailing lists

- Link only. A user explicitly attaches a mailing-list archive link to a program or structure. Do not attempt mailing-list import in this integration.

---

## 12. Provenance and audit requirements

### 12.1 Built-in provenance

ICN's governance proof (`icn-governance::proof`), action item `linked_proposal`, and `ActionItem.parent` (`InstitutionalParent`) already give us:

- Proposal → action item lineage (via `linked_proposal` + provenance hash in the decision-to-action bridge)
- Action item → institutional scope (via `parent`)
- Action item → meeting (via `meeting_id`, on PR #1543)

### 12.2 Gaps

- **Sponsor state changes** — will need an audit log of (who, when, old state, new state) when `Sponsor` is implemented. Pattern: store state-change events alongside the record, either inline (`Vec<StatusChange>`) or as a separate `SponsorStatusLog`.
- **Milestone completion** — record `completed_by: Did` + `completed_at`. This is already in the proposed Milestone struct.
- **Registration confirmation** — needs payment-state history if any.
- **Document version lineage** — Phase 5; `InstitutionalDocument.parent_doc` gives us content-addressed version chains.

### 12.3 Rule

Every state transition on a canonical object should be attributable. If the Rust type doesn't record the actor of a transition, add a `changed_by: Did` field or a side-log, but do not silently mutate without actor attribution.

---

## 13. API surface expectations

All governance routes are mounted under the `/gov` scope in the gateway (see `icn/crates/icn-gateway/src/server.rs:2008`). Routes that act on a specific governance domain or parent entity are nested under the scope segment — they are **not** flat top-level paths.

### 13.1 Existing on `main` (verified in `apps/governance/src/http/configure.rs`)

- Domains: `POST/GET /gov/domains`, `GET /gov/domains/{domain_id}`, `POST/DELETE /gov/domains/{domain_id}/members`
- Proposals: `POST/GET /gov/proposals`, `GET /gov/proposals/{id}`, `POST /gov/proposals/{id}/open`, `POST .../close`, `POST .../vote`, `GET .../tally`, `GET .../proof`, `GET .../chain`, `GET .../discussion`, plus `discussion/comments[/{id}[/reactions]]`
- Delegations: `POST/GET /gov/delegations`, `DELETE /gov/delegations/{id}` (revoke — no GET-by-id handler exists on `main`)
- Action items: `POST/GET /gov/domains/{domain_id}/action-items`, `GET /gov/domains/{domain_id}/action-items/{item_id}`, `PUT .../status`, `POST .../notes`
- Structures: `POST/GET /gov/entities/{entity_id}/structures`, `GET /gov/structures/{structure_id}`, `POST /gov/structures/{structure_id}/roles`
- Activities: `POST/GET /gov/entities/{entity_id}/activities`, `GET /gov/activities/{activity_id}`
- Federation / SDIS proposal shortcuts: `/gov/proposals/federation/...`, `/gov/proposals/sdis/...`

### 13.2 On PR #1543 (meetings)

- `POST/GET /gov/meetings`, `GET /gov/meetings/{id}`, `PATCH /gov/meetings/{id}`
- `POST /gov/meetings/{id}/agenda-items`, `PATCH .../{aid}`
- `POST /gov/meetings/{id}/start`, `POST .../end`
- Attendance endpoint (verify exact path against PR)

### 13.3 Proposed additions (by tranche)

- **Tranche 0b (digest)**: `GET /gov/digest` (DID-level)
- **Tranche 1 (Program + views)**:
  - Programs: `POST/GET /gov/entities/{entity_id}/programs`, `GET /gov/programs/{program_id}`, `POST /gov/programs/{id}/milestones`, `PATCH /gov/programs/{id}/milestones/{mid}`, `POST /gov/programs/{id}/close`, `GET /gov/programs/{id}/dashboard`
  - Scope + work: `GET /gov/me/scopes`, `GET /gov/me/work`
  - Scope-partitioned digest: `GET /gov/digest?scope=structure:nycn-finance`
- **Tranche 2 (funding)**:
  - Sponsors: `POST/GET /gov/sponsors`, `PATCH /gov/sponsors/{id}`, `GET /gov/programs/{id}/sponsors`
  - Budget: `POST/GET /gov/budget-items`, `PATCH /gov/budget-items/{id}`, `GET /gov/programs/{id}/budget`
- **Tranche 3 (content/attendees)**:
  - Sessions: `POST/GET /gov/sessions`, `PATCH /gov/sessions/{id}`, `GET /gov/programs/{id}/sessions`
  - Registrations: `POST/GET /gov/registrations`, `GET /gov/programs/{id}/registrations`
  - Derived views: `GET /gov/programs/{id}/accessibility-status`, `GET /gov/programs/{id}/representation`
- **Tranche 5 (documents)**: `POST/GET /gov/documents`, version history, search
- **Provenance** (Tranche 4 or 5): `GET /gov/history/{ref}`, `GET /gov/provenance/{ref}`

New parent/entity/program-scoped routes should follow the established `/gov/{parent-type}/{parent-id}/{child}` pattern rather than flat top-level paths.

### 13.1 TypeScript SDK drift

Every new HTTP model added to `apps/governance/src/http/models.rs` requires regenerating `sdk/typescript/src/generated/api-types.ts` via `cd sdk/typescript && npm ci && npm run generate-types`. Commit as `chore(sdk): regenerate TypeScript API types` per CLAUDE.md's drift-fix recipe.

---

## 14. Storage model guidance

### 14.1 Pattern (already established)

For each new domain object:
1. Define the struct + store trait in `icn-governance::<module>`.
2. Implement `InMemory<X>Store` in-module for tests.
3. Implement `Sled<X>Store` in `apps/governance/src/manager.rs` following the existing naming pattern: primary keys as `<thing>:{scope}:{id}` (or `<thing>:{id}` when globally scoped), secondary indexes as `<thing>_by_<scope>:{scope_id}:{id}`.
4. Wire into `GovernanceManager`.
5. Use content-addressing for document-like objects (blake3 of canonical body).

### 14.2 Indexes — existing on `main` (verified in `apps/governance/src/manager.rs`)

| Store | Primary key | Secondary / scoped keys |
|-------|-------------|-------------------------|
| `SledActionItemStore` | `action_item:{domain_id}:{item_id}` | — (no secondary index yet) |
| `SledStructureStore` | `structure:{structure_id}` | `structure_by_entity:{entity_id}:{structure_id}`, `role:{role_id}`, `role_by_structure:{structure_id}:{role_id}` |
| `SledActivityStore` | `activity:{activity_id}` | `activity_by_entity:{entity_id}:{activity_id}` |

### 14.3 Indexes — proposed additions (new sled keys must follow the established `<thing>_by_<scope>:...` convention)

| Store | Primary key | Secondary keys | Tranche |
|-------|-------------|----------------|---------|
| `SledActionItemStore` | (existing) | `action_item_by_assignee:{did}:{domain_id}:{item_id}` | 0b (digest) |
| `SledActionItemStore` | (existing) | `action_item_by_parent:{parent_tag}:{parent_id}:{domain_id}:{item_id}` | 1 (views) |
| `SledActionItemStore` | (existing) | `action_item_by_status:{status}:{domain_id}:{item_id}` | 1 (dashboard counts) |
| `SledMeetingStore` | verify on #1543 | verify on #1543 — should include `meeting_by_scope:{scope_tag}:{scope_id}:{scheduled_at}:{meeting_id}` to support upcoming-meetings digest | 0a |
| `SledProgramStore` (new) | `program:{program_id}` | `program_by_entity:{entity_id}:{program_id}`, `program_by_status:{status}:{program_id}`, `program_by_kind:{kind}:{program_id}` | 1 |
| `SledSponsorStore` (new) | `sponsor:{sponsor_id}` | `sponsor_by_program:{program_id}:{sponsor_id}`, `sponsor_by_status:{status}:{sponsor_id}` | 2 |
| `SledBudgetItemStore` (new) | `budget_item:{item_id}` | `budget_item_by_program:{program_id}:{item_id}` | 2 |
| `SledSessionStore` (new) | `session:{session_id}` | `session_by_program:{program_id}:{session_id}`, `session_by_status:{status}:{session_id}` | 3 |
| `SledRegistrationStore` (new) | `registration:{registration_id}` | `registration_by_program:{program_id}:{registration_id}`, `registration_by_actor:{did}:{registration_id}` | 3 |
| `SledDocumentStore` (new) | `document:{doc_hash}` | `document_by_domain_type:{domain_id}:{doc_type}:{timestamp}:{doc_hash}`, `document_by_scope:{scope_tag}:{scope_id}:{doc_hash}` | 5 |

### 14.3 Migration-safe index construction

Any new index added to an existing store must be backfilled on first start. Pattern: add a `pub(crate) const INDEX_VERSION_KEY: &str = "_meta:idx_version:<name>";`. On store init, check the marker; if absent or below current version, walk all rows once and populate.

---

## 15. Testing strategy

### 15.1 Unit / crate-level (required minimum)

Each new module gets standard unit coverage (`cargo test -p icn-governance`, `-p icn-gateway`, specific `apps/governance` tests).

### 15.2 Scenario tests (the load-bearing ones)

These belong in `icn/crates/icn-governance/tests/` or `icn/apps/governance/tests/` and should exercise full loops, not type construction:

| Scenario | Objects exercised | What it proves |
|---------|-------------------|----------------|
| **S1: Backbone → full-team transition** | Entity, Structure(backbone), RoleAssignment, Proposal, Decision, Program, Milestone | Backbone group opens a program draft, venue options logged, recommendation proposal created, full organizer body ratifies via a second proposal in parent domain, milestone flips to VenueLocked |
| **S2: Committee consent decision → action** | Structure(content committee), Proposal(mode=Consent), ActionItemSpec, ActionItem, DigestSummary | Content committee proposes theme change with `action_items_on_accept`; consent threshold met; action items materialize with committee as `parent`; digest reaches content-committee members |
| **S3: Meeting closure continuity** | Meeting, AgendaItem, Proposal, ActionItem, DigestSummary | Meeting held with 3 agenda items; 2 become action items on close, 1 becomes a decision; post-meeting digest summarizes; new organizer queries `/meetings/{id}/summary` and reconstructs the meeting |
| **S4: Registration accessibility feedback** | Program, Registration, AccessibilityStatusView | Attendee registers with childcare + Spanish needs; program's accessibility-status view reflects counts; logistics committee gets a digest item |
| **S5: Sponsor pipeline** | Sponsor, BudgetItem, ActionItem, Program | Sponsor added as prospect; follow-up action item auto-created; sponsor confirmed; budget item income record created; recognition action item auto-created for marketing lead |
| **S6: Cycle closure + handoff** | Program(summit-2026), Program(summit-2027, `parent_program_id=summit-2026`) | Summit 2026 closes; lessons-learned document attached; summit-2027 program seeded with prior-cycle comparison view accessible |
| **S7: Provenance chain** | Document, Meeting, Proposal, ActionItem, SourceRef | Walk from an ActionItem back through meeting → proposal → accepted decision → source doc reference; ends at an external SourceRef |
| **S8: Replay / idempotence** | Proposal, ActionItemSpec materialization | Duplicate close events do not create duplicate action items (already tested in #1532; keep regression-guard) |

### 15.3 Test harness

Use existing `icn-testkit::TestNode` pattern. Scenarios S1–S6 are single-node. S7 should include at least one cross-node gossip replication path if `InstitutionalDocument` is gossiped in Phase 5.

---

## 16. Implementation sequencing

See the companion doc `NYCN-Execution-Tranches.md` for the full merge-order plan. Summary only here:

1. **Phase 0 — land already-open work** (#1543, then push+PR `feat/notification-digests` with stub fixes).
2. **Phase 1 — Program + Milestones + scope-aware digest partitioning** (core cycle container).
3. **Phase 2 — Sponsor + BudgetItem** (funding closure loop).
4. **Phase 3 — Session + Registration + Accessibility** (content + attendee + logistics loops).
5. **Phase 4 — SourceRef + ny-coop-net importer** (migration).
6. **Phase 5 — InstitutionalDocument + document-linked provenance** (phase 5 of the original implementation plan).
7. **Phase 6 — RoleDelegation + OrganizerOnboarding subsystem** (continuity).

Each phase is one or two PRs. Phases 2–3 can parallelize once Phase 1 lands.

---

## 17. Architecture risks and firewalls

### 17.1 Ontology drift

The layered ontology (entities vs. structures vs. activities vs. programs) is the single most fragile decision. Every time a new object is added, evaluate: does it sit cleanly in one layer? If it seems to need pieces of multiple layers, the model is being stretched — stop and re-examine.

### 17.2 Endpoint sprawl before ontology freeze

Phase 0 alone adds ~12 meeting-management endpoints and ~1 digest endpoint. Phase 1 adds program+milestone endpoints. **Do not** start adding sponsor/budget/session/registration endpoints until Phase 1 is reviewed and merged; the derived-view patterns need to stabilize first.

### 17.3 Authority ambiguity

The backbone-group question is political-process ambiguity bleeding into code. Keep the code honest: authority is `RoleAssignment.authority_scope` capability strings + `RoleDelegation` + policy-oracle rules keyed off those capability strings, not hard-coded backbone-group logic.

### 17.4 Organizer-hostile UX

Every new endpoint that returns raw domain objects instead of composed views is technical debt against the "can a new organizer continue the work" test. Pair every canonical endpoint with at least one composed view handler.

### 17.5 Spreadsheet-shaped architecture

Do not model `Sponsor`, `BudgetItem`, or `Registration` fields by copying the current Google Sheets columns. Model them by lifecycle and by the queries they need to serve. `SourceRef` preserves the link back to the spreadsheet; the ICN shape is structured and normalized.

### 17.6 Ceremonial governance

Every new proposal type should either use the decision→action bridge (via `action_items_on_accept`) or connect to a typed execution hook (config change, milestone completion, role assignment). Proposals without downstream effects should be rare and flagged.

### 17.7 Meaning firewall

Keep the firewall: no NYCN-specific strings, no NYCN-specific variants, no `"nycn"` matches in kernel crates. NYCN appears in code only as test fixtures (`StructureId::from_raw("nycn-finance")`, `ActivityId::from_raw("summit-2026")`) and in seed data (CCL charter YAML), not as branches in match arms.

---

## 18. Final technical thesis

The correct technical target is: **implement NYCN as a scoped institutional domain on ICN, with summit-year programs, organizer structures, meetings, decisions, action materialization, digests, provenance, and migration-safe links to existing planning systems.**

As of `37ec91e0`, the ICN governance layer already gives us ~70% of the institutional primitives NYCN needs. The remaining 30% is:

- **merge the two already-built things** (meeting #1543, digest branch),
- **add Program + Milestones** (real cycle container),
- **add Sponsor + BudgetItem + Session + Registration + Accessibility** (operational domains),
- **add SourceRef + importer** (migration),
- **add InstitutionalDocument + RoleDelegation + OnboardingPipeline** (memory + continuity).

None of this requires new kernel concepts. All of it fits inside `icn-governance` + `apps/governance` with gateway-event emission.

If Phase 0–3 lands cleanly, NYCN can instantiate itself on a running ICN node and the decisive test starts to return "yes."
