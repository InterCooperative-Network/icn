# NYCN on ICN — Institutional Design Document

> **⚠️ Revision notice (2026-04-15)** — Entity Structure section corrected in-place.
>
> This doc's original entity tree (2026-04-13) modeled committees and the summit year as `Community` entities. That is **superseded** by the layered ontology merged in #1540 (2026-04-14) and locked in [`NYCN-Repo-Architecture-Spec.md`](NYCN-Repo-Architecture-Spec.md) §0.3:
>
> - **Entities** (sovereign; join federations; hold independent treasury): NYCN federation, `nycn-organizers` co-op, member co-ops.
> - **Structures** (non-sovereign; owned by a parent entity; delegated authority): committees, working groups, backbone, host pods. `icn-governance::structure::Structure { kind: Committee | WorkingGroup | Team | Office }`.
> - **Activities** (time-bounded; owned by a parent entity): the summit cycle, regional meetup series. `icn-governance::activity::Activity { kind: Event | Program | Project | Initiative }`, with a proposed first-class `Program` wrapper adding milestones and stage gates (Tranche 1 of the execution plan).
>
> For canonical repo state, see the three companion docs: [`NYCN-Repo-Architecture-Spec.md`](NYCN-Repo-Architecture-Spec.md), [`NYCN-Implementation-Matrix.md`](NYCN-Implementation-Matrix.md), [`NYCN-Execution-Tranches.md`](NYCN-Execution-Tranches.md).

## Introduction

This document formalizes the design for the New York Cooperative Network (NYCN) as a **native institution inside the InterCooperative Network (ICN)**.

NYCN is not an application that uses ICN. NYCN is an institutional arrangement that lives within ICN. Every entity, internal structure, activity, decision, financial event, and communication channel maps to an ICN primitive. There is no separate database, no parallel app, no bespoke backend.

The goal is to transform NYCN from an informal coordination network into a **durable, governed federation** with real operational capacity, institutional memory, and long-term continuity — and to prove that ICN can host real institutions in the process.

---

## Core Thesis

> NYCN is a regional federation of cooperatives, communities, and aligned organizations that coordinate, govern shared initiatives, and organize the annual summit through ICN-native identity, governance, and economic primitives.

Every feature NYCN needs is a feature every future ICN institution needs. Nothing built for NYCN is bespoke. Everything built is platform.

---

## Entity Structure

### ICN Primitive Mapping

NYCN maps across three layers of ICN governance/identity primitives:

1. **Entity model** (`icn-entity`): four sovereign types — `Individual`, `Cooperative(CooperativeProfile)`, `Community(CommunityProfile)`, `Federation(FederationProfile)`. Entities nest via `parent_id`, and each cooperative/federation has a `governance_domain_id` and a `treasury_account`. Entities are the only layer that can join federations or hold independent treasury.

2. **Structure model** (`icn-governance::structure`): non-sovereign internal units — `Structure { kind: Committee | WorkingGroup | Team | Office }` — owned by a parent entity and carrying delegated authority. Structures have `RoleAssignment`s (with `authority_scope: Vec<String>` capability strings, not a typed enum). Structures cannot hold independent treasury or join federations.

3. **Activity model** (`icn-governance::activity`): time-bounded work — `Activity { kind: Event | Program | Project | Initiative }` — owned by a parent entity. A proposed first-class `Program` primitive (Tranche 1) will wrap `Activity` with milestones and stage gates for cycle-level containment.

Operational objects (action items, meetings, documents) attach to any of the three layers via `icn-governance::parent::InstitutionalParent`.

### NYCN Entity / Structure / Activity Topology

```
nycn                                   Entity: Federation(FederationProfile)
├── governance_domain_id:              "nycn-federation-gov"
├── treasury_account:                  "nycn-federation-treasury"
├── member_entities:                   [greenstar, cooperation-buffalo, ica-group, ...]
│
├── nycn-organizers                    Entity: Cooperative(CooperativeProfile)
│   ├── parent_id:                     "nycn"
│   ├── governance_domain_id:          "nycn-organizers-gov"
│   ├── treasury_account:              "nycn-ops-treasury"
│   │
│   ├── [STRUCTURES — owned by nycn-organizers]
│   │   ├── nycn-backbone              Structure { kind: Committee }
│   │   ├── nycn-steering              Structure { kind: Committee }
│   │   ├── nycn-content               Structure { kind: Committee }
│   │   ├── nycn-logistics             Structure { kind: Committee }
│   │   ├── nycn-marketing             Structure { kind: Committee }
│   │   ├── nycn-finance               Structure { kind: Committee }
│   │   └── nycn-accessibility-wg      Structure { kind: WorkingGroup }
│   │
│   └── [ACTIVITIES — owned by nycn-organizers]
│       ├── summit-2026                Activity { kind: Event }   — wrapped by Program (Tranche 1)
│       ├── summit-2027                Activity { kind: Event }   — future; parent_program_id = summit-2026
│       └── regional-meetup-q3-2026    Activity { kind: Series }
│
├── greenstar                          Entity: Cooperative(CooperativeProfile)
├── cooperation-buffalo                Entity: Cooperative(CooperativeProfile)
└── ... (N member organizations as independent entities)
```

**Key design decisions (corrected):**

1. **NYCN Federation** is the top-level entity. It governs network-wide decisions: membership admission, federation charter amendments, shared initiatives. `FederationProfile.member_entities` lists participating sovereign entities.

2. **NYCN Organizers** is a sovereign `Cooperative` entity because it holds treasury and executes operations. It is the operational body — the legal/economic container for the people and funds that actually run things.

3. **Committees are `Structure` records**, not entities. Each committee (`nycn-finance`, `nycn-content`, `nycn-logistics`, `nycn-marketing`, `nycn-backbone`, the accessibility working group) lives in `icn-governance::structure::Structure` with `kind: Committee` (or `WorkingGroup`) and `parent_entity_id: "nycn-organizers"`. A governance domain can optionally be provisioned for a structure where scoped proposals/votes are needed, but the structure itself is not a sovereign entity and does not appear in `FederationProfile.member_entities`. This is important: committees have delegated authority (via `RoleAssignment.authority_scope` capability strings), not sovereignty.

4. **Summit 2026 is an `Activity { kind: Event }`**, owned by `nycn-organizers`, to be wrapped by a first-class `Program` primitive in Tranche 1 for milestones, stage gates (`StrategyLocked → VenueLocked → BudgetLocked → PublicLaunchReady → EventReady → ClosureComplete`), and cycle-to-cycle handoff via `parent_program_id` (so `summit-2027` references `summit-2026` for year-over-year comparison). Activities do not appear in `FederationProfile.member_entities` either.

5. **Member organizations are independent entities** that join the federation via the `member_entities` list on `FederationProfile`. They keep their own governance and treasury. The federation relationship is additive, not subordinating.

6. **Authority model**: governance authority is expressed through `RoleAssignment.authority_scope: Vec<String>` (capability strings like `"propose"`, `"approve-budget-<=5000"`, `"backbone-authority"`) combined with PolicyOracle rules. There is no typed authority-level enum; backbone ratification rules key off capability strings declared in CCL/oracle config. See [`NYCN-Repo-Architecture-Spec.md`](NYCN-Repo-Architecture-Spec.md) §4.3.

---

## Identity

### ICN Primitive

DIDs: `did:icn:<base58-ed25519-public-key>`. Created locally via `icnctl id init`. Authentication is challenge-response: gateway issues 32-byte nonce → client signs with private key → gateway verifies → issues JWT with `sub` (DID), `coop_id`, and `scopes`.

### NYCN Application

Every organizer gets a DID. Every participating organization gets one. The practical value is **scope switching**.

When Matt authenticates with `coop_id: "nycn-marketing"` and scopes `["propose", "vote"]`, he's acting as the marketing committee. When he authenticates with `coop_id: "nycn-organizers"` and scopes `["propose", "vote", "treasury_access"]`, he's acting as a co-op steward. Same person, same DID, different institutional hat.

This solves the authority ambiguity problem: when someone approves a $2,000 sponsorship commitment, the system records that they did it as the finance committee coordinator with `TreasuryAccess` capability on `nycn-finance`, not as a random volunteer.

**First circle:** Core organizers (~15 people) and participating organizations. Attendees get identities optionally and later.

---

## Membership Model

### ICN Primitives

`MembershipRole` enum provides:

**Individual roles**: `Founder`, `Member`, `Worker`, `Consumer`, `Producer`, `BoardMember`, `Officer { title }`.

**Entity roles** (when the member is itself an organization): `FederatedMember`, `AssociateMember`, `ObserverMember`, `ProvisionalMember`.

**Custom**: `Custom { name }` for anything else.

Each role carries default `MembershipCapability` grants: `Vote`, `Propose`, `TreasuryAccess`, `Invite`, `ManageSubEntities`, `Sign`, `Configure`.

### NYCN Membership Tiers

| Tier | ICN Entity Type | Federation Role | Capabilities |
|------|----------------|-----------------|-------------|
| **Full Member Co-op** | `Cooperative` | `FederatedMember` | Vote, Propose, TreasuryAccess (if applicable), Invite |
| **Affiliate Organization** | `Cooperative` or `Community` | `AssociateMember` | Vote (limited), Propose (limited) |
| **Emerging Co-op / Community** | `Community` | `ProvisionalMember` | Propose (community-scoped only) |
| **Individual Participant** | `Individual` | `Member` | Vote (in committees they belong to), Propose |
| **Observer** | `Individual` or `Cooperative` | `ObserverMember` | Read-only, can attend but not vote |

### Roles Within the Organizer Co-op

| Role | ICN `MembershipRole` | Capabilities |
|------|---------------------|-------------|
| **Co-op Founder** | `Founder` | All capabilities (Vote, Propose, TreasuryAccess, Invite, ManageSubEntities, Sign, Configure) |
| **Steering Member** | `BoardMember` | Vote, Propose, TreasuryAccess, Invite |
| **Committee Coordinator** | `Officer { title: "Coordinator" }` | Vote, Propose, Invite, Configure (within committee scope) |
| **Organizer** | `Member` | Vote, Propose |
| **Volunteer** | `Custom { name: "Volunteer" }` | Propose (limited scope) |

### Membership Flow

1. Organization exists as an ICN entity (or individual creates a DID)
2. Organization assigns a delegate (membership with `FederatedMember` role)
3. Delegate submits membership request to `nycn-federation-gov`
4. Membership committee reviews (or auto-qualifies based on charter rules)
5. `Membership { action: Add, member: did }` proposal created in federation governance domain
6. Federation members vote per charter thresholds
7. On acceptance: entity added to `FederationProfile.member_entities`, membership record created in sled

---

## Governance

### ICN Primitives

`ProposalPayload` variants available today:

- `Text { body }` — Discussion, resolutions, records
- `Budget { amount, currency, recipient, purpose }` — Single-recipient allocation
- `Allocation { pool_amount, unit, options, purpose }` — Participatory budgeting across competing options
- `Membership { action, member }` — Add or remove members
- `ConfigChange { new_config }` — Governance parameter changes
- `FreezeMember { member, reason, duration }` — Emergency member suspension
- `VetoProposal { target_proposal_id, reason }` — Override governance process
- `ForceCloseProposal { target_proposal_id, reason, forced_outcome }` — Emergency halt

Proposal lifecycle: `Draft → [Deliberation] → Open → Accepted / Rejected / NoQuorum`

Each entity's `governance_domain_id` scopes proposals — steering proposals live in `nycn-steering-gov`, content proposals live in `nycn-content-gov`, etc.

### NYCN Governance Model

**Federation-level decisions** (domain: `nycn-federation-gov`):

| Decision | Payload Type | Threshold | Who Votes |
|----------|-------------|-----------|-----------|
| Admit new member co-op | `Membership { action: Add }` | Simple majority, quorum 50% | All `FederatedMember` delegates |
| Amend federation charter | `ConfigChange` | Two-thirds supermajority | All `FederatedMember` delegates |
| Federation-wide initiative | `Text` | Simple majority | All `FederatedMember` delegates |
| Remove member | `Membership { action: Remove }` | Two-thirds supermajority | All `FederatedMember` delegates |

**Steering committee decisions** (domain: `nycn-steering-gov`):

| Decision | Payload Type | Threshold | Who Votes |
|----------|-------------|-----------|-----------|
| Approve annual budget | `Allocation` | Two-thirds, quorum 50% | Steering committee members |
| Select venue | `Text` (with options in body) | Simple majority | Steering committee members |
| Approve program structure | `Text` | Simple majority | Steering committee members |
| Accept sponsorship > $500 | `Budget` | Simple majority | Steering committee members |
| Form new committee | `Text` + follow-up entity creation | Simple majority | Steering committee members |
| Emergency member freeze | `FreezeMember` | Supermajority | Steering committee members |

**Committee-level decisions** (domain: `nycn-{committee}-gov`):

| Decision | Payload Type | Authority | Who Decides |
|----------|-------------|-----------|-------------|
| Approve individual session | `Text` | Autonomous (no steering approval) | Content committee members |
| Accept sponsorship ≤ $500 | `Budget` | Autonomous | Finance committee members |
| Marketing plan approval | `Text` | Autonomous | Marketing committee members |
| Expense within allocation | `Budget` | Autonomous, within budget limit | Committee coordinator |

The charter defines the **governance-management boundary**: which decisions require a governance vote and which the committee handles autonomously. This boundary is the key organizational design choice.

### Concrete Decision Flow: Venue Selection

1. Logistics committee researches venues, narrows to three options
2. Logistics coordinator creates proposal in `nycn-steering-gov`:
   ```
   ProposalPayload::Text {
     body: "Venue selection: Albany Convention Center ($1,500 deposit, 200 cap, accessible)
            vs. Buffalo Conference Hall ($1,200, 150 cap, limited transit)
            vs. Syracuse Community Center ($800, 120 cap, full accessibility)
            Recommendation: Albany. Cost higher but capacity and accessibility justify it."
   }
   ```
3. Proposal enters deliberation period (charter-defined, e.g., 72 hours)
4. After deliberation, proposal opens for voting
5. Steering members cast votes (threshold: simple majority, quorum: half of steering)
6. On acceptance: decision recorded with full provenance — proposer DID, every vote with DID, timestamps, tally
7. 2027 logistics committee queries `GET /governance/nycn-steering-gov/proposals?state=accepted` and sees exactly why Albany was chosen

---

## Ledger and Budget

### ICN Primitives

`icn-ledger` provides double-entry mutual credit. Every financial event is an immutable `Entry` with debit, credit, timestamp, and metadata. Treasury operations include allocations (capacity grants), settlements (bilateral transfers), and dispute handling. Dynamic credit limits based on trust scores.

### NYCN Budget Architecture

**Treasury accounts** (mapped to entity tree):

```
nycn-federation-treasury     Federation-level funds (dues, shared resources)
nycn-ops-treasury            Organizer co-op operating funds
  ├── allocated to:
  │   nycn-steering          (no separate treasury — uses ops-treasury with scoped authority)
  │   nycn-content           $1,000 allocation
  │   nycn-logistics         $3,000 allocation
  │   nycn-marketing         $500 allocation
  │   nycn-finance           $200 allocation (admin costs)
  │   nycn-summit-2026       event-specific allocation
```

### Budget Lifecycle

**1. Budget approval** — Finance committee drafts budget, submits as `Allocation` proposal to `nycn-steering-gov`:

```
ProposalPayload::Allocation {
    pool_amount: 5200,
    unit: "USD",
    options: [
        { name: "content", amount: 1000, description: "Speaker fees, materials" },
        { name: "logistics", amount: 3000, description: "Venue, AV, catering" },
        { name: "marketing", amount: 500, description: "Ads, printing, outreach" },
        { name: "finance", amount: 200, description: "Banking, admin" },
        { name: "reserve", amount: 500, description: "Contingency" },
    ],
    purpose: "Summit 2026 operating budget",
}
```

When approved (two-thirds of steering), allocations become ledger entries — credits to each committee's scoped account.

**2. Income tracking**:

| Source | Ledger Entry | Provenance |
|--------|-------------|-----------|
| Registration (SimpleTix) | Credit to `nycn-ops-treasury`, metadata: `{ source: "simpletix", batch: "2026-Q3" }` | External receipt — no governance proposal needed |
| Sponsorship (GreenStar, $1,000) | Credit to `nycn-ops-treasury`, metadata: `{ sponsor: "greenstar", proposal_id: "prop-42" }` | Links to governance approval record |
| Grant (CDF Ed Fund) | Credit to `nycn-ops-treasury`, metadata: `{ grant: "cdf-ed-fund-2026" }` | External receipt with grant reference |

**3. Spending**:

When logistics books a $1,500 venue deposit:
```
Settlement {
    from: "nycn-logistics" (scoped allocation),
    to: "external:albany-convention-center",
    amount: 1500,
    currency: "USD",
    reference: "venue-deposit-2026",
    metadata: { proposal_id: "prop-37" }  // links to venue approval
}
```

The ledger knows logistics had $3,000 allocated and now has $1,500 remaining. Every organizer with `ledger:read` scope can see this.

**4. Reconciliation**: At end of cycle, the ledger provides complete financial picture — all income, all expenses, all committee allocations, all surplus or deficit. Not a manually assembled spreadsheet. The actual institutional record.

---

## Trust

### ICN Primitive

`icn-trust` computes trust scores from a graph of attestations. Trust scores gate rate limits, capability grants, and access control. The `TrustPolicyOracle` converts trust scores into `ConstraintSet` values the kernel enforces.

### NYCN Trust Model

Trust is not arbitrary hierarchy. It makes explicit what already happens informally.

| Trust Level | Who | What They Can Do |
|------------|-----|-----------------|
| **New volunteer** (trust < 0.2) | First-time participant | Join committees, propose session topics, participate in discussions |
| **Active organizer** (trust 0.2–0.5) | Returning volunteer with track record | Vote on committee decisions, propose committee-level actions |
| **Committee coordinator** (trust 0.5–0.7) | Experienced organizer, elected/appointed | Approve spending within committee allocation, invite members |
| **Steering member** (trust 0.7+) | Core leadership | Propose budget amendments, vote on federation decisions, sign on behalf of entity |

Trust builds through participation: attending meetings, completing commitments, participating in governance votes, receiving attestations from other members. The charter defines which trust thresholds gate which capabilities — anyone can propose a session topic, but only organizers above 0.5 trust can propose budget amendments.

---

## Communication

### ICN Primitive

`icn-gossip` provides topic-based pub/sub with access control. Topics have `AccessControl`: `Public`, `MinTrustScore(f64)`, or `Participants(Vec<Did>)`. Messages propagate via hybrid push/pull protocol.

### NYCN Topic Structure

```
Topic                                   ACL                              Purpose
────────────────────────────────────────────────────────────────────────────────────
nycn:federation:announcements           MinTrustScore(0.1)               Network-wide announcements
nycn:organizers:general                 Participants([organizer DIDs])   Organizer coordination
nycn:steering:discussion                Participants([steering DIDs])    Steering deliberation
nycn:content:coordination               Participants([content DIDs])     Content committee
nycn:logistics:coordination             Participants([logistics DIDs])   Logistics committee
nycn:marketing:coordination             Participants([marketing DIDs])   Marketing committee
nycn:finance:coordination               Participants([finance DIDs])     Finance committee
nycn:summit-2026:updates                MinTrustScore(0.1)               Summit planning updates
nycn:federation:public                  Public                           Public community space
```

Communication is scoped to institutional structure. Marketing coordination stays in marketing scope. Steering deliberation stays in steering scope. Cross-committee announcements go to the organizer channel. Public announcements go to the community-facing topic.

---

## Institutional Memory

### The Problem

13 years of NYCN organizing knowledge is scattered across Google Drive with inconsistent ownership, naming, and structure. Every year, new organizers reconstruct context from scratch. Knowledge is lost when people leave.

### The ICN Solution

Every meeting record, decision, budget, evaluation, and committee report is stored as a content-addressed, provenance-tracked artifact in ICN's storage layer. It doesn't disappear when someone's Google account changes. It doesn't get filed in the wrong folder.

**What becomes institutional record:**

| Artifact | How It's Stored | Provenance |
|----------|----------------|-----------|
| Meeting agenda | Document artifact scoped to committee entity | Author DID, committee scope, timestamp |
| Meeting notes | Document artifact scoped to committee entity | Author DID, attendee list, decisions captured |
| Governance decision | Proposal record in governance domain | Proposer, all votes, tally, timestamps |
| Budget approval | Allocation proposal + ledger entries | Full chain from proposal to expenditure |
| Committee report | Document artifact scoped to committee entity | Author DID, reporting period |
| Summit evaluation | Document artifact scoped to summit entity | Year, metrics, lessons learned |
| Sponsorship record | Ledger entry + governance approval | Sponsor entity, approval vote, payment receipt |

The 2028 planning committee can trace the complete institutional history: how 2026 was organized, what was decided, what evaluations said, what the budget looked like, who was on each committee and what they did.

---

## Federation

### ICN Primitive

`FederationProfile` tracks `member_entities: Vec<EntityId>` with optional `parent_id` for nested federations. `icn-federation` provides clearing management — multi-party netting, bilateral settlement, receipt generation.

### NYCN Federation Model

The 217+ organizations in the NYCN directory become real federation members or affiliates. Each participating co-op has its own ICN entity. The federation surface lets them:

- **Discover each other** through the federation member registry
- **Coordinate on shared concerns** through federation governance proposals
- **Participate in network governance** through delegate voting
- **Contribute shared resources** through commons artifacts
- **Track inter-organizational relationships** through federation clearing

When GreenStar sponsors the summit for $1,000, that's an inter-entity transaction in the federation ledger, with governance provenance, visible to all federation members who have `ledger:read` scope.

When a Buffalo co-op sends a speaker, that's a cross-entity contribution tracked in the trust system — an attestation that builds the institutional relationship between organizations.

The summit becomes the annual convening of a living network — not a one-off event a small team scrambles to produce.

---

## Summit Lifecycle

### Before (January–September)

**January**: Steering committee meets. Agenda posted as artifact in ICN storage. Committee reports submitted. Steering votes to approve budget (`Allocation` proposal, two-thirds threshold). Allocations become ledger entries. Meeting notes become permanent institutional record.

**February**: Marketing committee operates within $500 allocation autonomously. Marketing plan stored as artifact scoped to marketing entity. $150 Facebook ads logged as settlement against authorized budget.

**March**: Content committee proposes program structure (four tracks, keynote, 20 sessions) as `Text` proposal to `nycn-steering-gov`. Approved. Individual sessions approved within content committee's delegated authority via `nycn-content-gov`.

**April**: Logistics proposes Albany as venue → steering vote → passes. Venue deposit ($1,500) recorded as settlement from logistics allocation, referencing venue approval proposal.

**May–September**: Sponsor commitments flow through governance (above $500) or autonomous committee authority (below $500). All financial events recorded in ledger. Operational execution happens in external tools (email, task apps, etc.) but every institutional decision flows through ICN.

### During (October)

Summit happens. Registration data links attendees to event. Financial transactions (ticket revenue, on-site sponsorship, expenses) flow through ledger.

### After (November)

Post-event. Evaluation data captured and stored as artifact. Financial reconciliation generated from ledger. Knowledge capture: what worked, what didn't, what committees learned. All permanent institutional memory. The 2027 planning committee inherits a complete, auditable, structured record.

---

## What Stays Outside ICN

| Outside ICN | Why |
|-------------|-----|
| Task-level operations ("email sponsor back," "print badges") | Management, not governance. Use whatever task tool organizers prefer. |
| Email campaigns | Mailchimp or equivalent. ICN is not an email platform. |
| Ticket sales UX | SimpleTix or equivalent. Registration needs a consumer-facing interface. |
| Website | Public marketing surface. |
| Social media | External platforms. |
| Day-of logistics | Room assignments, AV, volunteer shifts, food. Operational execution. |

**The line**: ICN holds the institutional reality (who, what authority, what was decided, what money moved, what the rules are). External tools handle operational execution (the actual doing of tasks, the public-facing surfaces, the marketing).

---

## Design Principles

1. **Federation, not centralization.** Member co-ops retain sovereignty. The federation coordinates; it does not govern member internals.
2. **Governed shared layer.** What's shared (budget, program, standards) is governed through proposals and voting. What's internal is each member's business.
3. **Durable institutional memory.** Every decision, every budget, every evaluation persists with provenance. New organizers inherit complete context.
4. **Transparency and accountability.** Every organizer can see budget position, decision history, and committee status at any time.
5. **Actionable coordination, not symbolic membership.** Membership implies structured participation — voting rights, committee access, governance voice. Not just a directory listing.
6. **Scope-bound authority.** Roles and capabilities are per-entity, not global. You act as a committee coordinator within your committee, not across the whole organization.

---

## ICN Platform Gaps

These are features NYCN needs that ICN does not yet provide. They are listed as platform gaps, not NYCN customizations — every institution would need them.

### Exists But Needs Wiring

| Gap | Current State | What's Needed |
|-----|---------------|---------------|
| **Budget-as-constraints** | Charter engine evaluates proposals; ledger tracks balances | Wire allocation approval → spending authority enforcement. When marketing tries to settle $600 but only has $500 allocated, the system should reject or flag. |
| **Multi-domain navigation in UI** | Pilot UI shows one governance domain at a time | Organizer needs to see "my committees → each committee's proposals" in one view. Dashboard scoped to identity and roles. |
| **Proposal creation forms** | API endpoints exist; UI has basic governance tab | Need full proposal creation UX — select type, fill fields, submit, track status |

### Does Not Exist Yet

| Gap | What's Needed | Why Every Institution Needs It |
|-----|---------------|-------------------------------|
| **Content-addressed document storage** | Artifacts (agendas, reports, evaluations, templates) stored with authorship, scope, and provenance | Every institution produces documents. Without this, institutional memory is incomplete. |
| **Onboarding UX** | Invite link → create identity → join entity → get role, all in-browser without crypto key management expertise | Volunteer organizers will not adopt DID-based identity if the onboarding is `icnctl id init` in a terminal. |
| **Notification system** | Push notifications, email digest, "you have 3 pending votes" summaries | No one polls a dashboard. Governance participation requires notification when votes open. |
| **Calendar/event primitive** | Lightweight event/milestone type attached to entities with time-bounded lifecycle | Summits, meetings, deadlines are universal institutional artifacts. |
| **Rich member profiles** | Structured profile fields (skills, organization affiliation, contact info, bio) on entities | Every institution needs a member directory that's more than a list of DIDs. |
| **Seasonal lifecycle** | Entity status patterns for dormant ↔ active ↔ intense ↔ dormant cycles | The summit happens once a year. The institution persists. Entities need to handle seasonal intensity without creating/destroying structure. |

---

## Charter Outline (Draft)

The NYCN federation charter, written in CCL, would define:

### Governance Parameters

```yaml
federation:
  name: "New York Cooperative Network"
  governance:
    membership_admission:
      threshold: simple_majority
      quorum: 0.5
      eligible_voters: federated_members
    charter_amendment:
      threshold: supermajority_two_thirds
      quorum: 0.5
      eligible_voters: federated_members
    
organizer_coop:
  governance:
    budget_approval:
      threshold: supermajority_two_thirds
      quorum: 0.5
      eligible_voters: steering_members
    venue_selection:
      threshold: simple_majority
      quorum: 0.5
      eligible_voters: steering_members
    sponsorship_above_500:
      threshold: simple_majority
      quorum: 0.5
      eligible_voters: steering_members

committees:
  delegation:
    - content: autonomous for individual session approval
    - finance: autonomous for sponsorship ≤ $500
    - marketing: autonomous within budget allocation
    - logistics: autonomous within budget allocation
  escalation:
    - program_structure: requires steering approval
    - budget_amendment: requires steering approval
    - new_committee: requires steering approval

trust:
  thresholds:
    propose_session: 0.0        # anyone
    vote_committee: 0.1         # minimal participation
    propose_budget_change: 0.5  # established organizer
    committee_coordinator: 0.5  # elected/appointed
    steering_membership: 0.7    # core leadership
```

---

## Bootstrap Sequence

The minimum viable institutional instantiation:

1. **Create federation entity** (`nycn`, type `Federation`)
2. **Create organizer co-op** (`nycn-organizers`, type `Cooperative`, parent: `nycn`)
3. **Create steering committee** (`nycn-steering`, type `Community`, parent: `nycn-organizers`)
4. **Issue identities** to 3–5 founding organizers via `icnctl id init`
5. **Add founders** to `nycn-organizers` with `Founder` role
6. **Add founders** to `nycn-steering` with `BoardMember` role
7. **Submit and ratify bootstrap charter** via `Charter` proposal in `nycn-organizers-gov`
8. **Create remaining committees** (content, logistics, marketing, finance)
9. **Invite additional organizers** — founders use `Invite` capability to bring in the broader team
10. **Submit and ratify budget** via `Allocation` proposal in `nycn-steering-gov`
11. **Begin admitting member organizations** to the federation

Steps 1–7 can happen in a single session. Steps 8–11 happen over the following weeks as the institution becomes operational.

---

## Conclusion

NYCN becomes a real institution when:

- Membership implies structured participation, not just a directory listing
- Decisions are governed through proposals with recorded provenance
- Budget is a real ledger with double-entry accounting, not a spreadsheet
- Coordination is persistent, scoped, and survives leadership transitions
- The summit is one function of an ongoing institutional structure
- Member organizations are federation entities with real relationships, not rows in a database

This design establishes NYCN as the first real institutional implementation of ICN. Everything required to make NYCN function is everything required to make ICN function for any institution.

The first serious proof that ICN works will not be a feature checklist. It will be a real federation using it to govern itself.

---

*Established 2026-04-13.*
