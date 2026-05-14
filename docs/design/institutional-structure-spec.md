# Institutional Structure + Event Model — Design Spec

**Status**: DRAFT
**Date**: 2026-04-14
**Supersedes**: the prior Committee-as-Community pattern from the earlier NYCN institutional design draft (superseded by the layered ontology).

## Problem

ICN models sovereign institutions (Cooperative, Community, Federation) but has no way to represent the internal operating structures that real institutions use daily — committees, working groups, roles — or the time-bounded activities they produce — events, projects, programs.

NYCN exposed this gap: its 4 committees and annual summit were being modeled as Community entities, which is a category error. Committees are not sovereign. The summit is not an institution. Both need representation without polluting the entity graph.

## Decision

ICN's institutional model has three layers. Each layer has distinct sovereignty, lifecycle, and attachment rules.

### Layer A: Sovereign Entities

`Person | Cooperative | Community | Federation`

Properties:
- Hold identity (DID)
- Own governance domains
- Own treasury accounts
- Join federations
- Persist independently
- Have charter/constitution

These exist today in `icn-entity`. No changes needed to this layer.

### Layer B: Internal Structures

`Committee | WorkingGroup | Team | Office`

Properties:
- Exist only inside a parent entity
- Authority is delegated, not sovereign
- Cannot join federations independently
- Cannot hold independent treasury (spend through parent's treasury with scoped authority)
- Dissolving the parent dissolves them
- Created and dissolved by governance decisions of the parent entity

### Layer C: Activities

`Event | Program | Project | Initiative`

Properties:
- Time-bounded or purpose-bounded
- Owned by an entity (or co-owned by entity + structure)
- Gather meetings, documents, budgets, tasks, registrations
- Not sovereign — no independent governance or federation membership
- Have lifecycle: Planned -> Active -> Completed/Cancelled

## Object Definitions

### Structure

```rust
pub struct Structure {
    pub id: StructureId,
    pub parent_entity_id: EntityId,
    pub kind: StructureKind,
    pub name: String,
    pub mandate: Option<String>,       // what this structure is authorized to do
    pub scope: Vec<String>,            // delegated authority scopes
    pub status: StructureStatus,
    pub created_at: Timestamp,
    pub created_by_decision: Option<ProposalId>,  // provenance
    pub updated_at: Timestamp,
}

pub enum StructureKind {
    Committee,
    WorkingGroup,
    Team,
    Office,
}

pub enum StructureStatus {
    Active,
    Suspended,
    Dissolved,
}
```

### RoleAssignment

```rust
pub struct RoleAssignment {
    pub id: RoleAssignmentId,
    pub structure_id: StructureId,
    pub person_did: Did,
    pub role: String,                  // e.g., "coordinator", "member", "secretary"
    pub authority_scope: Vec<String>,  // what this role can do
    pub start_date: Timestamp,
    pub end_date: Option<Timestamp>,
    pub assigned_by_decision: Option<ProposalId>,
}
```

### Activity

```rust
pub struct Activity {
    pub id: ActivityId,
    pub parent_entity_id: EntityId,
    pub kind: ActivityKind,
    pub name: String,
    pub description: Option<String>,
    pub status: ActivityStatus,
    pub start_date: Option<Timestamp>,
    pub end_date: Option<Timestamp>,
    pub linked_structures: Vec<StructureId>,  // committees contributing to this activity
    pub created_at: Timestamp,
    pub created_by_decision: Option<ProposalId>,
}

pub enum ActivityKind {
    Event,
    Program,
    Project,
    Initiative,
}

pub enum ActivityStatus {
    Planned,
    Active,
    Completed,
    Cancelled,
}
```

### Parent-Aware Attachment

All operational objects (Meeting, ActionItem, Document, Proposal) gain a polymorphic parent reference:

```rust
pub enum InstitutionalParent {
    Entity(EntityId),
    Structure(StructureId),
    Activity(ActivityId),
}
```

This replaces the current pattern where action items have `domain_id` as their only scope. A meeting can belong to a committee. A document can belong to a summit event. A proposal can be scoped to a federation.

## Invariants

1. **Sovereignty boundary**: Structures and Activities NEVER appear in federation membership lists. Only Entities join federations.
2. **Delegation, not independence**: A Structure's authority derives entirely from its parent entity. Revoking the parent's charter dissolves all structures.
3. **Provenance chain**: Structure creation, mandate changes, and role assignments are traceable to governance decisions when proposal-backed.
4. **Treasury scoping**: Structures spend through the parent entity's treasury with scoped authority. They do not hold independent accounts.
5. **Activity lifecycle**: Activities have bounded lifecycle. They transition to Completed or Cancelled, never persist as zombie entities.

## NYCN Concrete Example

### Sovereign layer
- `entity:nycn` — Federation
- `entity:nycn-organizers` — Cooperative (child of nycn)
- `entity:greenstar` — Cooperative (federation member)
- `entity:cooperation-buffalo` — Cooperative (federation member)

### Internal structure layer
- `structure:nycn-steering` — Committee in nycn-organizers
- `structure:nycn-content` — Committee in nycn-organizers
- `structure:nycn-logistics` — Committee in nycn-organizers
- `structure:nycn-finance` — Committee in nycn-organizers

### Activity layer
- `activity:ny-cooperative-summit-2026` — Event owned by nycn-organizers

### Artifact attachment
- Finance committee meeting → parent: `structure:nycn-finance`
- Summit venue contract → parent: `activity:ny-cooperative-summit-2026`
- Federation membership proposal → parent: `entity:nycn`
- Sponsor outreach task → parent: `structure:nycn-marketing`, linked to summit activity

## API Shape (REST)

### Structures

```
POST   /gov/entities/{entity_id}/structures
GET    /gov/entities/{entity_id}/structures
GET    /gov/structures/{structure_id}
PUT    /gov/structures/{structure_id}
DELETE /gov/structures/{structure_id}

POST   /gov/structures/{structure_id}/roles
GET    /gov/structures/{structure_id}/roles
DELETE /gov/structures/{structure_id}/roles/{role_id}
```

### Activities

```
POST   /gov/entities/{entity_id}/activities
GET    /gov/entities/{entity_id}/activities
GET    /gov/activities/{activity_id}
PUT    /gov/activities/{activity_id}
```

### Attachment

Existing endpoints (meetings, action items, documents) gain optional query parameter or body field:

```
POST /gov/domains/{domain_id}/action-items
  { ..., "parent": { "type": "structure", "id": "nycn-finance" } }

GET /gov/structures/{structure_id}/meetings
GET /gov/activities/{activity_id}/documents
```

## Where This Lives in the Codebase

| Concern | Crate | Rationale |
|---------|-------|-----------|
| Structure, Activity types | `icn-governance` | Governance-scoped institutional objects |
| StructureStore, ActivityStore | `icn-governance` + `apps/governance` | Same pattern as ActionItemStore |
| REST endpoints | `apps/governance/src/http/` | Extends existing governance HTTP layer |
| Entity↔Structure relationship | `icn-entity` (optional ref) | Entities may list their structures |
| Gossip replication | `governance:{domain_id}:structures` topic | Same pattern as existing governance gossip |

## Build Order

1. **Types + store traits** in `icn-governance` (~200 lines)
2. **Sled store backend** in `apps/governance/src/manager.rs`
3. **HTTP endpoints** for structures and activities
4. **Parent-aware attachment** on ActionItem, Meeting, Document
5. **Governance hooks** — proposal-backed structure creation/dissolution
6. **Charter update** — correct NYCN charter draft to use structures

## What This Does NOT Include

- UI rendering (separate tranche)
- Notifications/digests for structures (Phase 4 from original plan)
- Document storage (Phase 5 from original plan)
- Trust thresholds for structure roles (future)
- CCL enforcement of delegation scopes (future — currently advisory)

## Relationship to Existing Plan

This tranche **replaces** the Meeting Management phase (Phase 3) from the original sprint plan. Meetings still get built, but they attach to structures and activities rather than being standalone institutional objects. The meeting types themselves are unchanged — what changes is their parent scope.

The Decision-to-Action bridge (Phase 1, PR #1532) and Consent mode (Phase 1, PR #1533) remain as-is. They are prerequisites — structures will eventually be created by accepted proposals through the action bridge.
