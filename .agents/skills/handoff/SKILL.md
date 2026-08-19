---
name: handoff
description: Write a non-authoritative session memory/evidence packet for expensive-to-reconstruct context.
argument-hint: "[--push]"
user-invocable: true
allowed-tools: "Bash, Read, Write, Grep"
truth_contract:
  canonical_sources:
    - docs/dev/HANDOFF_TEMPLATE.md
    - ops/state/truth/sources.json
  live_load_required:
    - "git rev-parse --show-toplevel"
    - "git branch --show-current"
    - "git rev-parse HEAD"
    - "git status --short"
  examples_only: []
  never_hardcode:
    - sprint number
    - current PR/issue state
    - required check set
    - session date
---

Write a handoff only when this session contains rationale, evidence, or partially completed work that would be expensive to reconstruct from durable repository surfaces.

A handoff is **memory, not authority**. It may record the state observed at session end as provenance, but the next session must requery every volatile fact before acting.

## 1. Decide whether a handoff is useful

Skip the handoff if Git/GitHub already preserve everything that matters, for example: "PR opened, checks green, merge pending." Do not create documentary churn just to restate queryable state.

Write one when the session contains at least one of:

- non-obvious reasoning or rejected alternatives;
- experiment/mutation evidence not otherwise persisted;
- unfinished local work or a multi-step investigation;
- unsafe assumptions the next session must know;
- a resume path spanning several durable surfaces.

## 2. Gather end-of-session provenance

```bash
REPO_ROOT="$(git rev-parse --show-toplevel)"
git branch --show-current
git rev-parse HEAD
git status --short
git rev-parse origin/main 2>/dev/null || true
date --iso-8601=seconds
```

If a specific PR/issue is relevant, query **that item** live. Do not dump the whole repository's PR list into the handoff.

## 3. Check durable promotion before writing memory

For each important discovery, ask whether it belongs in a durable surface:

- executable invariant/behavior -> test;
- semantic/architectural truth -> registered domain owner or ADR;
- defect/follow-up/control state -> issue/control surface;
- machine-readable operational state -> its registered owner;
- navigation/routing fact -> registry/source followed by regeneration.

Promote it first when appropriate. The handoff then links to that durable surface.

If something important exists only in the handoff, mark it explicitly as context-loss risk.

## 4. Write using the template

Use `docs/dev/HANDOFF_TEMPLATE.md`.

Preferred path:

```text
docs/dev/handoff-YYYY-MM-DD-<topic>.md
```

The opening warning that the file is historical/non-authoritative is mandatory.

At minimum include:

- Session scope and boundary
- Observed checkout at handoff time
- Durable truth consulted
- Evidence produced/inspected
- Work completed / not completed
- Unsafe assumptions
- Durable promotion check
- Suggested resume point with mandatory revalidation
- Reverification targets
- Closing classification

## 5. Do not encode false authority

Do not write statements like:

- "main is currently X" without an observation timestamp;
- "the next task is Y" as doctrine;
- "CI is green, merge it next session" without revalidation/authorization;
- "issue X remains open" as a future fact.

Write instead that these were **observed** at handoff time and name what must be requeried.

## 6. Push behavior

Do not commit the handoff automatically.

If `$ARGUMENTS` includes `--push`, the user has still authorized only the existing push workflow, not a merge/deploy. Follow the repository's canonical push skill/policy and do not include unrelated dirty files.

## Report

Report the handoff path plus:

- observed branch/HEAD;
- number of unsafe assumptions;
- any important discovery that could not be promoted to a durable surface;
- the volatile facts the next session must reverify.
