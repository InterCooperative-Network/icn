# NYCN × ICN Execution Tranches

**Status**: Merge-order + dependency plan, grounded against `main` at `37ec91e0` (2026-04-14)
**Companions**: `NYCN-Repo-Architecture-Spec.md`, `NYCN-Implementation-Matrix.md`

This document sequences the NYCN integration into concrete tranches. Each tranche is one or two PRs with explicit entry conditions, exit conditions, and dependencies. Agents and humans should read this before opening any NYCN-related branch.

---

## 0. Dependency graph (high-level)

```
Tranche 0: Land open work
  ├── 0a: Merge PR #1543 (meetings)
  └── 0b: PR the notification-digest branch (rebased on #1543)
         │
         ▼
Tranche 1: Program + Milestones + scope-aware digests
         │
         ├──► Tranche 2: Sponsor + BudgetItem
         └──► Tranche 3: Session + Registration + Accessibility
                  │
                  ▼
Tranche 4: SourceRef + ny-coop-net importer
         │
         ▼
Tranche 5: InstitutionalDocument + provenance surfacing
         │
         ▼
Tranche 6: RoleDelegation + Organizer Onboarding
```

Tranches 2 and 3 parallelize once 1 lands. Tranche 4 can start research/scripting in parallel with 2 and 3 (docs-only / scripts-only), but its code changes inside `icn-governance` should wait for SourceRef to be reviewable without conflicting with 2/3.

---

## 1. Tranche 0 — Land open work

**Entry condition**: `main` at `37ec91e0`. No other tranche should start until 0 is clear.

### 0a. Merge PR #1543 (Meeting management)

- **Status**: OPEN, mergeable, CI green except non-blocking benchmark regression.
- **Actions**:
  1. Request human review (one reviewer with meaning-firewall + governance context).
  2. Address any human review comments.
  3. If benchmark regression is deemed acceptable, add `perf-regression-ok` label; otherwise profile and mitigate the 18% postcard serialization regression flagged by the bench bot.
  4. Merge via `/push`-gated workflow → `gh pr merge 1543 --squash --delete-branch`.
- **Exit condition**: `main` contains `meeting.rs`, `SledMeetingStore`, meeting HTTP endpoints, `ActionItem.meeting_id`, and `MeetingCreated/Started/Ended` gateway events.

### 0b. PR + ship the notification-digest branch

- **Status**: `feat/notification-digests` exists local+remote, 1 commit ahead of main, no PR yet.
- **Required rework before opening the PR**:
  1. **Rebase on post-#1543 main**. Old base (`37ec91e0`) is before meetings land.
  2. **Unstub `upcoming_meetings`** in `GovernanceManager::generate_digest`. Use `SledMeetingStore::list_for_scope` (or equivalent) filtered to `scheduled_at` within the next 48h, where scope is derived from the requesting DID's role assignments.
  3. **Rename the assignee index** to follow the existing `<thing>_by_<scope>` convention: `action_item_by_assignee:{did}:{domain_id}:{item_id}` (not `ai_idx:assignee:*`). Match how `SledStructureStore` / `SledActivityStore` name their secondary keys on `main`.
  4. **Add assignee-index backfill** to `SledActionItemStore::open` (or equivalent init path). Write a `_meta:idx_version:assignee` marker; if missing or below current version, walk all action items once and populate the secondary index. Add a unit test for the backfill on a pre-populated store.
  5. **Add `GovernanceActionItemCreated` gateway event** — emit from `apps/governance/src/actor.rs` where the decision→action bridge materializes items. This is ~20 LOC and lets the digest architecture (and every future consumer) be event-driven from the start instead of store-polling. Variant fields: `{ item_id, domain_id, linked_proposal, assignee }`. Follow the existing `Governance<Thing><Verb>` naming pattern.
  6. **Keep scope partitioning out of scope for this PR** — it's a v1.1 concern; PR scope is "correct DID-level digest".
  7. Add a scenario test: create proposals with `action_items_on_accept`, close them, verify `GovernanceActionItemCreated` fires, and that the digest reflects the materialized items plus upcoming meetings.
- **PR body must call out**:
  - Depends on #1543 (meeting store).
  - Notes that `SledActionItemStore` assignee index is being introduced under the `<thing>_by_<scope>` naming convention — CI must pass on both empty and pre-populated stores.
  - Introduces `GovernanceActionItemCreated` gateway event (first non-proposal governance-event variant).
- **Exit condition**: PR open, CI green, review approved, merged.

### Tranche 0 risks

- **If #1543 does not merge within a week**, consider whether to base the digest rework on the #1543 branch directly and merge them as a stack — but prefer clean sequential merging.
- **If #1543 hits unexpected review blockers**, the digest work still cannot ship without its meeting-store dependency.

---

## 2. Tranche 1 — Program + Milestones + scope-aware digest partitioning

**Entry condition**: Tranche 0 complete.
**Scope**: Large (Opus-grade design on Program/Milestone; Sonnet-executable rest).
**Branch**: `feat/program-container`.

### 1.1 Add `icn-governance::program`

- Types: `ProgramId`, `ProgramKind`, `ProgramStatus`, `Program`, `Milestone`, `MilestoneId`, `MilestoneType`, `MilestoneStatus`, `CheckRef`, `InMemoryProgramStore`.
- `Program.activity_id` as an optional link; do not require it. Program is the first-class container; Activity is kept for lighter cases.

### 1.2 Add `SledProgramStore` in `apps/governance/src/manager.rs`

- Key prefix `program:<id>`.
- Secondary indexes: `program_idx:parent_entity:<eid>:<pid>`, `program_idx:status:<status>:<pid>`, `program_idx:kind:<kind>:<pid>`.
- Backfill pattern with `_meta:idx_version:program:<name>` markers.

### 1.3 HTTP surface

- `POST/GET /gov/programs`
- `GET /gov/programs/{id}`
- `POST /gov/programs/{id}/milestones`
- `PATCH /gov/programs/{id}/milestones/{mid}`
- `POST /gov/programs/{id}/close`
- `GET /gov/programs/{id}/dashboard` — composes program + milestones + action-item counts by parent + open-decision count

### 1.4 Gateway events

- `GovernanceProgramOpened`, `GovernanceProgramClosed`, `GovernanceMilestoneCompleted`, `GovernanceMilestoneBlocked`. Follow the existing `Governance<Thing><Verb>` naming pattern on `GatewayEvent`.
- Also emit `GovernanceStructureCreated`, `GovernanceRoleAssigned`, `GovernanceActivityCreated`, `GovernanceActivityStatusChanged` here if Tranche 0b has not already added them — these were assumed present in earlier planning but are **not** on `main`.

### 1.5 Scope-partitioned digest (v1.1 of digest)

- Extend `generate_digest(did, scope: Option<DigestScope>)`.
- `DigestScope::Structure(sid) | Program(pid) | Entity(eid) | All`.
- Handler: `GET /gov/digest?scope=structure:nycn-finance`.

### 1.6 MyScope view

- `GET /gov/me/scopes` — enumerate current user's `RoleAssignment`s grouped by scope, with authority type and time-bounds. Trivial but unblocks UI.

### 1.7 Scenario tests

- S1 (backbone → full-team transition) with Program/Milestone.
- Milestone transition gated on required checks.
- Scope-partitioned digest returns disjoint results for two scopes the same user is in.

### 1.8 Exit condition

PR merged; NYCN charter bootstrap can create `summit-2026` as a `Program { kind: AnnualSummit, parent_entity_id: "nycn-organizers" }` with `StrategyLocked`, `VenueLocked`, `BudgetLocked`, `PublicLaunchReady`, `EventReady`, `ClosureComplete` milestones.

---

## 3. Tranche 2 — Sponsor + BudgetItem (funding closure)

**Entry condition**: Tranche 1 merged.
**Scope**: Medium. Parallelizable with Tranche 3.
**Branch**: `feat/sponsor-budget`.

### 2.1 Add `icn-governance::sponsor`

- `Sponsor { id, entity_ref: Option<String>, program_id, relationship_owner_did, status, tier, amount, follow_up_at, benefits_status, source_refs, notes }`.
- `SponsorStatus { Prospect, Warm, Asked, Negotiating, Confirmed, Declined, Fulfilled }`.
- Lifecycle validation: disallow illegal transitions (e.g., Declined → Confirmed is only via a new Sponsor record).

### 2.2 Add `icn-governance::budget_item`

- `BudgetItem { id, program_id, category, amount, direction: Income|Expense|Commitment|Forecast, status, owner_did, linked_party, due_at, source_refs }`.
- No double-entry bookkeeping here. Budget items are **program-level operational planning records**; treasury remains in `icn-ledger`. If/when a budget item is realized financially, it references a ledger journal entry by ID — but BudgetItem itself does not post ledger entries.

### 2.3 Sled stores + HTTP

- `SledSponsorStore`, `SledBudgetItemStore` in `apps/governance/src/manager.rs`.
- Endpoints per §13 of the architecture spec.

### 2.4 Sponsor → Budget + Action auto-materialization

- On `Sponsor.status → Confirmed`: emit `SponsorStatusChanged`, auto-create a `BudgetItem { direction: Income, status: Forecast, amount: sponsor.amount, linked_party: sponsor.entity_ref }`, and auto-create a recognition `ActionItem` with `parent = InstitutionalParent::Activity(program_activity_id)` assigned to the marketing-committee lead role holder.
- Use a deterministic materialization pattern (keyed by sponsor ID + event) so retries are idempotent.

### 2.5 Gateway events

- `SponsorStatusChanged`, `BudgetItemCreated`, `BudgetItemUpdated`.

### 2.6 Scenario tests

- S5 (sponsor pipeline → budget → action).
- Idempotence: re-emitting `SponsorStatusChanged(Confirmed)` does not duplicate the budget item or action item.

### 2.7 Exit condition

PR merged; NYCN can track sponsor prospects and confirmed funders with automatic budget and recognition follow-ups.

---

## 4. Tranche 3 — Session + Registration + Accessibility

**Entry condition**: Tranche 1 merged. Parallelizable with Tranche 2.
**Scope**: Large.
**Branch**: `feat/session-registration`.

### 3.1 Add `icn-governance::session`

- `Session { id, program_id, title, track, format, speaker_dids, status, language_needs, publication_state, representation: RepresentationMetadata, source_refs }`.
- `SessionStatus { Idea, Invited, Confirmed, Published, Cancelled }`.
- `RepresentationMetadata { sector, geography, language, cooperative_type }` — all `Option<String>`, used for representation dashboards.

### 3.2 Add `icn-governance::registration`

- `Registration { id, program_id, actor_did, ticket_type, payment_state, dietary_needs, childcare_needs, language_needs, accessibility_notes, volunteer_interest, organizer_interest, checkin_state, source_refs }`.
- `ChildcareNeed { None, RequestedOnSite, RequestedNearSite, PartialCoverage }`.

### 3.3 Add `icn-governance::accessibility`

- `VenueAccessibility { transit_score, hotel_walkability_score, childcare_fit, interpretation_supported, sobriety_friendly, navigation_clarity_score, restroom_inclusion_notes, family_support_notes, environmental_quality_notes }`.
- `AccessibilityScore { Poor, Fair, Good, Excellent, Unknown }`.
- Embed on `Program.metadata.venue_accessibility` (serde passthrough) or as a first-class optional field on Program — prefer first-class.

### 3.4 Sled stores + HTTP

- `SledSessionStore`, `SledRegistrationStore`.
- Endpoints per §13 of architecture spec.
- `GET /gov/programs/{id}/accessibility-status` — composes venue accessibility + aggregated registration accessibility needs into a single status struct.
- `GET /gov/programs/{id}/representation` — aggregates `Session.representation` fields into buckets.

### 3.5 Gateway events

- `SessionPublished`, `SessionStatusChanged`, `RegistrationConfirmed`, `RegistrationCheckedIn`.

### 3.6 Scenario tests

- S4 (registration accessibility feedback).
- Representation bucket aggregation returns stable counts under rapid session status changes.

### 3.7 Exit condition

PR merged; attendee and content dashboards populate; logistics committee can query accessibility status.

---

## 5. Tranche 4 — SourceRef + ny-coop-net importer

**Entry condition**: Tranches 2 + 3 merged (SourceRef is attached to sponsor/budget/session/registration records from day 1 going forward, but the importer populates historical data).
**Scope**: Medium Rust + scripts.
**Branches**: `feat/source-ref` (docs-touching Rust), `chore/nycn-importer` (binary/script).

### 4.1 Add `icn-governance::source_ref`

- `SourceSystem { GoogleDoc, GoogleSheet, DriveFile, LegacyRepo, MailingList, NyCoopNetRow, ManualEntry, Other(String) }`.
- `ProvenanceType { Linked, Mirrored, Imported, Derived }`.
- `SourceRef` struct as specified.
- Attach `source_refs: Vec<SourceRef>` field (`#[serde(default)]`) to: `Program`, `Sponsor`, `BudgetItem`, `Session`, `Registration`, `InstitutionalDocument` (when Tranche 5 lands), and any entity/participation records created via import.

### 4.2 `bins/icn-nycn-import` (or `scripts/nycn-importer/`)

- Reads ny-coop-net Postgres (connection string via env).
- Maps tables → canonical ICN objects:
  - `organizations` → `Entity { type: Cooperative | Community }` or `Participation { type: MemberOrg }` depending on relation
  - `members` → `Participation { type: Member }` linked to entities
  - `events` / `summits` → `Activity` or `Program` (summit years as Program, misc events as Activity)
  - `sponsors` → `Sponsor` with status mapped from legacy state
  - `registrations` → `Registration`
- **All imported records carry `SourceRef { source_system: NyCoopNetRow, external_identifier: "<table>:<pk>", provenance_type: Imported }`.**
- Writes via the gateway HTTP API — never directly to sled — so validation is exercised identically to normal create flows.
- Idempotent: re-running must not duplicate. Dedup by (source_system, external_identifier).

### 4.3 Scenario tests

- Import + re-import does not duplicate.
- Random-sample verification: pick 10 ny-coop-net rows, assert each maps to an ICN record with matching `SourceRef`.
- Provenance round-trip: `GET /gov/provenance/<id>` returns the source ref.

### 4.4 Exit condition

- NYCN can be bootstrapped on a fresh ICN node by running the importer. Historical sponsor pipeline, summit attendees, and member orgs are present with provenance links to ny-coop-net.

---

## 6. Tranche 5 — InstitutionalDocument + cross-object provenance

**Entry condition**: Tranche 4 merged.
**Scope**: Large. Design-Opus, implementation-Sonnet.
**Branch**: `feat/institutional-documents`.

### 6.1 Add `icn-governance::document`

- Content-addressed (`id = blake3(body)`).
- `DocumentType { MeetingAgenda, MeetingMinutes, CommitteeReport, Policy, Evaluation, General }`.
- Linked-to-scope via `InstitutionalParent` OR linked to a specific meeting/proposal instance.
- Version lineage via `parent_doc`.
- `attachments: Vec<AttachmentRef>` (each attachment is content-addressed separately).

### 6.2 Gossip integration

- Topic: `governance:{domain_id}:documents`.
- Published as gossip entries; content-addressed so replication is automatic via existing protocol.
- Sled indexes: `doc:<domain>:<doc_id>`, `doc_idx:<domain>:<type>:<timestamp>`, `doc_link:<scope_tag>:<scope_id>:<doc_id>`.

### 6.3 HTTP surface

- `POST /gov/documents`, `GET /gov/documents/{id}`, version-history list, search.

### 6.4 Meeting notes linkage

- `Meeting.notes_doc: Option<DocumentId>` (add field now, defaulting to `None`, on the meeting model so post-meeting summaries can attach the minutes document).

### 6.5 Provenance surfacing

- `GET /gov/history/{ref}` — walks the provenance chain. Given an `ActionItem`, returns: action → meeting → agenda item → decision (proposal) → accepted tally → originating documents.
- `GET /gov/provenance/{ref}` — raw provenance edges only.

### 6.6 Scenario tests

- S7 (provenance chain).
- Cross-node gossip: document created on node A is retrievable and hash-verifiable on node B.
- Version lineage: v1 → v2 → v3 queryable by parent_doc chain.

### 6.7 Exit condition

- A new organizer can walk from their action item inbox back through meetings, decisions, and supporting documents to understand the full lineage of any obligation.

---

## 7. Tranche 6 — RoleDelegation + Organizer Onboarding

**Entry condition**: Tranche 5 merged.
**Scope**: Medium.
**Branch**: `feat/role-delegation-onboarding`.

### 7.1 Add `icn-governance::role_delegation`

- Distinct from vote-delegation (`icn-governance::delegation`).
- `RoleDelegation { id, grantor_did, grantee_did, scope_ref, capabilities, starts_at, ends_at, reason, revoked_at }`.
- Sled store + HTTP (`POST /gov/role-delegations`, `POST /gov/role-delegations/{id}/revoke`, `GET /gov/me/delegations`).
- Active-scope resolution in the gateway includes active delegations within time window.

### 7.2 Add `icn-governance::onboarding`

- `OnboardingPipelineEntry { id, actor_did, pipeline_type (interest | orientation | role-assigned | first-task | continuity), status, created_at, promoted_at, structure_interest: Option<StructureId>, notes }`.
- HTTP: `POST /gov/onboarding`, `PATCH /gov/onboarding/{id}/advance`, `GET /gov/onboarding`.
- View: `OrganizerOnboardingView` — pipeline state + next actions per entry.

### 7.3 Scenario tests

- Delegation grant → grantee acts in scope → revocation → grantee loses capability.
- Onboarding pipeline traversal: interest → role-assigned → first-task completion.
- S6 (cycle closure + handoff): closed program's role assignments do not block new organizers onboarding to the next cycle's program.

### 7.4 Exit condition

- Organizers can be onboarded and delegated to with full auditability; annual cycle handoff leaves clean state.

---

## 8. Docs and charter work (parallel / continuous)

These are **not** code tranches but must progress alongside:

### 8.1 After Tranche 0 lands

- Update `docs/strategy/NYCN-Implementation-Plan.md`: Phase 3 (meetings) → ✅, Phase 4 (digests) → ✅ for v1; add v1.1 (scope partitioning) as a note.
- Update `docs/INDEX.md` to reference this architecture spec and matrix docs.

### 8.2 After Tranche 1 lands

- Revise `docs/strategy/NYCN-Institutional-Design.md`:
  - Replace the committee-as-Community entity-tree diagram with the Structure-based topology.
  - Add Program wrapping.
  - Note that the older entity tree is superseded.
- Add `docs/strategy/NYCN-Bootstrap-Runbook.md` (does not currently exist despite prior-session claim).

### 8.3 After Tranche 4 lands

- Add `docs/strategy/NYCN-Schema-Mapping.md` documenting the ny-coop-net → ICN mapping actually implemented by the importer.

### 8.4 NYCN Charter YAML

- `docs/strategy/NYCN-Charter-Draft.yaml` exists. When Tranche 1 lands, update the charter YAML to declare programs and milestones if the CCL schema supports them. If not, open a CCL extension proposal.

---

## 9. Rollback and risk mitigation

### 9.1 Tranche rollback rule

Each tranche is independently revertable via `gh pr revert`. Specifically:

- Reverting Tranche 0 restores digest-less, meeting-less state — no data loss.
- Reverting Tranche 1 requires first reverting all later tranches that reference Program. Phase gate: if Tranche 1 has problems, no later tranche should have merged.
- Tranches 2 and 3 are independently revertable from each other.
- Tranche 4 importer bugs do not require code revert — `SourceRef`-tagged records can be bulk-deleted by source-system filter.
- Tranche 5 documents are content-addressed; reverting the code still leaves gossiped documents on peer nodes. Plan a clean-shutdown migration if this becomes a live concern.

### 9.2 Load-bearing risks

| Risk | Mitigation |
|------|------------|
| Program/Activity overlap confusion | Clear docstring separation; both primitives kept but linter/docs explain when to use which |
| Authority ambiguity (backbone vs full group) | Model via `RoleAssignment.authority_type` + PolicyOracle rules, not hardcoded logic |
| Endpoint sprawl | No sponsor/budget/session/registration endpoints before Tranche 1 lands |
| Spreadsheet-shaped architecture | All imported data carries `SourceRef`; canonical shape is ICN-native, not sheet-derived |
| Meaning firewall breach | No NYCN-specific strings in kernel crates; only in seed data and test fixtures |
| Digest staleness / index drift | Every new secondary index includes backfill-on-init with version marker |
| Migration double-import | Idempotent import keyed on (source_system, external_identifier) |

---

## 10. Agent assignments

| Tranche | Suggested agent(s) | Reason |
|---------|-------------------|--------|
| 0a (merge #1543) | human review + `icn-rust-core` for any review fixes | meaning-firewall sensitive |
| 0b (digest rework) | `icn-rust-core` | index backfill + store composition |
| 1 (Program + Milestones) | `icn-governance-advisor` design + `icn-rust-core` impl | governance state machine correctness |
| 2 (Sponsor + Budget) | `icn-governance-advisor` + `icn-economics-advisor` on the budget/ledger boundary | ledger boundary risk |
| 3 (Session + Registration + Accessibility) | `icn-rust-core` with `icn-governance-advisor` review | operational domains |
| 4 (SourceRef + importer) | `icn-rust-core` + human operator for the importer binary | touches external DB |
| 5 (Documents + gossip) | `icn-gossip-net` + `icn-rust-core` + `icn-governance-advisor` | gossip integration |
| 6 (RoleDelegation + Onboarding) | `icn-identity-iam-advisor` + `icn-governance-advisor` | IAM + governance crossover |

Every tranche also gets one pass by `icn-invariants-guardian` before merge.

---

## 11. Success criteria (the decisive test)

> Can a new organizer enter the system mid-cycle, switch into the right scope, see the summit's current phase, understand what was decided, know what is blocked, receive their obligations, trace why they exist, and continue the work without needing private oral history?

| Tranche | What this enables toward the decisive test |
|---------|-------------------------------------------|
| 0 | Meetings recorded; overdue work surfaces in digest |
| 1 | New organizer can see summit's current phase (milestones) + their scope |
| 2 | New organizer can see funding state and follow-ups without asking |
| 3 | New organizer can see content + attendees + accessibility status |
| 4 | Historical context from ny-coop-net is present with provenance |
| 5 | New organizer can walk back from any obligation to the document that originated it |
| 6 | New organizer can be onboarded with delegated scope without oral handoff |

When Tranche 6 is merged and a migration has happened, the decisive test returns "yes" for the first time.

---

## 12. Start line

- **Immediate next action**: Push `feat/notification-digests` rework is blocked until #1543 is reviewed + merged. So **the very next action is: request human review on PR #1543**.
- Second action: once #1543 is merged, rebase `feat/notification-digests` on main, apply the digest-rework list in §2.0b, push, open PR.
- Third action: design review for Tranche 1 (`Program` module) with `icn-governance-advisor` and `icn-invariants-guardian` on the proposed module shape before opening a branch.

Do not start Tranche 2+ branches until Tranche 1 is under review.
