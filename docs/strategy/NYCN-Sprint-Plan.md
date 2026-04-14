# NYCN Sprint Plan — Institutional Closure

## North Star

Use NYCN to force ICN to become a real institution-operating system. Every feature closes one piece of the minimum loop required for NYCN to actually govern, coordinate, remember, and execute itself natively on ICN.

The minimum closed loop:

```
committee meeting → discussion → consent decision → accepted outcome
    → generated action/obligation → budget visibility → notification
    → persistent institutional memory → inherited by next cycle
```

If that loop is closed, NYCN is starting to become real inside the system.

---

## Three Lanes

### Lane 1: Platform Closure (Rust code — feature branches + PRs)

Build the institution-facing features ICN currently lacks. Each tranche is one PR.

### Lane 2: Institutional Model (docs + CCL — can land on main)

Define NYCN's actual federation structure, charter, and governance rules. Uses the templates we already wrote. Produces the artifacts needed to instantiate NYCN on a running node.

### Lane 3: Migration Prep (research + scripts — ny-coop-net repo)

Map the existing ny-coop-net data to ICN entities. Build the bridge from PostgreSQL rows to ICN federation members.

---

## Lane 1: Platform Closure

### Tranche 1A: Decision→Action Bridge
**Branch**: `feat/decision-action-bridge`
**Agent**: Opus
**Scope**: ~200 LOC across 4 files
**Why first**: Governance without execution is ceremony. This is the spine.

**What to build**:
- Add `ActionItemSpec` struct and `action_items_on_accept` field to `Proposal` (`icn-governance/src/proposal.rs`)
- Hook into proposal close path in governance actor (`apps/governance/src/actor.rs`, after `ProposalAccepted` event)
- On acceptance: iterate specs → create `ActionItem` records linked to proposal with provenance hash
- Update HTTP models to accept specs in `CreateProposalRequest`

**Guardrails**:
- This is the first bridge pattern, not the universal execution model
- Other proposal types (Budget→ledger, Charter→oracle, Membership→entity) keep their own hooks
- Action items must carry proposal provenance hash for verifiable obligation chain
- Idempotent: duplicate close events must not create duplicate items

**Tests**:
- Proposal with action specs → accept → items created with correct provenance
- Proposal without specs → accept → no items created (backward compat)
- Duplicate close event → no duplicate items
- Action item links back to originating proposal

---

### Tranche 1B: Consent-Based Governance Mode
**Branch**: `feat/consent-governance-mode`
**Agent**: Opus
**Scope**: ~250 LOC across 4 files
**Parallel with 1A** (no dependency)
**Why**: NYCN decides by discussion and lack of objection, not formal voting. This meets the institution where it is.

**What to build**:
- Add `DecisionMode { Majority, Consent }` enum to `GovernanceParams` (`icn-governance/src/config.rs`)
- Add `max_objections: Option<u8>` to `ProposalThresholds` (default: 0 = any objection blocks)
- Add consent evaluation path in `GovernanceProfile` (`profile.rs`): accept if `against_votes <= max_objections`
- Thread `decision_mode` through close path in governance actor
- Expose in domain creation API

**Guardrails**:
- Consent is a configurable policy mode, not hardcoded ideology
- Different institutions will want different objection thresholds and escalation paths
- Quorum semantics: consensus institutions may need "participation witnessed" model, not just "voter count" — consider `QuorumMode { VoterCount, ParticipantWitnessed }` but okay to defer to follow-up

**Tests**:
- Zero objections at deadline → Accepted
- One objection with max_objections=0 → Rejected
- One objection with max_objections=1 → Accepted
- Quorum not met → NoQuorum regardless of mode
- Decision mode set per domain, not global

---

### Tranche 2: Meeting Management
**Branch**: `feat/meeting-management`
**Agent**: Sonnet (Opus review before merge)
**Scope**: ~800 LOC, 1 new file + ~7 modified
**Depends on**: 1A merged (action items must link to meetings)
**Why**: NYCN lives through meetings. Without this primitive, the actual operating heartbeat stays outside the system.

**What to build**:
- New `icn-governance/src/meeting.rs`:
  - `Meeting { id, domain_id, title, scheduled_at, attendees, agenda, status, notes_doc, linked_proposals, linked_action_items }`
  - `MeetingAttendee { did, presence: AttendanceStatus, role: Option<MeetingRole> }`
  - `AttendanceStatus { Invited, Present, Absent, Remote }`
  - `MeetingRole { Facilitator, NoteTaker, Participant, Observer }`
  - `AgendaItem { id, title, presenter, linked_proposal, discussion_outcome, generated_action_items }`
  - `MeetingStoreBackend` trait (follows `ActionItemStoreBackend` pattern)
- `SledMeetingStore` in `apps/governance/src/manager.rs`
- HTTP endpoints under `/gov/domains/{domain_id}/meetings/`
- Gateway events: `MeetingCreated`, `MeetingStarted`, `MeetingEnded`
- Link `ActionItem.meeting_id: Option<MeetingId>`

**Guardrails**:
- Meetings are institutional trace objects, not a calendar clone
- Bias toward agenda/proposal/action/attendance linkage, not scheduling features
- Meeting roles are coordination roles, not governance authority grants
- When meeting ends: unresolved agenda items should be convertible to action items

**Tests**:
- Create meeting → add agenda items linked to proposals → record attendance → end → verify linked outputs
- Meeting-generated action items carry meeting provenance
- Attendance records usable for witness/quorum context

---

### Tranche 3: Document Storage
**Branch**: `feat/institutional-documents`
**Agent**: Opus design + Sonnet implementation
**Scope**: ~600 LOC, 1 new file + ~6 modified
**Depends on**: Tranche 2 (meeting-document linking)
**Why**: Once meetings and decisions exist, institutional memory needs a real home.

**What to build**:
- New `icn-governance/src/document.rs`:
  - `InstitutionalDocument { id (blake3 of body), domain_id, doc_type, title, authors, body, attachments, version, parent_doc, linked_meeting, linked_proposal, tags }`
  - `DocumentType { MeetingAgenda, MeetingMinutes, CommitteeReport, Policy, Evaluation, General }`
  - Every document links to at least one institutional scope or object (entity, meeting, proposal)
  - Version lineage via `parent_doc` chain
- `SledDocumentStore` in manager
- HTTP endpoints: CRUD, version history, search by type/scope
- Gossip topic: `governance:{domain_id}:documents`
- Gateway events: `DocumentCreated`, `DocumentUpdated`

**Guardrails**:
- Typed institutional memory, not generic blob storage
- `id = blake3(body)` is body-only. Attachments referenced by their own hashes. Metadata is mutable.
- Scope linkage can be entity-level (for charters, policies, handbooks) not just meeting/proposal instances
- Protect against "document dumping ground" — types and linkage are enforced

---

### Tranche 4: Notification Digests
**Branch**: `feat/notification-digests`
**Agent**: Sonnet
**Scope**: ~300 LOC, 1 new file + 3 modified
**Depends on**: Tranches 1A + 2
**Why**: Volunteer organizers don't poll dashboards. They need "what needs my attention" summaries.

**What to build**:
- `icn-gateway/src/notification_digest.rs`:
  - `NotificationDigestService` on tokio interval
  - Queries: pending votes (proposals in Open where user hasn't voted), overdue action items (by assignee), upcoming meetings (next 48h)
  - Returns `DigestSummary { pending_votes, overdue_items, upcoming_meetings }`
- Index-driven queries, not whole-store scans
- `GovernanceDigest` notification type in queue
- Wire into gateway startup

**Guardrails**:
- Must be index-driven to avoid becoming a performance trap
- Start with flat DID-level summaries; domain/role partitioning is Phase 4.5
- Respects user notification preferences

---

## Lane 2: Institutional Model

### Tranche 2A: NYCN Charter Draft
**Where**: `docs/strategy/` (new file)
**Agent**: Opus
**No branch needed** (docs-only, lands on main)

Write the actual NYCN charter in CCL YAML using the federation template we already created. Include:
- Federation governance rules (membership admission, charter amendment)
- Steering committee thresholds (budget approval, venue selection, sponsorship)
- Committee delegation rules (content: autonomous for sessions, finance: autonomous ≤$500)
- Trust thresholds mapped to real NYCN operating patterns
- Consent mode as default decision mode for steering

**Source**: Real NYCN meeting notes (already pulled), current committee structure, actual budget/sponsor data.

### Tranche 2B: Entity Instantiation Script
**Where**: `scripts/` or `docs/strategy/` (new file)
**Agent**: Opus

Document the exact sequence of API calls to instantiate NYCN on a running ICN node:
- Create federation entity (`nycn`)
- Create organizing co-op (`nycn-organizers`)
- Create committees (steering, content, logistics, marketing, finance)
- Create summit 2026 entity
- Issue identities for the 14 active organizers
- Assign memberships and roles
- Submit and ratify charter

This becomes both the NYCN bootstrap script and the reference instantiation example for any future institution.

---

## Lane 3: Migration Prep

### Tranche 3A: Schema-to-Entity Mapping
**Where**: `docs/strategy/` (new file)
**Agent**: Sonnet (research only, no code)

Map every ny-coop-net PostgreSQL table to its ICN entity equivalent:
- `organizations` (236 rows) → which become `Cooperative` entities, which become `AssociateMember`, which are just directory entries
- `people` (702 rows, with duplicates) → which become DID-bearing `Individual` entities, which are just contact records
- `sponsorships` → ledger entries with governance provenance
- `sessions` → meeting agenda items or program artifacts
- `committees` → `Community` child entities under organizer co-op
- `events` → time-bounded `Community` entities (summit instances)

### Tranche 3B: Data Cleanup Plan
**Where**: ny-coop-net repo
**Agent**: Sonnet

Address the known data quality issues before migration:
- 180/236 orgs typed as "unknown" — classify real org types
- Duplicate people records from attendee import — deduplicate
- Junk orgs ("Committed", "Total") imported from spreadsheet structure rows — remove
- 2025 sync configs — fix the year-selection bug (already patched locally, needs commit)

---

## Sprint Order

```
Week 1:
  ├── 1A: Decision→Action bridge (Opus)        ──┐
  ├── 1B: Consent governance mode (Opus)         │  parallel
  ├── 2A: NYCN charter draft (Opus, docs-only)   │  parallel
  └── 3A: Schema mapping (Sonnet, research)      ┘  parallel

Week 2:
  ├── 2: Meeting management (Sonnet, after 1A merges)
  ├── 2B: Entity instantiation script (Opus, docs)
  └── 3B: Data cleanup (Sonnet, ny-coop-net)

Week 3:
  ├── 3: Document storage (Opus design + Sonnet impl)
  └── 4: Notification digests (Sonnet, after 2 merges)
```

Phases 1A and 1B are the institutional spine. Everything else builds on them.

---

## Verification Standard

Each platform tranche (Lane 1) requires before merge:
1. `cargo check -p icn-governance` + `cargo test -p icn-governance`
2. `cargo clippy --all-targets -- -D warnings`
3. Scenario test matching the tranche's test list above
4. Failure/idempotence test (duplicate events, partial failures, replay safety)
5. Meaning-firewall review: no institutional semantics in kernel crates

---

## Success Criteria

The sprint is successful when:
- A consent proposal can be created, deliberated, and accepted with no objections
- That accepted proposal automatically generates linked action items
- A meeting can be scheduled, held, and closed, producing linked action items and notes
- Documents can be stored with entity/meeting/proposal linkage and version lineage
- An organizer gets a digest notification showing pending votes + overdue items
- The NYCN charter exists as a real CCL document ready for ratification
- The entity instantiation sequence is documented and tested against the API
- The ny-coop-net data is mapped and cleanup is scoped

When all of that works, NYCN can start inhabiting ICN for real.
