# ICN Institutional Operability Roadmap

> From protocol-shaped architecture toward an accessible, executable operating
> system for cooperative institutions, starting with single-institution truth
> and growing into federation only after governance and economics are real locally.

## Core Principle

**Build downward toward operable institutional reality, and upward only when the
lower layer is honest.**

The product is not "a protocol." It is a **cooperative operating environment**.
Not just identity. Not just governance. Not just payments. Not just contracts.
The real thing is the composition of those parts into institutional workflows:

- join an institution
- prove membership / role / trust
- propose something
- deliberate and vote
- allocate resources
- record obligations
- execute an outcome
- audit what happened
- federate that process across groups

Development is organized around **institutional workflows**, not subsystem elegance.

## North Star

> Make it easier to run a democratic, collectively owned institution than to patch
> one together with capitalist admin tools.

Not ideology versus ideology. Infrastructure versus infrastructure.

The moment ICN lets a cooperative run itself more clearly, more safely, and more
democratically than the Google Docs + Stripe + Slack + bank account + treasurer
spreadsheet mess, it stops being a theory project.

---

## Phase 1: Make One Institution Real

**Threshold:** One real organization can use ICN to run meaningful governance and
economic coordination without falling back to Google Docs, Stripe, Discord, and a
treasurer's spreadsheet.

### Capabilities Required

- Member identity exists
- Institution membership exists
- Roles and permissions exist
- Proposals can be created, discussed, voted, and finalized
- Passed proposals trigger real state changes
- Treasury or budget actions can be requested and approved
- Allocations, obligations, and settlements are durably recorded
- A user can inspect what happened and why

### Done Means

A real institution can run:
- Member admission/removal
- One budget approval flow
- One allocation or settlement flow
- One policy change that actually changes permissions or behavior

### Not Yet

- Broad federation
- Advanced market logic
- Generalized treaty systems
- Multi-domain civilization abstractions

Because if one institution cannot run cleanly, everything above it is theater.

---

## Phase 2: Governance Must Compile Into System Behavior

**Goal:** A passed proposal changes something real.

### Priority Implementation Pattern

Every major proposal type maps to an executable state transition:

| Proposal Type | Effect |
|---------------|--------|
| Add/remove member | Membership roster changed |
| Assign/revoke role | Permissions altered |
| Approve budget | Budget line authorized |
| Authorize disbursement | Release executed |
| Change treasury rule | Policy updated |
| Modify institution policy | Constraints reconfigured |
| Approve federation relationship | Treaty activated |
| Commons usage policy | Access rules changed |

### Required Architecture

- Proposal types with explicit effect models
- Validation before voting closes
- Deterministic execution after passage
- Durable recording of execution result
- Failure handling that is legible, not silent

### Done Means

The system can prove:
- Who proposed it
- Who voted
- Why it passed
- What state changed
- Whether execution succeeded or failed
- What the post-change state is

### Not Yet

- Highly dynamic arbitrary execution
- Universal contract engine replacing all domain logic at once

Explicit, bounded proposal-effect types first. Generalization comes later.

---

## Phase 3: Institutional Economics, Not Consumer Payments

**Goal:** Make ICN useful for collective provisioning and accountable resource
coordination. **Avoid becoming left fintech.**

### Core Economic Primitives

- Treasury/budget lines
- Requests for allocation
- Approval and release conditions
- Recurring obligations
- Member dues or contribution commitments
- Mutual credit between actors or subgroups
- Escrow/release logic
- Durable settlement history

### The Economic Layer Should Answer

- What resources exist?
- What has been committed?
- Who approved what?
- What remains available?
- What obligations are outstanding?
- What conditions govern release?
- How do we coordinate provision without defaulting to commodity purchase?

### First Bounded Tranche

One honest institutional treasury path:
1. Create request
2. Review request
3. Governance approval
4. Execute release or obligation creation
5. Record result in ledger
6. Expose via API/UI

### Done Means

A real institution can manage shared funds or internal credits with governance-backed
authorization and auditability.

### Not Yet

- Speculative tokenomics
- Retail wallet obsession
- Generalized exchange systems
- "Economy layer" abstractions without institutional use

The first question is not "can two strangers transact?"
It is "can a cooperative govern and allocate common resources responsibly?"

---

## Phase 4: Design Everything for the Human Interface Now

**Goal:** Backend structures support a future where most people interact through a
phone, and many users are nontechnical or disabled.

### Development Filter

Before adding backend complexity, ask:
- Can this be explained in plain language in the UI?
- Can a user understand what action they are authorizing?
- Can a user inspect why the system accepted or rejected something?
- Can this be represented accessibly on mobile?
- Does this create hidden operator dependency?

### Priority Surfaces

- Human-readable action summaries
- Plain-language proposal descriptions
- Explicit permission/state explanations
- Accessible audit history
- Notification/event model suitable for mobile
- Role-aware dashboards: member, steward, treasurer, delegate

### Done Means

A user can: join, see what they can do, review a proposal, vote, view
treasury-related actions, understand outcomes without reading code.

### Not Yet

- Pixel-perfect polished app
- Premature frontend expansion across every platform

Legibility first, polish second.

---

## Phase 5: Earn Federation

**Goal:** Allow real institutions to interoperate without surrendering autonomy.

### Order of Operations

1. Institution can govern itself
2. Institution can manage internal economic actions
3. Institution can expose legible state and rules
4. **Then** institutions can recognize and coordinate with one another

### First Federation Capabilities

- Institution identity
- Federation relationship records
- Shared recognition/trust rules
- Inter-institution agreements
- Scoped obligations across institutions
- Auditable cross-institution actions

### Done Means

Two institutions can enter a governed relationship and execute a bounded shared
workflow with durable records and clear permissions.

### Not Yet

- Global spontaneous federation
- Open-ended network complexity
- Abstract distributed choreography for its own sake

Do not network illusions together.

---

## Phase 6: CCL as Institutional Grammar

**Goal:** CCL becomes the legible, executable expression layer for institutional rules,
obligations, permissions, and coordination.

### What CCL Should Eventually Express

- Membership conditions
- Quorum rules
- Voting thresholds
- Treasury release policies
- Recurring obligations
- Steward permissions
- Resource access policies
- Federation agreements
- Conflict resolution paths
- Commons governance rules

### Constraint

CCL must remain readable by serious non-programmer operators. If only
priest-engineers can author or audit it, power recentralizes.

### Done Means

An institution can inspect its own rules in both machine-executable form and
human-readable civic form.

### Not Yet

- Universal everything-language
- Overpowered syntax before good authoring and review models
- Expressive complexity beyond operator comprehension

CCL should become governance law you can run, not magical incantation syntax.

---

## Development Spine

### Track A: Institutional Core
Members, roles, permissions, institution state, audit history

### Track B: Governance Execution
Proposals, voting, tally/finalization, deterministic effect handlers, visible outcomes

### Track C: Treasury / Resource Coordination
Requests, approvals, releases, obligations, settlements, balances/budgets

### Track D: Human Surface
Plain-language summaries, accessible event feed, mobile-friendly APIs, explainable
errors, status/notification model

### Track E: Federation
Institution-to-institution trust, agreements, scoped cross-boundary actions, shared
auditability

### Track F: CCL
Rule templates, executable policy bindings, readable governance/economic contracts,
versioned institutional law

---

## Milestones

### Milestone 1: Minimum Viable Cooperative
One institution can govern membership, vote, authorize a treasury action, and audit
the result.

### Milestone 2: Operational Commons
One institution can manage recurring obligations, budget logic, and role-governed
stewardship through ICN.

### Milestone 3: Accessible Member Interface
A nontechnical user can participate from a phone and understand what they are doing.

### Milestone 4: Federated Cooperation
Two or more real institutions can coordinate through governed agreements.

### Milestone 5: Institutional Grammar
CCL expresses the rules of institutional life in legible executable form.

---

## Explicit Deprioritizations

- Sprawling protocol expansion without user workflows
- Symbolic decentralization features
- Speculative token/economy complexity
- Federation demos that outpace institutional truth
- Overly abstract contract machinery
- Polishing provenance forever while operability lags
- Frontend glitter over backend dishonesty
- Adding endpoints that do not correspond to real capability

---

## Current Priority Order (as of 2026-03-25)

1. Single-institution operability
2. Governance-to-effect linkage
3. Treasury/resource workflow
4. Legible API/event surface
5. Mobile/accessibility-informed backend design
6. Federation of already-real institutions
7. CCL gradual expansion

## Next Implementation Tranche

**Governance→Execution Bridge (FreezeMember)**

When `POST /gov/proposals/{id}/close` finalizes a `FreezeMember` proposal as
`Accepted`, the target member is frozen in the cooperative's ledger with governance
provenance. This is the first proof that ICN governance compiles into execution.

See `docs/strategy/operability-audit-2026-03-25.md` for the full system audit and
ranked implementation queue.
