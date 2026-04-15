# NYCN × ICN Implementation Matrix

**Status**: Execution ledger, grounded against `main` at `37ec91e0` (2026-04-14)
**Companions**: `NYCN-Repo-Architecture-Spec.md` (architectural map), `NYCN-Execution-Tranches.md` (merge-order plan)
**Working instruction**: Treat the architecture spec as the map and this matrix as the execution ledger. Before writing code, reconcile any disagreement between them and current repo state — reality wins.

---

## Legend

**Lifecycle**
- ✅ **exists** — canonical state lives on `main`, tests passing
- 🚧 **partial** — work exists but not on `main` (see `Source` column)
- ⚠️ **adjacent** — something close exists but does not fully cover the need
- ❌ **missing** — no code, no branch

**Source** (mechanical state)
- `main` — merged into `main`, covered by CI on `main`
- `open_pr` — on an open pull request targeting `main`
- `branch_only` — local/remote branch with no PR yet opened
- `spec_only` — described in docs, no code
- `external` — lives in a non-governance crate (e.g., `icn-entity`, `icn-ledger`)

**Touch points** (expected implementation surfaces per row)
- `D` domain module in `icn-governance::<name>`
- `S` sled store in `apps/governance/src/manager.rs`
- `M` `GovernanceManager` wiring
- `Hm` HTTP models in `apps/governance/src/http/models.rs`
- `Hh` HTTP handlers/routes in `apps/governance/src/http/{handlers,configure}.rs`
- `E` gateway event variant(s) in `icn-gateway::events`
- `T` unit + scenario tests
- `SDK` regenerate `sdk/typescript/src/generated/api-types.ts`

---

## 1. Canonical objects

| Object | Status | Source | Code location (existing or proposed) | Touch points (remaining) | Next move | Blockers / deps |
|--------|--------|--------|--------------------------------------|-------------------------|-----------|-----------------|
| Entity (Federation / Cooperative / Community / Individual) | ✅ | `external` | `icn-entity` | — | Use as-is for NYCN, nycn-organizers, member co-ops | — |
| Did / Actor | ✅ | `external` | `icn-identity` | — | — | — |
| Governance domain | ✅ | `main` | `icn-governance::domain` | — | One domain per entity + optionally per structure | — |
| Proposal + Vote + Tally + Profile | ✅ | `main` | `icn-governance::{proposal,vote,tally,profile}` | — | — | — |
| Consent mode (`DecisionMode::Consent`, `max_objections`) | ✅ | `main` | `icn-governance::{config,profile}` | — | — | #1533 merged |
| ActionItem **core** (type, sled store, HTTP) | ✅ | `main` | `icn-governance::action_item`, `apps/governance/src/manager.rs::SledActionItemStore` | — | — | — |
| ActionItem `parent: InstitutionalParent` field | ✅ | `main` | `icn-governance::action_item::ActionItem.parent` | — | — | #1540 merged |
| ActionItem `linked_proposal` + decision→action bridge | ✅ | `main` | `icn-governance::proposal::ActionItemSpec`, `apps/governance/src/actor.rs` | — | — | #1532 merged |
| ActionItem `meeting_id: Option<MeetingId>` field | 🚧 | `open_pr` (#1543) | `icn-governance::action_item` (on meeting branch) | T (on land) | Merge #1543 | — |
| ActionItem **assignee secondary index** (proposed key: `action_item_by_assignee:{did}:{domain_id}:{item_id}`) | 🚧 | `branch_only` | `apps/governance/src/manager.rs::SledActionItemStore` (on `feat/notification-digests`) | S, T (backfill test) | Open PR; align key name to existing `_by_` convention; add backfill-on-init with `_meta:idx_version:assignee` marker | #1543 merge first |
| ActionItem **assignee-index backfill** for pre-existing items | ❌ | `spec_only` | same file | S, T | Add on same PR as above | — |
| ActionItem secondary indexes (`action_item_by_parent:*`, `action_item_by_status:*`) | ❌ | `spec_only` | same file | S, T | Add alongside parent-scoped / status-scoped queries (Tranche 1 views) | Program views |
| ActionItemSpec (proposal materialization) | ✅ | `main` | `icn-governance::proposal::ActionItemSpec` | — | — | #1532 |
| Structure (Committee / WorkingGroup / Team / Office) | ✅ | `main` | `icn-governance::structure`, `apps/governance/src/manager.rs::SledStructureStore` | — | Seed NYCN committees via charter bootstrap | #1540 |
| RoleAssignment (`structure_id`, `person_did`, `role: String`, `authority_scope: Vec<String>`, start/end, `assigned_by_decision`) | ✅ | `main` | `icn-governance::structure::RoleAssignment` | Hh (`GET /me/scopes` handler missing) | Add `GET /me/scopes` in Tranche 1. Authority is capability-string based (`authority_scope`), not a typed enum. | — |
| Activity (Event / Program / Project / Initiative) | ✅ | `main` | `icn-governance::activity`, `apps/governance/src/manager.rs::SledActivityStore` | — | Keep; wrap into Program for stage-gated cycles | #1540 |
| InstitutionalParent (polymorphic attachment) | ✅ | `main` | `icn-governance::parent` | — | Use for every new operational object | #1540 |
| Charter + CharterStore | ✅ | `main` | `icn-governance::{charter,charter_store}` | — | Use NYCN charter YAML draft | — |
| Steward (SDIS) | ✅ | `main` | `icn-governance::{steward,steward_store}` | — | — | — |
| Delegation (vote-level) | ✅ | `main` | `icn-governance::delegation` | — | Distinct from operational RoleDelegation (Phase 6) | — |
| Gateway events — governance subset (`GovernanceDomainCreated`, `GovernanceProposalCreated`, `GovernanceProposalOpened`, `GovernanceProposalClosed { outcome, ... }`, `GovernanceVoteCast`) | ✅ | `main` | `icn-gateway::events` (verified) | — | — | — |
| Gateway events for action-item / structure / activity lifecycle | ❌ | `spec_only` | `icn-gateway::events` | E, wire emission in `apps/governance/src/actor.rs` | Add at least `GovernanceActionItemCreated` in Tranche 0b so digest architecture is event-driven; others per tranche | — |
| **Meeting** + AgendaItem + attendance + SledMeetingStore + 9 HTTP endpoints | 🚧 | `open_pr` (#1543) | `icn-governance::meeting`, `apps/governance/src/manager.rs::SledMeetingStore`, handlers | T (on land) | Human review + merge #1543 | Benchmark regression tag; no blocking human review yet |
| Meeting-lifecycle events (`MeetingCreated/Started/Ended`) | 🚧 | `open_pr` (#1543) | `icn-gateway::events` | — | Lands with #1543 | — |
| **DigestSummary** + `GET /digest` + `list_by_assignee` | 🚧 | `branch_only` | `apps/governance/src/manager.rs`, handlers | Hh, T | PR it after #1543 merges; unstub `upcoming_meetings` against meeting store | #1543 |
| **Program** + ProgramStatus + ProgramKind | ❌ | `spec_only` | proposed `icn-governance::program` | D, S, M, Hm, Hh, E, T, SDK | Tranche 1 | — |
| **Milestone** + MilestoneType + required_checks | ❌ | `spec_only` | proposed `icn-governance::program::Milestone` | D, Hm, Hh, E, T | Same PR as Program | — |
| **CheckRef** (milestone predicates) | ❌ | `spec_only` | proposed `icn-governance::program::checks` | D, T | Same PR as Program; enum + evaluator | — |
| Scope-partitioned digest (`DigestScope::{Structure,Program,Entity,All}`) | ❌ | `spec_only` | `apps/governance/src/manager.rs` | Hm, Hh, T | Tranche 1 (after base digest lands) | — |
| MyScope view (`GET /me/scopes`) | ❌ | `spec_only` | `apps/governance/src/http/handlers.rs` | Hh, T, SDK | Tranche 1 (trivial) | — |
| MyWork view (`GET /me/work`) | ❌ | `spec_only` | `apps/governance/src/http/handlers.rs` | Hh, T, SDK | Tranche 1 | — |
| ProgramDashboardView | ❌ | `spec_only` | `apps/governance/src/http/views.rs` (new file) | Hh, T, SDK | Tranche 1 | Program |
| **Participation** (non-authority: sponsor-contact / speaker / attendee / volunteer) | ❌ | `spec_only` | proposed `icn-governance::participation` OR `icn-entity` extension | D, S, M, Hm, Hh, T, SDK | Decide location before Tranche 2 | — |
| **Sponsor** | ❌ | `spec_only` | proposed `icn-governance::sponsor` | D, S, M, Hm, Hh, E, T, SDK | Tranche 2 | Program |
| **BudgetItem** | ❌ | `spec_only` | proposed `icn-governance::budget_item` | D, S, M, Hm, Hh, E, T, SDK | Tranche 2 | Program |
| **Session** | ❌ | `spec_only` | proposed `icn-governance::session` | D, S, M, Hm, Hh, E, T, SDK | Tranche 3 | Program |
| **Registration** | ❌ | `spec_only` | proposed `icn-governance::registration` | D, S, M, Hm, Hh, E, T, SDK | Tranche 3 | Program |
| **VenueAccessibility + AccessibilityScore** | ❌ | `spec_only` | proposed `icn-governance::accessibility` | D, Hm, T, SDK | Tranche 3 (ship with Registration) | — |
| RepresentationMetadata | ❌ | `spec_only` | field on `Session` | D, T, SDK | Tranche 3 | Session |
| **SourceRef** (Linked/Mirrored/Imported/Derived) | ❌ | `spec_only` | proposed `icn-governance::source_ref` | D, Hm, T, SDK (field on Program/Sponsor/etc.) | Tranche 4 | Target objects must exist |
| **ny-coop-net importer** | ❌ | `spec_only` | proposed `bins/icn-nycn-import` or `scripts/nycn-importer/` | binary/script, T | Tranche 4 | SourceRef + target objects |
| **InstitutionalDocument** (content-addressed, versioned) | ❌ | `spec_only` | proposed `icn-governance::document` | D, S, M, Hm, Hh, E, T, SDK; gossip topic | Tranche 5 | — |
| AttachmentRef | ❌ | `spec_only` | inside `document` module | D, T | Ships with Document | — |
| Meeting.notes_doc field | ❌ | `spec_only` | `icn-governance::meeting` | D, Hm, T | Ships with Document (Tranche 5), add field now as `Option<DocumentId>` default None when #1543 lands OR as part of Tranche 5 | #1543 first |
| **RoleDelegation** (operational, distinct from vote delegation) | ❌ | `spec_only` | proposed `icn-governance::role_delegation` | D, S, M, Hm, Hh, T, SDK | Tranche 6 | — |
| **OrganizerOnboardingPipeline** | ❌ | `spec_only` | proposed `icn-governance::onboarding` | D, S, M, Hm, Hh, T, SDK | Tranche 6 | — |
| CycleComparisonView | ❌ | `spec_only` | `apps/governance/src/http/views.rs` | Hh, T, SDK | Tranche 6 or later | Program.parent_program_id |
| DigestPacket (persisted digest for delivery history) | ❌ | `spec_only` | later | — | Defer | — |

---

## 2. Bounded domains → module map

| Domain | Module (existing/proposed) | Status | Source |
|--------|---------------------------|--------|--------|
| Identity & participation (authority) | `icn-identity`, `icn-entity`, `icn-governance::{membership, structure::RoleAssignment}` | ✅ | `main` / `external` |
| Identity & participation (non-authority) | proposed `icn-governance::participation` OR `icn-entity::participation` | ❌ | `spec_only` |
| Institutional structure | `icn-governance::structure` | ✅ | `main` |
| Governance | `icn-governance::{proposal,vote,tally,profile,config,discussion,appeal,amendment}` + `apps/governance/src/actor.rs` | ✅ | `main` |
| Program / cycle | proposed `icn-governance::program` | ❌ | `spec_only` |
| Meeting | `icn-governance::meeting` | 🚧 | `open_pr` #1543 |
| Work execution | `icn-governance::action_item` + bridge | ✅ | `main` |
| Communications / digests (DID-level) | `apps/governance::manager::DigestSummary` | 🚧 | `branch_only` |
| Communications / digests (scope-partitioned, pre-/post-meeting packets) | proposed extensions | ❌ | `spec_only` |
| Funding / treasury | `icn-ledger` | ✅ | `external` |
| Funding / sponsor pipeline | proposed `icn-governance::sponsor` | ❌ | `spec_only` |
| Funding / program budget | proposed `icn-governance::budget_item` | ❌ | `spec_only` |
| Content / session | proposed `icn-governance::session` | ❌ | `spec_only` |
| Registration / attendee | proposed `icn-governance::registration` | ❌ | `spec_only` |
| Archive / provenance (decisions) | `icn-governance::proof` | ✅ | `main` |
| Archive / provenance (source refs) | proposed `icn-governance::source_ref` | ❌ | `spec_only` |
| Archive / provenance (documents) | proposed `icn-governance::document` | ❌ | `spec_only` |

---

## 3. Authority & scope gaps

| Gap | Proposal | Status | Source |
|-----|----------|--------|--------|
| No API to enumerate current user's scopes | `GET /me/scopes` handler | ❌ | `spec_only` |
| Gateway JWT does not carry explicit active-scope | `X-ICN-Active-Scope` header or JWT claim; server-side resolution against `RoleAssignment` | ❌ | `spec_only` |
| Operational role-delegation record (distinct from vote delegation) | New `icn-governance::role_delegation` | ❌ | `spec_only` (Tranche 6) |
| Backbone-group authority (political debt) | `Structure { kind: Committee }` on `nycn-organizers` + PolicyOracle rule requiring parent-domain ratification for certain proposal types | ⚠️ | Structure exists (`main`); oracle rule `spec_only` |
| Ratification flow (committee → network) | Proposal-type metadata + companion proposal auto-opened in parent domain | ❌ | `spec_only` |

---

## 4. Storage indexes

| Store | Index | Status | Source | Required for |
|-------|-------|--------|--------|--------------|
**On `main`** (verified in `apps/governance/src/manager.rs`):

| Store | Keys | Status | Source |
|-------|------|--------|--------|
| `SledActionItemStore` | `action_item:{domain_id}:{item_id}` | ✅ | `main` |
| `SledStructureStore` | `structure:{structure_id}`, `structure_by_entity:{entity_id}:{structure_id}`, `role:{role_id}`, `role_by_structure:{structure_id}:{role_id}` | ✅ | `main` |
| `SledActivityStore` | `activity:{activity_id}`, `activity_by_entity:{entity_id}:{activity_id}` | ✅ | `main` |

**Needed** (new keys follow the existing `<thing>_by_<scope>:...` convention, not a fabricated `_idx:` prefix):

| Store | Key | Status | Source | Required for |
|-------|-----|--------|--------|--------------|
| `SledActionItemStore` | `action_item_by_assignee:{did}:{domain_id}:{item_id}` | 🚧 | `branch_only` | Digest performance |
| `SledActionItemStore` | assignee-index backfill on init | ❌ | `spec_only` | Correctness for pre-existing items |
| `SledActionItemStore` | `action_item_by_parent:{parent_tag}:{parent_id}:{domain_id}:{item_id}` | ❌ | `spec_only` | Scope-scoped queries |
| `SledActionItemStore` | `action_item_by_status:{status}:{domain_id}:{item_id}` | ❌ | `spec_only` | Dashboard counts |
| `SledMeetingStore` | `meeting:{meeting_id}`, `meeting_by_scope:{scope_tag}:{scope_id}:{scheduled_at}:{meeting_id}` | ⚠️ | verify on #1543 | Upcoming meetings within 48h for digest |
| `SledProgramStore` | `program:{id}`, `program_by_entity:{entity_id}:{id}`, `program_by_status:{status}:{id}`, `program_by_kind:{kind}:{id}` | ❌ | `spec_only` | All program views |
| `SledSponsorStore` | `sponsor:{id}`, `sponsor_by_program:{program_id}:{id}`, `sponsor_by_status:{status}:{id}` | ❌ | `spec_only` | Sponsor pipeline view |
| `SledBudgetItemStore` | `budget_item:{id}`, `budget_item_by_program:{program_id}:{id}` | ❌ | `spec_only` | Program budget view |
| `SledSessionStore` | `session:{id}`, `session_by_program:{program_id}:{id}`, `session_by_status:{status}:{id}` | ❌ | `spec_only` | Program content view |
| `SledRegistrationStore` | `registration:{id}`, `registration_by_program:{program_id}:{id}`, `registration_by_actor:{did}:{id}` | ❌ | `spec_only` | Attendee counts, user's registrations |
| `SledDocumentStore` | `document:{doc_hash}`, `document_by_domain_type:{domain_id}:{type}:{timestamp}:{doc_hash}`, `document_by_scope:{scope_tag}:{scope_id}:{doc_hash}` | ❌ | `spec_only` | Phase 5 document queries |

**Migration-safe index rule**: every new index gets a `_meta:idx_version:<name>` marker. On store init, if absent or below current version, walk rows and populate. Test coverage must include both "empty store" and "pre-populated store".

---

## 5. Gateway events

Current `GatewayEvent` governance-related variants on `main` (verified in `icn/crates/icn-gateway/src/events.rs`):

| Event variant | Status | Source |
|---------------|--------|--------|
| `GovernanceDomainCreated` | ✅ | `main` |
| `GovernanceProposalCreated` | ✅ | `main` |
| `GovernanceProposalOpened` | ✅ | `main` |
| `GovernanceProposalClosed { outcome: String, ... }` | ✅ | `main` |
| `GovernanceVoteCast` | ✅ | `main` |

Proposed additions — **none of these exist on `main` yet**:

| Event variant | Status | Source | Tranche |
|---------------|--------|--------|---------|
| `GovernanceActionItemCreated` (emitted when decision→action bridge materializes items) | ❌ | `spec_only` | **0b (add alongside digest PR so digest architecture is event-driven from day 1)** |
| `GovernanceActionItemUpdated/Completed` | ❌ | `spec_only` | 1 |
| `GovernanceStructureCreated`, `GovernanceRoleAssigned` | ❌ | `spec_only` | 1 |
| `GovernanceActivityCreated`, `GovernanceActivityStatusChanged` | ❌ | `spec_only` | 1 |
| `GovernanceMeetingCreated/Started/Ended` | 🚧 | `open_pr` #1543 | 0a |
| `GovernanceProgramOpened/Closed`, `GovernanceMilestoneCompleted/Blocked` | ❌ | `spec_only` | 1 |
| `GovernanceSponsorStatusChanged` | ❌ | `spec_only` | 2 |
| `GovernanceBudgetItemCreated/Updated` | ❌ | `spec_only` | 2 |
| `GovernanceSessionPublished`, `GovernanceSessionStatusChanged` | ❌ | `spec_only` | 3 |
| `GovernanceRegistrationConfirmed`, `GovernanceRegistrationCheckedIn` | ❌ | `spec_only` | 3 |
| `GovernanceDocumentCreated/Updated` | ❌ | `spec_only` | 5 |

**Naming convention**: follow the existing `Governance<Thing><Verb>` pattern on `GatewayEvent`. Do not invent a parallel naming scheme.

---

## 6. HTTP API surface

All governance routes are mounted under the `/gov` scope (see `icn/crates/icn-gateway/src/server.rs:2008`).

### 6.1 On `main` (verified in `apps/governance/src/http/configure.rs`)

- **Domains**: `POST/GET /gov/domains`, `GET /gov/domains/{domain_id}`, `POST/DELETE /gov/domains/{domain_id}/members`
- **Proposals**: `POST/GET /gov/proposals`, `GET /gov/proposals/{id}`, `POST /gov/proposals/{id}/{open,close,vote}`, `GET /gov/proposals/{id}/{tally,proof,chain,discussion}`, discussion comment CRUD
- **Delegations** (vote-level): `POST/GET /gov/delegations`, `GET /gov/delegations/{id}`
- **Action items** (domain-scoped): `GET /gov/domains/{domain_id}/action-items`, `GET /gov/domains/{domain_id}/action-items/{item_id}`, `PATCH .../status`, `POST .../notes`
- **Structures** (entity-scoped): `POST/GET /gov/entities/{entity_id}/structures`, `GET /gov/structures/{structure_id}`, `POST /gov/structures/{structure_id}/roles`
- **Activities** (entity-scoped): `POST/GET /gov/entities/{entity_id}/activities`, `GET /gov/activities/{activity_id}`
- **Federation / SDIS proposal shortcuts**: `/gov/proposals/federation/...`, `/gov/proposals/sdis/...`

### 6.2 On PR #1543

- `POST /gov/meetings`, `GET`, `GET /{id}`, `PATCH /{id}`
- `POST /gov/meetings/{id}/agenda-items`, `PATCH .../{aid}`
- `POST /gov/meetings/{id}/start`, `POST .../end`
- Attendance endpoint (verify exact shape against impl)

### 6.3 On `feat/notification-digests`

- `GET /gov/digest?did=...`

### 6.4 Proposed per tranche

- **Tranche 1 (Program)**: `POST/GET /gov/entities/{entity_id}/programs`, `GET /gov/programs/{program_id}`, `POST /gov/programs/{id}/milestones`, `PATCH .../{mid}`, `POST /gov/programs/{id}/close`, `GET /gov/programs/{id}/dashboard`; `GET /gov/me/scopes`, `GET /gov/me/work`; scope-partitioned `GET /gov/digest?scope=...`
- **Tranche 2 (Funding)**: `/gov/sponsors` CRUD, `/gov/programs/{id}/sponsors`, `/gov/budget-items` CRUD, `/gov/programs/{id}/budget`
- **Tranche 3 (Content/Attendees)**: `/gov/sessions` CRUD, `/gov/programs/{id}/sessions`, `/gov/registrations` CRUD, `/gov/programs/{id}/registrations`, `/gov/programs/{id}/accessibility-status`, `/gov/programs/{id}/representation`
- **Tranche 4 (Migration)**: provenance reads `/gov/history/{ref}`, `/gov/provenance/{ref}` (if not deferred to Tranche 5)
- **Tranche 5 (Documents)**: `/gov/documents` CRUD, version history, search
- **Tranche 6 (IAM/Onboarding)**: `/gov/role-delegations` CRUD, `/gov/onboarding` CRUD

### 6.5 SDK drift

Every added model in `apps/governance/src/http/models.rs` → regenerate `sdk/typescript/src/generated/api-types.ts` → commit as `chore(sdk): regenerate TypeScript API types` (no mixed commits).

---

## 7. Derived views

| View | Data available? | Implementable today? | Tranche |
|------|----------------|---------------------|---------|
| MyActionQueueView | ✅ | 🚧 partial (digest branch) | 0 |
| MyScopeView | ✅ | ❌ (no handler) | 1 |
| MyWorkView | ✅ | ❌ | 1 |
| ProgramDashboardView | ❌ | ❌ | 1 |
| CommitteeOperationalView | ⚠️ | ⚠️ needs composition handler | 1 |
| SponsorPipelineView | ❌ | ❌ | 2 |
| AccessibilityStatusView | ❌ | ❌ | 3 |
| RepresentationDashboardView | ❌ | ❌ | 3 |
| PostMeetingSummaryView | 🚧 | 🚧 (lands with #1543) | 0 |
| CycleComparisonView | ❌ | ❌ | 1+6 |
| OrganizerOnboardingView | ❌ | ❌ | 6 |

---

## 8. External integration status

| Source | Today | Target | Tranche |
|--------|-------|--------|---------|
| Google Docs planning notes | none | `SourceRef { system: GoogleDoc, provenance: Linked }` | 4 |
| Google Sheets planning workbook | none | Linked + selective Imported rows (sponsor, budget) | 4 |
| Summit website / CMS | none | Linked; Session publication state later mirrored | 3 / 4 |
| ny-coop-net (Postgres) | live separate app | Imported: entities, participations, programs, sponsors, registrations | 4 |
| Mailing lists | none | Linked only | 4 |
| Legacy NYCN repos | none | Linked or Mirrored | 4 |

**Rule**: no external-source data enters ICN canonical state without a `SourceRef` attached.

---

## 9. ny-coop-net → ICN import crosswalk

Concrete per-domain migration mapping the importer must implement (Tranche 4). Every row carries `SourceRef { source_system: NyCoopNetRow, external_identifier: "<table>:<pk>", provenance_type: Imported }`.

| ny-coop-net (conceptual) | ICN target object | Notes |
|--------------------------|-------------------|-------|
| `organizations` (co-ops, partner orgs) | `Entity { type: Cooperative | Community }` for independent orgs; or `Participation { type: MemberOrg, object_ref: "nycn" }` for federation-only relationships | Some orgs will need both: Entity for their own identity + Participation to NYCN |
| `members` (individuals associated with orgs) | `Participation { type: Member }` linked to their org entity, plus possibly `RoleAssignment` if they have an organizer role | Only those with organizer authority get `RoleAssignment` |
| `summits` / `events` (annual summit rows) | `Program { kind: AnnualSummit, year: <n>, parent_entity_id: "nycn-organizers" }` for cycles; `Activity { kind: Event }` for smaller events | Historical summits imported as `Closed` programs |
| `sponsors` (summit sponsor rows) | `Sponsor` with status mapped from legacy state; linked to the appropriate Program | Dedup by (sponsor_org_entity, program_id) |
| `sponsorship_commitments` / amount rows | `BudgetItem { direction: Income, status: Confirmed|Forecast, amount, linked_party }` | If legacy has confirmed amount, mark `Confirmed`; else `Forecast` |
| `registrations` / `attendees` (summit registration rows) | `Registration` with accessibility fields mapped from free-text where possible (else preserved in `accessibility_notes`) | Accept partial mapping; do not require perfect structured translation |
| `sessions` / `speakers` (summit program rows) | `Session` + speaker `Participation` records | Speaker DIDs may not exist for historical speakers — store as stub Entity if needed, or leave speaker_dids empty with `source_refs` carrying original name |
| Notes / meeting minutes rows | `SourceRef { system: GoogleDoc, provenance: Linked }` attached to the relevant program/meeting | Full `InstitutionalDocument` lift is Tranche 5 |
| Email threads / mailing-list archives | `SourceRef { system: MailingList, provenance: Linked }` only | No import; reference only |

**Idempotence rule**: every importer write is keyed on `(source_system, external_identifier)`. Re-running the importer must not create duplicates. The importer writes via the gateway HTTP API (not directly to sled) so validation is exercised identically to normal create flows.

---

## 10. Bootstrap & seed data (missing operational artifact)

Bootstrapping NYCN on a running ICN node requires seed data. This is not currently scripted. It is part of making the institution reproducible and belongs in Tranche 1 / Tranche 4 scope.

| Seed object | Source | Required fields | Owning tranche | Status |
|-------------|--------|-----------------|----------------|--------|
| NYCN federation entity | `icn-entity` create | `EntityId: "nycn"`, `type: Federation`, `FederationProfile { member_entities: [...] }` | 0 (pre-merge) or 1 | ❌ no seed script |
| `nycn-organizers` co-op entity | `icn-entity` create | `EntityId: "nycn-organizers"`, `parent_id: "nycn"`, `type: Cooperative`, treasury account | 0 or 1 | ❌ |
| Committee structures (backbone, finance, content, logistics, marketing) | `POST /gov/structures` | `StructureId::from_raw("nycn-{name}")`, `kind: Committee`, `parent_entity_id: "nycn-organizers"` | 1 | ❌ |
| Accessibility working group | `POST /gov/structures` | `kind: WorkingGroup` | 1 | ❌ |
| Summit-year program | `POST /gov/programs` | `ProgramId::from_raw("summit-2026")`, `kind: AnnualSummit`, `parent_entity_id: "nycn-organizers"`, milestone set | 1 | ❌ (needs Program impl) |
| Default milestones for summit cycle | `POST /gov/programs/{id}/milestones` | `StrategyLocked`, `VenueLocked`, `BudgetLocked`, `PublicLaunchReady`, `EventReady`, `ClosureComplete` | 1 | ❌ |
| Initial RoleAssignments for core organizers | `POST /gov/structures/{id}/roles` | one per committee lead | 1 | ❌ |
| NYCN charter registration | `CharterStore` via governance actor | CCL body from `NYCN-Charter-Draft.yaml` | 1 | ⚠️ YAML exists, not seeded |
| PolicyOracle config for backbone ratification rules | oracle registration via governance actor | type-to-rule mapping | 1 or 2 | ❌ |

**Deliverable**: `docs/strategy/NYCN-Bootstrap-Runbook.md` + a reproducible seed script (shell or Rust binary under `bins/`). Create alongside Tranche 1 merge.

---

## 11. Docs-update triggers

| When | Update |
|------|--------|
| #1543 merges | `docs/INDEX.md` (meeting module), `docs/ARCHITECTURE.md` if meeting-level subsystem mentioned |
| Digest PR merges | `NYCN-Implementation-Plan.md` Phase 4 → ✅, add note on v1.1 scope partitioning |
| Tranche 1 lands | `NYCN-Institutional-Design.md` needs correction pass (committees are Structures, summit is Activity wrapped by Program); architecture spec status table; add `NYCN-Bootstrap-Runbook.md` |
| Tranche 2 lands | Update charter YAML if budget authority is encoded |
| Tranche 4 lands | Add `NYCN-Schema-Mapping.md` documenting actual importer mapping |
| Tranche 5 lands | Close out Phase 5 of `NYCN-Implementation-Plan.md` |

---

## 12. Open questions — phase-gated

Each must be resolved **before** the named phase begins. Do not let these float.

| # | Question | Must decide before | Current recommendation |
|---|----------|-------------------|-----------------------|
| Q1 | Does `Activity` stay a separate primitive, or does `Program` subsume it? | **Tranche 1 branch opens** | Keep both. Activity for lightweight time-bounded things; Program for stage-gated cycles. Document the distinction in Program module docstring. |
| Q2 | Does `Participation` (non-authority) belong in `icn-governance` or `icn-entity`? | **Tranche 2 branch opens** | Lean `icn-governance::participation` to keep governance-adjacent relationships colocated. Reconfirm by reading `icn-entity::membership` first. |
| Q3 | Does `Sponsor + BudgetItem` justify its own `apps/programs` app, or live in `apps/governance`? | **Tranche 2 branch opens** | Start in `apps/governance`. Split to a separate app only when combined LOC exceeds ~5k or oracle logic diverges. |
| Q4 | Is operational `RoleDelegation` distinct from `icn-governance::delegation` (1717 LOC), or should we extend the existing module? | **Tranche 6 branch opens** | Read `delegation.rs` fully first. Spec currently assumes distinct. Extend only if the semantics genuinely overlap without overload. |
| Q5 | Does `InstitutionalDocument` gossip from day 1, or sled-only initially? | **Tranche 5 design review** | Gossip from day 1 (cleaner design; replication is the whole point of institutional memory). Sled-only is a Plan B if review surface is too large. |
| Q6 | How are authority-ratification rules expressed? Hardcoded match arms, oracle config, or CCL? | **Tranche 1 before policy-oracle rule is written** | CCL-expressed rules loaded into oracle config. Keeps meaning firewall intact. |
| Q7 | Meeting.notes_doc: add as `Option<DocumentId>` now during #1543 review, or defer to Tranche 5? | **before #1543 merges** | Add now as `Option<DocumentId>` with `#[serde(default)]`. Cheap to add pre-merge; avoids a later migration. |

---

## 13. Tests required per tranche

| Tranche | Scenario tests (minimum) |
|---------|-------------------------|
| 0 | S2 (consent → action → digest), S3 (meeting closure continuity), assignee-index backfill correctness |
| 1 | S1 (backbone → full-team), Program lifecycle + milestone transitions, scope-partitioned digest disjointness |
| 2 | S5 (sponsor pipeline → budget → action), sponsor-confirm idempotence |
| 3 | S4 (registration accessibility feedback), representation bucket stability |
| 4 | Importer idempotence, random-sample spot-check, provenance round-trip |
| 5 | S7 (provenance chain), cross-node gossip replication, version lineage |
| 6 | Delegation grant → act → revoke, onboarding pipeline traversal, S6 (cycle closure + handoff) |
| Cross-cutting | S8 (replay idempotence) maintained as regression guard throughout |

---

## 14. Agent assignments

| Tranche | Suggested agent | Reason |
|---------|----------------|--------|
| 0a (merge #1543) | human review + `icn-rust-core` for fixes | meaning-firewall sensitive |
| 0b (digest rework) | `icn-rust-core` | index backfill + store composition |
| 1 (Program + Milestones) | `icn-governance-advisor` design + `icn-rust-core` impl | state machine correctness |
| 2 (Sponsor + Budget) | `icn-governance-advisor` + `icn-economics-advisor` on budget/ledger boundary | ledger boundary risk |
| 3 (Session + Registration + Accessibility) | `icn-rust-core` with `icn-governance-advisor` review | operational domains |
| 4 (SourceRef + importer) | `icn-rust-core` + human operator for importer binary | touches external DB |
| 5 (Documents + gossip) | `icn-gossip-net` + `icn-rust-core` + `icn-governance-advisor` | gossip integration |
| 6 (RoleDelegation + Onboarding) | `icn-identity-iam-advisor` + `icn-governance-advisor` | IAM + governance crossover |

Every tranche gets one pass by `icn-invariants-guardian` before merge.
