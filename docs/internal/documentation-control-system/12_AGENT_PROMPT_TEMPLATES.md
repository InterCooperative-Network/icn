# Agent Prompt Templates

These prompt shapes are derived from the formal doc system.

## 1. Orientation Prompt

Use at session start.

Include:
- current branch
- authoritative docs
- current milestone/tranche
- what is already known
- immediate objective
- constraints
- output shape

### Template
You are working in the ICN repository.

Read first:
- docs-system spec
- INDEX
- STATE
- ARCHITECTURE
- relevant registry or targeted docs

Current branch:
Current milestone:
Known truth:
Immediate objective:
Non-goals:
Required output:

Do not start implementation until you can summarize current truth accurately.

---

## 2. Discovery Prompt

Use when truth is unclear.

### Template
You are in Discovery Mode.

Question to resolve:
Relevant paths:
Evidence required:
What not to do yet:
Required output headings:
1. Current truth
2. Conflicts or ambiguity
3. Candidate resolution paths
4. Recommended next artifact

Do not modify files yet.

---

## 3. Tranche Prompt

Use when work is bounded.

### Template
You are in Delivery Mode.

Tranche goal:
Smallest safe scope:
Non-goals:
Files allowed to change:
Verification required:
Stop condition:

Do not widen scope.

---

## 4. Continuation / Review Prompt

Use after a prior session, transcript, or PR.

### Template
Continue from the current state.

Already completed:
Still open:
Top pressure point:
What should not be reopened:
Next bounded tranche:
Required deliverable:

Begin by re-confirming current truth before making changes.

