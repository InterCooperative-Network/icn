# ICN Demo System Design

**Date:** 2026-03-07
**Status:** Approved — implementation planning in progress
**Branch:** target `main`

---

## Thesis

> ICN is not "blockchain for co-ops."
> It is a coordination substrate that lets co-ops govern, account, prove, and collaborate across organizational boundaries without surrendering autonomy.

The demo's job is to answer one question for every person in the room:

**What real problems does this solve for actual cooperatives and the organizations that support them?**

The technology is evidence. The cooperative pain is the subject.

---

## Core Design Principle

> **Demo truthfulness over demo blockage.**
> The governance flow must remain demonstrable even when the final execution-proof layer is still in review. The demo presents the strongest currently verifiable behavior, while cleanly reserving stronger proof claims for the merged `ExecutionReceiptGate` path.

---

## Audience

The demo serves four audience types, often in the same room:

| Audience | Primary question | What lands |
|---|---|---|
| Cooperators | "Would this help us?" | Problem recognition, visible workflow |
| Funders / grant reviewers | "Is this accountable and scalable?" | Auditability, provenance, governance proof |
| Cooperative dev orgs | "Can this support our members without owning them?" | Federation, visibility without centralization |
| Technical collaborators | "Is this real and how does it work?" | Multi-node, architecture, test coverage, honesty about in-progress |

---

## The Demo Cast

Four nodes in the live K3s cluster, reseeded with canonical identities:

| Node | Persona | Type | Seeded issue |
|---|---|---|---|
| `icn-alpha` | **BrightWorks Cooperative** | Worker cooperative | How do we fairly track and distribute member contributions? |
| `icn-beta` | **River City Tool Library** | Community / resource cooperative | How do we coordinate shared equipment across member coops? |
| `icn-gamma` | **Harbor Homes Cooperative** | Housing cooperative | How do we prove that a board vote actually drove a maintenance expenditure? |
| `icn-delta` | **Finger Lakes Cooperative Development Network** | Intermediate org / regional federation | How do we support member coops without becoming their central authority? |

Each node has: named members, seeded proposals, seeded accounts/balances, seeded contribution records, and one unresolved issue the flow will resolve live.

---

## Three-Layer Demo Stack

### Layer 1: Canonical narrative — "A Federation in Motion" (15–20 min)

The primary live presentation. A single story arc showing four cooperatives operating autonomously and coordinating across boundaries. Each act foregrounds a different coop type's perspective, so every audience segment has a "this is for me" moment.

### Layer 2: Modular scenario cuts (5–7 min each)

The canonical story decomposes into standalone modules that can be:
- presented to a single-audience segment,
- recorded independently,
- used as self-serve walkthroughs,
- swapped in as fallback if live demo conditions fail.

### Layer 3: Handoff path

A self-contained package a stranger can run without help:
- one command to start,
- one script per flow,
- one page explaining what they are seeing,
- no improvisation required.

**If it cannot survive handoff, it is not presentation-ready.**

---

## Presentation Arc

### Act 1: The problem (2 min)

Open with pain, not architecture.

> Cooperatives are supposed to be more democratic than firms.
> But in practice, they often inherit the same old problems: unclear decisions, disconnected records, admin overhead, weak federation tooling, and no easy way to prove what happened across organizations.
>
> ICN is built to solve that coordination gap.

### Act 2: One coop, one decision, one proof (Flow 1)

Establish the mental model with the simplest unit: proposal created, members vote, proposal passes, action follows, traceability exists.

### Act 3: Value and fairness (Flow 2)

Widen to contribution and fairness: who contributed, how was value tracked, how do we justify allocations.

### Act 4: Federation (Flow 3)

Show the network: separate nodes, separate autonomy, cross-coop coordination, no central owner.

### Act 5: Institutional trust (Flow 4, optional)

Close with the funder/intermediary view: visibility without domination, audit without bureaucracy, proof that scales.

---

## Flow Definitions

### Flow 1 — Governance Legitimacy and Action Traceability

**Purpose:** Demonstrate that cooperative decisions are not just discussed, but translated into accountable operational action.

**Audience fit:** All — especially housing and worker coops.

**Narrative:** Harbor Homes' board votes on a roof repair. The motion passes. A treasury allocation follows. The system shows the provenance chain from proposal to action.

**Core cooperator question:** *"Did the thing we voted on actually happen, and can we prove it?"*

#### Flow 1A — Current demonstrable scope (buildable now)

- Proposal creation
- Member voting
- Approval result
- Resulting treasury / operational action
- Provenance context around the action

Does **not** yet claim: cryptographic end-to-end receipt-gated linkage.

**Presenter narration (before #1327 lands):**
> Here, the coop approves a spending proposal. We then show the treasury action that follows from that decision, along with the provenance trail. The final receipt-gated enforcement layer is being finalized — that's what upgrades this from visible workflow to machine-verifiable governance-to-execution proof.

#### Flow 1B — Presentation-grade target scope (pending PR #1327)

All of 1A, plus:
- `ExecutionReceiptGate` enforcement
- Verifiable linkage: the execution is bound to the approved governance event
- Receipt artifact as proof output

**Presenter narration (after #1327 merges):**
> This proposal was approved through cooperative governance. The resulting action is not merely recorded after the fact — it is bound through the execution receipt path, so the system can prove the action corresponds to approved governance.

**Use case skins:**
- Housing coop: roof repair approval → maintenance spend
- Worker coop: equipment purchase → treasury allocation
- Food coop: vendor transition → procurement budget

**Dependency:** PR #1327 (`ExecutionReceiptGate`) — in review as of 2026-03-07.

---

### Flow 2 — Patronage, Contribution, and Value Legibility

**Purpose:** Show that value accounting is inspectable, explainable, and not arbitrary.

**Audience fit:** Worker coops, food co-ops, intermediate orgs.

**Narrative:** BrightWorks tracks member labor contributions and distributes patronage. Any member can see why their allocation looks the way it does.

**Core cooperator question:** *"Why did this member get this distribution?"*

What it demonstrates:
- Commons credit / contribution accounting
- Provenance-backed balances
- PatronageTracker logic (NY Corp Law §93 internal capital accounts)
- Distribution legitimacy
- Member-visible explanations

**Use case skins:**
- Worker coop: patronage by labor contribution
- Tool library: contribution credits from volunteer hours
- Food coop: member participation incentives / dividend basis
- Federation: shared program participation credits across orgs

**Readiness:** Demoable now. PatronageTracker (PR #1325) and commons credit settlement (Sprint 13) are merged.

---

### Flow 3 — Federation Coordination Across Autonomous Coops

**Purpose:** Show that independent cooperatives can coordinate without creating a new bureaucracy.

**Audience fit:** Federations, cooperative developers, ecosystem builders.

**Narrative:** River City Tool Library shares equipment access with BrightWorks. Finger Lakes CDN facilitates the federation agreement. Value settles across organizational boundaries. No coop surrenders its autonomy.

**Core cooperator question:** *"How do independent co-ops work together without creating another bureaucracy?"*

What it demonstrates:
- Multiple live nodes with separate identities
- Cross-coop interaction
- Commons credit settlement across orgs
- Federation without centralization
- DelegationManager with gossip sync

**Use case skins:**
- Worker coop assists housing coop, gets credited value
- Tool library shares access with partner coop
- Co-op dev org distributes support resources
- Regional federation tracks commitments without owning member systems

**Readiness:** Demoable now. Multi-node cluster healthy. Commons settlement and gossip sync merged.

---

### Flow 4 (Optional) — Federation Oversight and Institutional Reporting

**Purpose:** Show that an intermediate org can verify what happened without becoming a command center.

**Audience fit:** Funders, grant reviewers, ecosystem orgs.

**Narrative:** Finger Lakes CDN reviews governance and expenditure evidence across member coops for a grant report, without having control over any coop's internal system.

**Core institutional question:** *"Can this produce trustworthy reporting without adding massive admin overhead?"*

What it demonstrates:
- Read-only visibility and proofs
- Governance and expenditure evidence
- Federation-level reporting
- Grant/compliance narrative
- Audit without centralized control

**Readiness:** Demoable now. Strengthened significantly when Flow 1B is complete.

---

## Flow Readiness Matrix

| Flow | Title | Current state | Presentation-ready | Dependency |
|---|---|---|---|---|
| 1A | Governance to authorized action | Demoable | Safe with scoped wording | None |
| 1B | Governance to execution proof | Not yet | Blocked | PR #1327 |
| 2 | Patronage / contribution legibility | Demoable | Ready for polish | None |
| 3 | Federation coordination | Demoable | Needs narrative reseeding | None |
| 4 | Reporting / audit | Demoable | Strengthened by Flow 1B | Soft dep on #1327 |

---

## What Presentation-Ready Requires

### 1. Canonical seeded environment

Deterministic state that always tells the same story:
- same coop names and personas
- same named members per coop
- same sample proposals (one pending, one approved, one with treasury action)
- same balances and contribution records
- same expected CLI/API output
- same federation relationships

### 2. One-command startup

```bash
./demo/scripts/start-demo.sh          # start local or cluster
./demo/scripts/reseed-demo.sh         # reset to canonical state
./demo/scripts/flow-1-governance.sh   # run Flow 1
./demo/scripts/flow-2-patronage.sh    # run Flow 2
./demo/scripts/flow-3-federation.sh   # run Flow 3
```

### 3. Narrated happy-path scripts

Each flow script outputs human-readable narration, not raw API JSON. The terminal is stage machinery; the story is the demo.

### 4. UI anchor points

The Pilot UI should expose at least:
- participating coops and their members
- active and recent proposals
- vote status and outcome
- treasury / execution state
- balances with provenance summary
- recent cross-coop actions and federation relationships

Without visual state, non-technical audiences lose the thread.

### 5. Fail-soft design

Each flow has:
- **Live path** — against local or cluster
- **Prerecorded fallback** — for network/cluster instability
- **Static artifacts** — screenshots, expected outputs, prepared states
- **Explanation layer** — narrator can describe what should appear if something fails

---

## Deliverables

| ID | Artifact | Description | Blocked by |
|---|---|---|---|
| A | Demo architecture doc | This document | — |
| B | Seed data pack | Deterministic fixture set for all four coop personas | — |
| C | Flow scripts | `flow-1-governance.sh`, `flow-2-patronage.sh`, `flow-3-federation.sh`, `flow-4-reporting.sh` | — |
| D | Presenter runbook | Duration variants (5/10/20 min), speaking notes, fallback notes, reset instructions | — |
| E | Self-serve quickstart | Stranger-survivable: what this shows, how to launch, how to run flows, what results mean | — |
| F | Slide layer | 5–7 anchor slides: problem, ecosystem cast, how ICN fits, flow sequence, why it matters, pilot invitation | — |
| G | Flow 1B final wording | Lock proof language and receipt artifacts | PR #1327 |
| H | Full recording | Canonical 15–20 min recorded walkthrough | Flow 1B (#1327) |

---

## Implementation Order

### Phase 1: Canonical narrative and personas

- Finalize four coop identities, names, and one-line descriptions
- Define the story arc and audience variant framings
- Lock module boundaries and flow sequence

### Phase 2: Seed data

- Design deterministic fixture set supporting all three flows
- Cover: proposals, votes, members, accounts, contribution records, federation relationships, treasury actions
- Make reset/reseed deterministic and fast

### Phase 3: Harden Flow 1A

- Governance → treasury action with provenance, no #1327 dependency
- Complete presenter narration with scoped wording
- Integrate UI anchor points for this flow

### Phase 4: Flow 2 — patronage and contribution

- PatronageTracker walkthrough, balance provenance
- Member-legible explanation of why allocations look the way they do

### Phase 5: Flow 3 — federation

- Multi-node story with cross-coop settlement
- Finger Lakes CDN as federation facilitator
- Commons credit exchange visible in UI and CLI

### Phase 6: Handoff materials

- Presenter runbook with duration variants
- Self-serve quickstart
- Recording plan (screen layout, what to click when, fallback moments)
- Audience-specific variant framing

### Phase 7: Flow 1B upgrade (after #1327 merges)

- Replace scoped wording with full proof language
- Capture receipt artifacts
- Polish institutional/funder segment
- Final recording of complete canonical presentation

---

## What Not to Do

- Do not lead with jargon, DAGs, or cryptographic primitives.
- Do not make the audience infer the practical value.
- Do not show every subsystem.
- Do not rely on improvisation.
- Do not let demo state be nondeterministic.
- Do not collapse the distinction between "workflow logged" and "execution receipt enforced."

The technology should feel like **evidence**, not the subject.

---

## Success Criteria

The demo is presentation-ready when all of these are true:

- [ ] A cooperator can say: *"I know exactly why this would help us."*
- [ ] A funder can say: *"I can see how this supports accountability and scaling."*
- [ ] A technical collaborator can say: *"I understand the architecture and why the multi-node model matters."*
- [ ] The core flow runs twice in a row without surprises.
- [ ] A recorded version and a live version tell the same story.
- [ ] A handoff user can reproduce the happy path without needing you in the room.
- [ ] Flow 1 narration is honest about what is demonstrated vs. what #1327 completes.

---

## Appendix: Messaging Rules

### Before PR #1327 lands — Flow 1 narration

> Here, the coop approves a spending proposal. We show the treasury action that follows from that decision, along with the provenance trail around it. The final receipt-gated enforcement layer is being finalized — that's what upgrades this from visible workflow to machine-verifiable governance-to-execution proof.

### After PR #1327 lands — Flow 1 narration

> This proposal was approved through cooperative governance. The resulting action is not merely recorded after the fact — it is bound through the execution receipt path, so the system can prove the action corresponds to approved governance.

### General framing for "is this production-ready?"

> We're in pilot phase with real cooperatives. The core infrastructure has 2,000+ tests passing. We're building out the presentation and handoff layer now, and hardening the final proof linkage.
