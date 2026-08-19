---
description: Reconcile durable project truth after work by updating the actual domain owner, not a universal state document.
allowed-tools: Read, Write, Edit, Grep, Glob, Bash(git:*, python3:*)
---

Reconcile durable truth after a meaningful change.

This command does **not** assume `docs/STATE.md` is the owner of every project fact. It resolves ownership through `ops/state/truth/sources.json`, updates only the relevant owner(s), and regenerates downstream projections where appropriate.

## 1. Establish what actually changed

```bash
REPO_ROOT="$(git rev-parse --show-toplevel)" || exit 1
git status --short
git log --oneline -20
git diff --name-status origin/main...HEAD 2>/dev/null || true
```

If reconciling already-merged work, fetch/review the actual landed commit(s) instead of relying on session memory.

Separate:

- implementation behavior changed;
- normative semantics changed;
- only tests/evidence changed;
- only generated/projection files changed;
- issue/control state changed;
- no durable truth changed.

## 2. Resolve the affected fact domain(s)

Read `ops/state/truth/sources.json`.

For every claim that needs synchronization, identify its registered owner. Examples of claim classes include semantic contracts, sprint/task state, merge policy, repository topology, agent/skill routing, deployment roles, or other registered domains.

Do not choose an owner because a familiar document happens to mention the fact.

If no domain owns an important durable claim, report **MISSING TRUTH OWNER**. Do not solve the ambiguity by silently declaring a convenient file canonical.

## 3. Verify before synchronizing

For each proposed sync edit, gather evidence from the correct plane:

- implementation claim -> current code/test/schema or landed commit;
- merge/PR/issue claim -> live GitHub;
- operational/runtime claim -> the registered operational source;
- semantic decision -> ratified owner/ADR/maintainer decision.

Do not upgrade implementation maturity, deployment status, phase meaning, or public claims merely because a primitive landed.

## 4. Update the owner, not every mention

Apply the smallest accurate update to the registered owner.

Classify it:

- **sync/process edit**: aligns a stale owner/projection with already-established facts;
- **semantic/governance proposal**: changes what the project means, permits, or claims. Surface separately for review.

Do not mechanically rewrite historical documents to match current terminology. Historical accuracy is also truth.

## 5. Regenerate projections

After an owner changes, identify generated/indexed dependents from repository tooling and regenerate only the affected projections.

Examples may include documentation indexes, the Agent Context Spine, website projections, or other generated artifacts. Use the generators named by those artifacts; never hand-edit their output.

## 6. Check memory/control promotion

If the session discovered:

- a defect -> ensure an issue/control surface owns it;
- a new invariant -> ensure it has an executable test and/or registered semantic owner;
- important rationale -> ADR/owner/issue where appropriate;
- only a resume clue -> handoff may be enough, clearly non-authoritative.

Do not create a handoff to compensate for failing to update durable truth.

## 7. Report

```text
ICN truth reconciliation
  changed behavior: <summary>
  affected domains: <domain -> owner>
  owner edits: <paths or none>
  generated projections: <regenerated / none>
  semantic/governance proposals: <none or explicit list>
  missing owners: <none or explicit list>
  historical docs intentionally untouched: <paths/reason if relevant>
```

If no durable truth owner needs changing, say so and stop. A session ending is not itself a reason to edit project state.
