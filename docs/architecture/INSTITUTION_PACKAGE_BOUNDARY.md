---
Status: normative
Authority: architecture
Canonical: no
Last Reviewed: 2026-04-17
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
| RoleAssignment | `icn-governance::structure` | Authority as `Vec<String>` capability strings |
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

**Not proposed for ICN now** (institution-specific; no confirmed second-cooperative case):

| Object | Why it stays out | How NYCN models it |
|--------|-----------------|-------------------|
| **Sponsor** | A housing federation calls this "Funder"; a mutual aid network calls it "Partner"; a coop calls it "Donor." Tier semantics, benefit tables, and commitment lifecycle are institution-specific business logic. | NYCN institution package: Activity-linked entity with CCL-governed commitment lifecycle; tiers as CCL data, not ICN enum variants |
| **ServiceRequest / Intake** | Reducible to Activity (kind=project) + ActionItem chain with a routing tag | Compose in institution package; add `intake` tag to Activity kind Custom |
| **Contact / OrgRelationship** | CRM is institution-specific; shape and field semantics vary too much | Institution package data model |
| **VolunteerAssignment** | A RoleAssignment with `capabilities: ["volunteer"]`; no new type needed | RoleAssignment variant |
| **ReviewQueue** | An ActionItemFilter view, not a primitive | `/gov/me/work?status=pending&tag=review` |

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

## Boundary in One Sentence

ICN provides the generic institutional operating system — typed shapes, enforced lifecycles, provenance, capability-gated APIs. The institution package provides the charter, the vocabulary, the seed data, and the specific rules that give those shapes meaning for one community.

---

*This document is normative for future crate and route placement decisions. Update when a new primitive is proposed or when a placement decision is contested.*
