---
Status: normative
Authority: architecture
Canonical: no
Last Reviewed: 2026-04-16
---

# Institution Package Boundary

> Defines what belongs in ICN platform versus what belongs in an institution-specific package repo (e.g., NYCN). Anchors all future layered implementation decisions.

**Grounded against:** `main` at `fde1466a` (Program + Milestone primitives landed, PR #1548).
**Companion docs:** `KERNEL_APP_SEPARATION.md`, `ADR-001-What-ICN-Is.md`, `NYCN-Repo-Architecture-Spec.md`.

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

---

## B. ICN vs Institution Package

### Goes in ICN (`icn-governance` + `apps/governance`)

A type, object, or capability belongs in ICN if:
- **A second cooperative would need it with different content but the same shape.** Committee, Meeting, Program, Milestone, ActionItem, Sponsor — every recurring institution has these.
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
| RoleAssignment | `icn-governance::structure` | Authority as `Vec<String>` capability strings |
| Meeting | `icn-governance::meeting` | PR #1543, landing |
| InstitutionalParent attachment | `icn-governance::parent` | Polymorphic object attachment |

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

## D. Reusable Primitive Set for NYCN/Summit

These belong in ICN because every cooperative institution needs them. Verified against what NYCN concretely requires for an operational summit cycle.

| Primitive | Why ICN | Institution-specific parts |
|-----------|---------|---------------------------|
| **Structure** (Committee/WG) | Every institution delegates authority to sub-groups | Member list, specific authorities, NYCN committee names |
| **Meeting** | Every body needs a record of deliberation with decisions and attendance | Agenda templates, NYCN meeting norms |
| **Program + Milestone** | Recurring cycles with stage gates exist across institutions | Summit-specific stage names expressed as `completion_criteria: Vec<String>` data |
| **ActionItem** | Work that materializes from decisions is universal | Assignment rules, NYCN priority conventions |
| **Sponsor** | Resource partnership with lifecycle is generic across events and programs | Tier names (platinum/gold/etc.), benefit tables, NYCN ask amounts |
| **ServiceRequest / Intake** | Member-initiated requests with routing and status apply broadly | Category taxonomy, routing rules, SLA definitions |
| **Contact / OrgRelationship** | External relationship management is generic across institutions | CRM fields, relationship types specific to NYCN's partner network |

**Not proposed for ICN now** (institution-specific without a clear second-cooperative case):
- VolunteerAssignment (could be a RoleAssignment variant with `volunteer` kind — defer)
- SponsorLead (a status in the Sponsor lifecycle, not a separate type)
- ReviewQueue (implement as an ActionItem filter view, not a new primitive)

---

## E. Immediate Implementation Implications

In merge order:

**1. `GET /gov/me/scopes` + `GET /gov/me/work`**
The member's entry point. Returns their `RoleAssignment` set and open `ActionItem`s filtered by assignee. Unblocks the "new organizer can see their scope" decisive test. Requires no new types — compose from existing `SledRoleAssignmentStore` + `SledActionItemStore`. This is Tranche 1c.

**2. `Activity.parent_program_id` linkage**
Wire `Activity` to its parent `Program` so `summit-2026 (Activity)` points to `annual-summit-cycle (Program)`. Enables cycle-over-cycle comparison. One field addition; no schema migration needed (optional field, backward-compatible). Tranche 1c.

**3. Sponsor primitive** (`icn-governance::sponsor`)
`Sponsor { id, program_id, org_name, contact_did, tier: String, status, committed_amount, confirmed_at }`. Sled store with `sponsor_by_program` and `sponsor_by_status` indexes. HTTP surface. No NYCN tier names in the enum — `tier` is a free string. Tranche 2.

**4. Notification digest wired to Meeting**
`feat/notification-digests` branch stubs `upcoming_meetings: Vec::new()` because Meeting didn't exist when it was written. After Meeting lands (PR #1543), wire the digest to `SledMeetingStore.list_upcoming_by_scope()`. Tranche 2 follow-on.

**5. `GET /gov/programs/{id}/dashboard` composite view**
Returns Program + Milestone statuses + ActionItem counts by status + Sponsor counts by status. Pure composition from existing stores once Sponsor lands. No new primitives. Enables the summit dashboard without a client-side join. Tranche 3.

---

## Boundary in One Sentence

ICN provides the generic institutional operating system — typed shapes, enforced lifecycles, provenance, capability-gated APIs. The institution package provides the charter, the vocabulary, the seed data, and the specific rules that give those shapes meaning for one community.

---

*This document is normative for future crate and route placement decisions. Update when a new primitive is proposed or when a placement decision is contested.*
