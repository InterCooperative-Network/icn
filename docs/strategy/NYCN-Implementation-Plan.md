# Implementation Plan: ICN Platform Features for Institutional Use

## Context

NYCN is the first real institution to live inside ICN. From their 2026 meeting notes (pulled live from Google Docs), we see they make decisions by informal consensus, delegate to 4 committees, and track everything in Google Docs. ICN has strong governance, ledger, and identity primitives but lacks the operational features institutions actually use daily.

Every feature below is a platform feature — not NYCN-specific. Every institution on ICN will need them.

## Core Diagnosis

ICN already has governance, identity, receipts, notifications, content-addressed storage, and an event bus. The plan is mostly about finishing bridges, not inventing subsystems. But the load-bearing gap is this: **governance risks becoming ceremonial unless accepted decisions reliably dispatch downstream institutional effects.** The Decision-to-Action bridge is the spine of the whole plan, not a follow-on convenience.

---

## Phase 1: Decision-to-Action Bridge (THE SPINE)

**Scope**: Medium | **Agent**: Opus (execution semantics are the correctness hotspot)

When a proposal is accepted, it must produce real downstream institutional effects — not just a status change. Accepted proposals create **institutional obligations** with provenance linkage and verifiable fulfillment state, not merely reminder tasks.

### Design

Add `action_items_on_accept: Vec<ActionItemSpec>` to `Proposal`. On acceptance, the actor creates `ActionItem` records linked to the proposal via `linked_proposal`. Each action item carries the proposal's provenance hash, making the obligation chain verifiable: proposal → acceptance tally → action item → fulfillment update → completion receipt.

This is stronger than "create task objects." It is: accepted proposal → deterministic creation of institutional obligations with provenance linkage.

**Important constraint**: `action_items_on_accept` is the **first bridge pattern** for obligation-producing proposals, not the universal execution model. Some accepted proposals should alter config, allocate resources, change permissions, or trigger other typed execution paths (budget allocations → ledger entries, charter ratification → oracle deployment, membership → entity updates). The action item bridge handles the common case; other proposal types already have their own execution hooks. Do not let this pattern harden into the only dispatch mechanism.

### Files to modify
- `icn/crates/icn-governance/src/proposal.rs` — add `ActionItemSpec { title, description, assignee, due_offset_seconds, priority, tags }` and `action_items_on_accept: Vec<ActionItemSpec>` with `#[serde(default)]`
- `icn/apps/governance/src/actor.rs` (~line 1970 after ProposalAccepted event) — iterate specs, create ActionItems via store, link to proposal with provenance hash
- `icn/apps/governance/src/http/models.rs` — accept action_items_on_accept in CreateProposalRequest
- `icn/apps/governance/src/http/handlers.rs` — pass through specs

### Scenario tests
- Consent proposal closes with no objections → dispatches linked action items with correct provenance
- Budget proposal accepted → committee allocation action items created with due dates relative to acceptance time
- Action item completion updates are verifiable against the originating proposal

---

## Phase 2: Consent-Based Proposal Mode

**Scope**: Medium | **Agent**: Opus (governance state machine correctness)

**Parallel with Phase 1.** NYCN decides by consensus, not formal votes. ICN needs a configurable "consent" mode: proposal passes unless objections exceed a configurable threshold within the deliberation period.

### Design Principle

Consent is a **configurable governance evaluation mode**, not a universal primitive embedded too deeply. Different institutions will want different objection thresholds, escalation paths, and quorum assumptions. The implementation layers consent as a policy over the same proposal infrastructure — the deliberation period becomes the objection window, "against" votes become objections.

### Files to modify
- `icn/crates/icn-governance/src/config.rs` — add `DecisionMode { Majority, Consent }` enum (with `#[serde(default)]` → `Majority`), add to `GovernanceParams`. Add `max_objections: Option<u8>` to `ProposalThresholds` (default: 0 = any objection blocks). Add `escalation_on_objection: Option<EscalationPolicy>` for institutions that want objections to trigger escalation rather than rejection.
- `icn/crates/icn-governance/src/profile.rs` — add consent evaluation path: if quorum met AND `against_votes <= max_objections` → Accepted. If objections exceed threshold → Rejected (or escalated per policy). If quorum not met → NoQuorum. **Quorum semantics note**: consensus-mode institutions often operate with softer participation assumptions than majority-vote institutions. Consent quorum should support a "participation witnessed" model (e.g., quorum = those who viewed the proposal, not those who voted), not just reuse majority-mode quorum logic with different labels. This may require a `QuorumMode { VoterCount, ParticipantCount }` distinction.
- `icn/apps/governance/src/actor.rs` (~line 1764) — check `domain.config.params.decision_mode`, route to consent evaluation
- `icn/apps/governance/src/http/models.rs` — expose decision_mode and max_objections in domain creation API

### Scenario tests
- Zero objections at deadline → Accepted
- One objection with max_objections=0 → Rejected
- One objection with max_objections=1 → Accepted
- Quorum not met → NoQuorum regardless of mode
- Decision mode change via ConfigChange proposal

---

## Phase 3: Meeting Management

**Scope**: Large | **Agent**: Sonnet with Opus review | **Depends on**: Phase 1 (action items must link to meetings)

### Design Principle

Meetings are **institutional trace objects**, not a calendar clone. A meeting exists because it produces deliberation context, witness context, and linked outputs. Bias toward agenda/proposal/action/minutes linkage, not generic scheduling features.

The part that matters: meetings become first-class institutional objects connecting agenda → proposals → attendance (witness) → action items → records. Not CRUD for calendar entries.

### New file
- `icn/crates/icn-governance/src/meeting.rs` (~300 lines)
  - `Meeting { id, domain_id, title, scheduled_at, attendees: Vec<MeetingAttendee>, agenda: Vec<AgendaItem>, status, notes_doc: Option<DocumentId>, created_by, linked_action_items, linked_proposals }`
  - `MeetingAttendee { did, presence: AttendanceStatus, role: Option<MeetingRole> }` — richer than `(Did, bool)` to avoid boxing in witness context too early
  - `AttendanceStatus { Invited, Present, Absent, Remote }` — extensible for quorum/witness purposes
  - `MeetingRole { Facilitator, NoteTaker, Participant, Observer }` — optional, defaults to Participant
  - `MeetingStatus { Scheduled, InProgress, Completed, Cancelled }`
  - `AgendaItem { id, title, presenter, linked_proposal, discussion_outcome, generated_action_items }`
  - Each agenda item can produce proposals (submitted during meeting) and action items (generated at meeting close)
  - `MeetingStoreBackend` trait — same pattern as `ActionItemStoreBackend`

### Files to modify
- `icn/crates/icn-governance/src/lib.rs` — register module
- `icn/crates/icn-governance/src/action_item.rs` — add `meeting_id: Option<MeetingId>` (keep existing `meeting_context` for backward compat)
- `icn/apps/governance/src/manager.rs` — add `SledMeetingStore`, add to `GovernanceManager`
- `icn/apps/governance/src/http/` — handlers, models, routes under `/gov/domains/{domain_id}/meetings/`
- `icn/crates/icn-gateway/src/events.rs` — add `MeetingCreated`, `MeetingStarted`, `MeetingEnded`

### Scenario tests
- Meeting created → agenda items link to pending proposals
- Meeting ended → unresolved agenda items generate action items with meeting provenance
- Attendance records provide witness context for proposals discussed in meeting

---

## Phase 4: Notification Aggregation

**Scope**: Medium | **Agent**: Sonnet | **Depends on**: Phases 1 + 3

"You have 3 pending votes and 2 overdue action items" — digest summaries, not event-noise spam.

### Design Principle

Aggregation must be **index-driven, not whole-store-scan**. This is the kind of feature that starts simple and becomes a performance trap. The digest service queries derived indexes/views, not raw stores.

### New file
- `icn/crates/icn-gateway/src/notification_digest.rs` (~250 lines)
  - `NotificationDigestService` — spawns tokio task on configurable interval
  - `generate_digest(did)` queries: pending votes (Open proposals where user hasn't voted — use proposal index by state + vote set), overdue action items (user is assignee past due_date — use action item index by assignee+status), upcoming meetings (next 48h — use meeting index by scheduled_at)
  - Returns `DigestSummary { pending_votes, overdue_items, upcoming_meetings }`

### Indexes required
- Action items: index by `assignee` + `status` (add to SledActionItemStore)
- Proposals: index by `state` + `domain_id` (may already exist)
- Meetings: index by `domain_id` + `scheduled_at` range

### Files to modify
- `icn/crates/icn-gateway/src/notification_queue.rs` — add `GovernanceDigest` notification type
- `icn/crates/icn-gateway/src/server.rs` — spawn digest service
- `icn/apps/governance/src/manager.rs` — add index-backed query methods for digest generation

### Scenario tests
- Digest counts are correct across voting/action/meeting state changes
- Digest respects user notification preferences (opt-in/out)
- Digest generation completes in bounded time regardless of store size

---

## Phase 5: Document Storage (Institutional Memory)

**Scope**: Large | **Agent**: Opus for type design + gossip integration, Sonnet for HTTP/sled | **Depends on**: Phase 3 (meeting-document linking)

### Design Principle

**Typed institutional memory, not generic blob storage.** Documents are scoped, typed, content-addressed artifacts with provenance, version lineage, authorship, and linkage to meetings, proposals, entities, and committees. Protect against "documents as blob dumping ground."

Build on existing gossip + sled. No new kernel subsystem.

### New file
- `icn/crates/icn-governance/src/document.rs` (~400 lines)
  - `InstitutionalDocument { id (blake3 hash of body), domain_id, doc_type, title, authors: Vec<Did>, body: String, attachments: Vec<AttachmentRef>, version, parent_doc: Option<DocumentId>, linked_meeting: Option<MeetingId>, linked_proposal: Option<ProposalId>, tags, created_at }`
  - `DocumentType { MeetingAgenda, MeetingMinutes, CommitteeReport, Policy, Evaluation, General }`
  - Every document is linked to at least one **institutional scope or object** — this can be a specific meeting/proposal instance OR an entity scope (committee, cooperative, federation). Charters, policies, member handbooks, and onboarding docs link to entity scope without requiring a specific meeting or proposal origin.
  - Version lineage via `parent_doc` — each version is a new content-addressed document pointing to its predecessor

### Gossip integration
- Topic: `governance:{domain_id}:documents`
- Published as gossip entries (content-addressed, auto-replicated via existing protocol)
- Sled indexes: `doc:{domain_id}:{doc_id}`, `doc_idx:{domain_id}:{type}:{timestamp}`, `doc_link:{meeting_id}:{doc_id}`

### Files to modify
- `icn/crates/icn-governance/src/lib.rs`, `meeting.rs` (link notes to DocumentId)
- `icn/apps/governance/src/manager.rs` — `SledDocumentStore`
- `icn/apps/governance/src/http/` — CRUD, version history, search
- `icn/crates/icn-gateway/src/events.rs` — `DocumentCreated`, `DocumentUpdated`

### Scenario tests
- Document versions remain hash-addressable and gossip-replicable
- Meeting minutes linked to meeting → linked to proposals discussed → linked to action items generated
- Full provenance chain queryable: document → meeting → proposal → action items

---

## Merge Ordering

```
Phase 1 (Decision→Action SPINE) ──┐
                                   ├──> Phase 4 (Notification Digests)
Phase 2 (Consent Mode) ───────────┘         │
                                             v
Phase 3 (Meeting Management) ──────> Phase 5 (Document Storage)
```

**Narrative**: Governance semantics and execution bridge first (Phases 1 + 2). Then operator usability layers (Phases 3–5). Meetings are a coordination surface. Execution closure is the institutional core.

Phases 1 and 2 develop in parallel on separate branches. Phase 3 can start development while 1+2 are in review. Phase 4 depends on 1+3. Phase 5 depends on 3.

## Agent Assignment

| Phase | Agent | Rationale |
|-------|-------|-----------|
| 1. Decision→Action | **Opus** | Execution semantics, provenance chain correctness |
| 2. Consent Mode | **Opus** | Governance state machine, policy configurability |
| 3. Meeting Management | **Sonnet** + Opus review | Follows action item pattern, but meaning-firewall review needed |
| 4. Notification Digests | **Sonnet** | Additive, index design review by Opus |
| 5. Document Storage | **Opus** design + **Sonnet** HTTP/sled | Gossip integration, institutional typing |

**Integration review requirement**: One pass by whoever has strongest understanding of ICN's meaning-firewall and governance/execution boundary. The risk is not syntax — it's accidentally building institution-specific semantics into the wrong layer.

## Implementation Guardrails

1. **Phase 1 should eventually grow into a formal execution-dispatch abstraction.** `action_items_on_accept` is the first bridge, but long-term the architecture wants typed `ExecutionEffect` handlers rather than accumulating one-off post-accept hooks.

2. **Consent quorum witness model needs explicit evidence semantics.** If quorum can be based on participation rather than voting, be explicit about how participation is attested and by whom. Don't smuggle social-process ambiguity into correctness-sensitive state transitions.

3. **Meeting roles are coordination roles, not governance authority grants.** Facilitator and NoteTaker do not imply permission or decision authority unless explicitly linked by charter policy. Keep the boundary clean.

4. **Document hash identity**: `id = blake3(body)` is intentionally body-only. Attachments are referenced by their own hashes in `AttachmentRef`. Metadata (title, authors, tags) is mutable and does not affect the content hash. A new version = new body = new hash with `parent_doc` pointing to predecessor. Make this explicit in the type docs.

5. **Notification digests should eventually support partitioning by domain/role/scope.** A single DID-level summary gets noisy when members act across multiple committees. Phase 4 can start with flat DID-level; Phase 4.5 adds domain partitioning.

6. **Meaning-firewall review on every phase.** One integration pass by whoever understands the kernel/app boundary best. The risk is not syntax — it's institutional semantics leaking into the wrong layer.

## Verification

### Compile/lint (each phase)
- `cargo check -p icn-governance`
- `cargo test -p icn-governance`
- `cargo clippy --all-targets -- -D warnings`

### End-to-end institutional scenarios (critical)
1. **Full proposal lifecycle**: Create consent proposal with action items → deliberation period → no objections → auto-accept → action items created with provenance → fulfillment tracked
2. **Meeting-driven governance**: Schedule meeting → add agenda with linked proposals → hold meeting → record attendance → close → generate action items and minutes document
3. **Provenance chain**: Query from document → meeting → proposal → acceptance tally → generated action items → completion status. Full chain must be traversable.
4. **Digest correctness**: Pending votes + overdue items + upcoming meetings counts accurate across state transitions
5. **Cross-node replication**: Documents published via gossip replicate to peer nodes, remain hash-verifiable
6. **Failure and idempotence**: Duplicate close events do not create duplicate action items. Repeated gossip publication does not create duplicate documents. Partial action-item creation failure rolls back cleanly. Acceptance hooks are replay-safe. Cross-node consistency holds under retry.
