---
Status: normative
Canonical: yes
Last Reviewed: 2026-03-26
---

# ICN Development Control System

**Note:** The YAML header above is the documentation control-plane source of truth for classification of this file. Narrative status below is retained for historical context.

**Canonical scope:** Only paths in `[control].canonical_doc_paths` in `docs/registry.toml` are *control-plane canonical* (machine-checked headers + merged registry alignment). Domain hubs (for example operations runbook indexes) remain important but are not in that set unless explicitly listed there.

**Status (narrative):** Proposed canonical replacement / consolidation draft  
**Date:** 2026-03-25  
**Purpose:** Replace recursive document sprawl with a formal development control plane that tells ICN what kind of work it is doing, what artifact should exist next, where that artifact belongs, and how humans and agents should move safely from discovery to delivery.

---

## 1. Why this exists

ICN no longer fails because there are too few ideas. It fails when ideas, architecture, plans, issues, PRs, and agent sessions drift out of sync.

The project needs a control system that answers, at any moment:

- What is ICN trying to become?
- What is true right now?
- What kind of work are we doing?
- What artifact should exist next?
- What is the smallest safe tranche?
- How should an agent be instructed for this stage?
- What must be updated afterward?

This is not bureaucracy. It is the minimum structure required for emergent architecture to become durable institutional architecture.

---

## 2. First principles

### 2.1 Kernel truth
ICN is infrastructure first: a P2P constraint engine with a hard kernel/app boundary.

### 2.2 Emergence over premature closure
The system should enable higher-order institutional and civilizational coordination as an emergent property. That does **not** mean hardcoding full social semantics into the kernel.

### 2.3 Truth before momentum
Do not let activity impersonate progress. Passing tests, long chats, and merged code do not matter if semantics, boundaries, or architectural truth are still false.

### 2.4 Explicit beats implicit
If a decision matters, it must exist outside chat:
- doc
- ADR
- RFC
- spec
- issue
- PR
- status note
- runbook

### 2.5 Retrieval over preload
Agents should not load the whole universe every time. They should start from a lightweight orientation, then retrieve targeted context on demand.

### 2.6 Smallest safe tranche
All work should be risk-bounded, not time-bounded.

---

## 3. The two-loop development model

ICN development runs on two interacting loops.

## 3.1 Discovery Loop
Use when the problem, boundary, or decision is still unclear.

**Loop:**  
**observe -> question -> investigate -> model -> propose**

**Outputs:**
- problem statements
- investigation notes / spikes
- architecture clarifications
- RFCs / design proposals

**Use discovery mode when:**
- the authoritative path is unclear
- semantics are ambiguous
- multiple plausible designs exist
- repo truth must be resolved before coding
- implementation would otherwise be guesswork

## 3.2 Delivery Loop
Use when enough is known to make a bounded, safe change.

**Loop:**  
**decide -> tranche -> implement -> verify -> record -> merge**

**Outputs:**
- tranche plans
- issues
- code changes
- tests
- PRs
- status notes
- doc updates

**Use delivery mode when:**
- the next change is bounded
- repo truth is known
- invariants are clear
- blast radius is acceptable
- success criteria are explicit

## 3.3 Governing rule
Never force delivery when still in discovery.  
Never stay in discovery once the next safe tranche is clear.

At every stage, explicitly ask:

**Are we discovering, or delivering?**

---

## 4. The master navigation loop

### Step 1: Re-orient
Establish current truth.

Questions:
- What branch are we on?
- What is true in code right now?
- What docs are authoritative?
- What decisions already exist?
- What work is in flight?
- What was the last completed tranche?
- What tests or repo evidence matter here?

### Step 2: Identify pressure
Find the most important mismatch between the north star and the current state.

Questions:
- What is most false right now?
- What is most blocking right now?
- What is causing drift, ambiguity, or false closure?
- What prevents the next safe move?

### Step 3: Name the next real question
Questions:
- What must we learn before acting?
- What decision is actually pending?
- What ambiguity is load-bearing?

If the question is still fuzzy, remain in discovery.

### Step 4: Choose mode
- Missing understanding -> discovery
- Missing implementation -> delivery

### Step 5: Produce the right artifact
Discovery artifacts:
- problem statement
- investigation note
- RFC
- architecture clarification

Delivery artifacts:
- tranche plan
- issue
- PR
- spec update
- ADR update
- runbook / status update

### Step 6: Execute one bounded tranche
A tranche should:
- target one class of problem
- have explicit non-goals
- have verification criteria
- stop cleanly

### Step 7: Verify truthfully
Questions:
- What changed in reality?
- What remains unverified?
- Were invariants preserved?
- Did scope widen accidentally?

### Step 8: Record what became true
Update the map:
- ADR
- spec
- issue
- PR
- roadmap / status
- session summary

### Step 9: Re-rank and repeat
Ask again:
- What is the top remaining pressure?
- What is the next real question?
- What is the next safe tranche?

---

## 5. Stage gates

Before significant work, pass these gates.

### Gate 1: Orientation Gate
Must know:
- current branch
- current truth in code
- authoritative docs
- last completed tranche
- top pressure point

### Gate 2: Mode Gate
Are we discovering or delivering?

If unclear, default to discovery.

### Gate 3: Artifact Gate
What artifact should exist next?

Choose one:
- problem statement
- investigation note
- RFC
- ADR
- spec
- implementation plan
- issue
- PR
- runbook
- status note

### Gate 4: Boundary Gate
Must define:
- explicit goal
- explicit non-goals
- minimal investigation or patch surface
- understood blast radius

### Gate 5: Verification Gate
Must define:
- what to run
- what evidence is sufficient
- what remains unverified

### Gate 6: Recording Gate
Must define:
- what issue/PR/doc/status artifact gets updated afterward

If these gates are unclear, do not start large work.

---

## 6. Prioritization stack

Use this order.

### 6.1 Invariants first
Anything threatening core project truth:
- Meaning Firewall
- authoritative execution path
- truthful outcomes
- provenance integrity
- kernel/app boundary
- current truth staying current

### 6.2 Blockers next
Anything preventing safe understanding or execution:
- ambiguous semantics
- unclear authoritative path
- missing context at a load-bearing boundary

### 6.3 Load-bearing boundaries
Prioritize interfaces many future tasks depend on:
- governance effect translation
- execution outcome semantics
- identity/trust scope
- treasury authorization boundaries
- gateway/app registration boundaries
- mobile/provenance rendering contracts

### 6.4 Truth before polish
Correctness and legibility before cleanup theater.

### 6.5 Foundational before expansive
Do the work that makes ten future things safer before building one flashy feature.

### 6.6 Smallest safe tranche
Even the right priority must still be sliced down.

---

## 7. Canonical artifact taxonomy

Different docs solve different problems. Stop using one doc type to do another doc type's job.

### 7.1 North-star artifacts
**Use for identity and long-range coherence.**
- thesis / vision doc
- architecture overview
- concept glossary

### 7.2 Discovery artifacts
**Use when truth or design is not settled.**
- problem statements
- investigation notes / spikes
- RFCs / design proposals

### 7.3 Decision artifacts
**Use when a choice becomes durable.**
- ADRs

### 7.4 Exact behavior artifacts
**Use when ambiguity must die.**
- specs

### 7.5 Execution artifacts
**Use to sequence bounded work.**
- roadmap
- implementation plan
- milestone plan
- tranche plan

### 7.6 Operational artifacts
**Use to run and recover real systems.**
- runbooks
- playbooks
- deployment procedures
- recovery procedures

### 7.7 Historical continuity artifacts
**Use to preserve continuity without pretending history is canon.**
- status notes
- changelog
- session summaries
- postmortems

---

## 8. Minimal durable stack for ICN

### Layer 1: North Star
- thesis / vision doc
- current-state architecture overview
- concept glossary

### Layer 2: Discovery + Decisions
- problem statements
- investigation notes
- RFCs
- ADRs

### Layer 3: Exact behavior
Priority specs:
- governance decision lifecycle
- governance effect translation + execution outcomes
- deferred/rejected institutional memory
- identity + trust scope model
- treasury semantics + authorization boundaries
- CCL expression-to-execution mapping
- mobile participation contracts

### Layer 4: Execution
- roadmap
- implementation plans
- milestone docs
- tranche plans

### Layer 5: Operations + History
- runbooks
- status notes
- changelog
- incident/postmortem docs

---

## 9. Physical doc placement rules

Your new draft was stronger on workflow. The repo already has a better instinct for **where** docs should live. Keep that.

### 9.1 Core docs that can live directly under `docs/`
Only durable, project-wide navigation or canon:
- `ARCHITECTURE.md`
- `GETTING_STARTED.md`
- `INDEX.md`
- `STATE.md`
- `TODO.md`
- `glossary.md`
- this control system doc (or a renamed canonical equivalent)

### 9.2 Directory placement
Use these defaults:

- `docs/architecture/` -> architecture deep dives, boundary docs, invariants
- `docs/design/` -> design proposals and subsystem designs not yet frozen
- `docs/spec/` -> exact contracts and behavioral specs
- `docs/planning/` or `docs/plans/` -> implementation sequencing, milestone docs, tranche plans
- `docs/strategy/` -> north-star framing, roadmap strategy, operability framing
- `docs/guides/` -> user/developer/operator guides
- `docs/deployment/` -> deployment and environment docs
- `docs/security/` -> threat models, security design, hardening notes
- `docs/status/` -> current or dated status notes
- `docs/development/sessions/` -> session summaries and archaeology
- `docs/archive/YYYY/` -> superseded or historical docs
- `docs/internal/` -> temporary internal planning that is not yet canon

### 9.3 Placement rule
If a doc is:
- **durable truth** -> put it where future contributors will search first
- **time-bound work coordination** -> planning/status/development
- **historical but still useful** -> archive it, do not leave it impersonating canon

---

## 10. Truth classes

Every important doc should declare a truth class near the top.

Suggested truth classes:
- `normative` -> how the system is supposed to work
- `descriptive` -> what the system currently is
- `operational` -> how to run or verify it
- `historical` -> preserved for context, not current truth
- `draft` -> proposal, not yet frozen

This prevents old docs from silently masquerading as canon.

---

## 11. GitHub model

GitHub is the work coordination layer. It is not the full thinking layer.

### 11.1 Docs vs Issues vs PRs
- **Docs** preserve understanding
- **Issues** track actionable work
- **PRs** propose concrete change

### 11.2 What becomes a doc?
Use a doc for:
- architecture truth
- design rationale
- behavioral contracts
- implementation sequencing
- operating instructions

### 11.3 What becomes an issue?
Use an issue when something is a bounded work item:
- bug
- missing implementation
- docs gap
- research spike
- refactor
- debt item
- follow-up from ADR/spec/plan

A good issue is:
- actionable
- bounded enough to discuss
- linked to source docs

### 11.4 What becomes a PR?
Use a PR when there is a real proposed change.

A PR should:
- explain what changed
- explain why
- link the issue(s)
- summarize verification
- state what remains unverified

### 11.5 Healthy traceability chain
**problem / RFC / ADR / spec / plan -> issue -> PR -> merge -> updated docs/history**

---

## 12. Naming conventions

### 12.1 Durable artifacts
- `ADR-###-short-title.md`
- `RFC-###-short-title.md`
- `SPEC-###-short-title.md` or stable topic-based spec names
- `YYYY-MM-DD-short-description.md` for status notes, investigations, session summaries

### 12.2 Internal numbering
Use hierarchical numbers inside plans:
- 1
- 1.1
- 1.1.1

### 12.3 Rule
File names should tell you:
- whether it is durable or dated
- whether it is decision, proposal, status, or spec
- what domain it belongs to

---

## 13. Agent instruction system

Your draft is clearly stronger than anything currently in the repo on this point. Keep it, but tighten it.

Every agent prompt must include:

1. current truth  
2. work stage  
3. exact objective  
4. constraints and non-goals  
5. verification expectations  
6. stop condition  

### 13.1 Prompt stages

#### Orientation Prompt
Use at session start.

Must include:
- current branch
- authoritative docs
- north star
- known architecture truth
- current tranche / issue / milestone
- what is already known and should not be rediscovered
- immediate objective
- constraints
- required output shape

#### Discovery Prompt
Use when truth is still unclear.

Must include:
- exact question to resolve
- files/surfaces to inspect
- required evidence
- no implementation until truth is resolved
- required output headings

#### Tranche Prompt
Use when work is bounded.

Must include:
- tranche goal
- explicit non-goals
- minimal patch surface
- verification requirements
- stop condition

#### Review / Continuation Prompt
Use after a tranche or transcript.

Must include:
- what was completed
- what remains
- top next pressure point
- what should not be reopened
- next bounded tranche

---

## 14. Retrieval architecture rule

This is where the existing `agent-knowledge-architecture` draft has the better idea.

### 14.1 Repo vs launchpad split
- **ICN repo** is the baseline for code-adjacent truth
- **Launchpad / external memory systems** are the baseline for personal, cross-project, and organizational context

### 14.2 Pull context on demand
Agents should start with:
- lightweight orientation
- authoritative entry docs
- retrieval instructions

They should **not** preload giant blobs of context by default.

### 14.3 2-hop visibility rule
Any durable doc should be reachable within:
**entry point -> index/map -> target**

If not, it is effectively invisible.

### 14.4 Structured indexes beat prose indexes
Maintain machine-friendly maps:
- doc registry
- truth class
- freshness
- canonical/superseded state
- tags/domains
- owning subsystem
- linked ADR/spec/issues

---

## 15. Session control panel

At the start of any serious session, fill these six fields.

1. **North Star**  
   What is ICN trying to become?

2. **Current State**  
   What is true right now in code and docs?

3. **Critical Gap**  
   What is the most important mismatch between current state and north star?

4. **Next Question**  
   What must be learned or decided before the next safe move?

5. **Next Tranche**  
   What is the smallest safe improvement to truth?

6. **Updated Record**  
   What must be written afterward so the project remains legible?

If these six are current, the project is navigable.

---

## 16. Anti-patterns to ban

- chat as the only memory system
- plans pretending to be specs
- ADRs used for unresolved debates
- specs written before the question is understood
- issues used as architecture memory
- PRs smuggling decisions with no doc trail
- overscoped tranches
- stale historical docs sitting in canonical locations
- success theater

---

## 17. Immediate implementation sequence

This is the best near-term adoption path.

### Phase A: Canon formation
1. Adopt this control system as the canonical development OS
2. Define truth classes and add them to top-level canon docs
3. Freeze a small set of authoritative entry docs

### Phase B: Map cleanup
4. Build a document registry with:
   - path
   - truth class
   - freshness
   - canonical/superseded flag
   - domain tags
5. Move obvious historical docs to archive
6. Mark superseded docs explicitly instead of leaving them ambiguous

### Phase C: Decision cleanup
7. Perform ADR sweep over de facto decisions already true in code
8. Link open issues back to ADRs/specs/plans where relevant

### Phase D: Exact behavior cleanup
9. Create or normalize the first load-bearing specs
10. Separate plans from specs where they are currently blended

### Phase E: Workflow enforcement
11. Add issue template aligned to discovery vs delivery
12. Add PR template aligned to verification + linked docs
13. Add session-start control panel template
14. Add agent prompt templates to the repo

---

## 18. What to adopt from the existing drafts

### Adopt directly from the new draft
These parts are already strong and should survive nearly intact:
- discovery vs delivery model
- navigation loop
- stage gates
- artifact taxonomy
- tranche discipline
- agent prompt staging
- six-part control panel
- anti-pattern list

### Adopt from existing repo docs
These parts are already better elsewhere and should be incorporated:
- doc placement rules from `DOCUMENTATION_MAINTENANCE.md`
- retrieval/indexing/on-demand knowledge model from `agent-knowledge-architecture.md`
- explicit kernel/app framing from `ARCHITECTURE.md` and ADR-001
- stage framing and north-star compression from `GOLDEN_PROMPT.md`

### Do not keep as-is
- multiple overlapping “master systems” that compete with each other
- static prose indexes without structured metadata
- historical documents left in active locations without truth-class labeling

---

## 19. Final rule

ICN cannot keep using intuition, memory, and chat recursion as its primary control plane.

That was good enough for emergence. It is not good enough for durable coordination.

The project now needs:
- explicit maps
- explicit truth classes
- explicit decisions
- explicit contracts
- explicit tranches
- explicit retrieval
- explicit records

That is how emergent architecture becomes institutional architecture.
