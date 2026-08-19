---
Status: template
Authority: agent process
Canonical: no
Last verified: 2026-08-19
---

# ICN Handoff Template

A handoff is a **timestamped memory and evidence packet** for resuming work. It is not a current-state document and must never be treated as one by a later session.

Use a handoff when rationale, experiment results, or a partially completed sequence would be expensive to rediscover. Do not create one merely to restate GitHub or the current branch.

````markdown
# Session Handoff — YYYY-MM-DD — <topic>

> **Memory status:** historical session evidence only. Reverify every branch, PR, CI, issue, and blocker claim before acting on it.

## Session scope

**Goal:** <what this session attempted>

**Boundary:** <what was deliberately not attempted>

## Observed checkout at handoff time

- repo root: `<path or repo identifier>`
- branch: `<branch>`
- HEAD: `<sha>`
- observed `origin/main`: `<sha or not checked>`
- working tree: `<clean / concise dirty state>`
- observed at: `<timestamp>`

These values are provenance for this handoff, not instructions for the next session.

## Durable truth consulted

| Domain/question | Owner consulted | Relevant conclusion |
|---|---|---|
| <domain> | <registered owner path> | <bounded conclusion> |

## Evidence produced or inspected

- `<command/test/query>` → `<result>`
- `<code/path inspection>` → `<result>`

Include exact artifacts, SHAs, test names, or issue/PR links when they make the conclusion reproducible.

## Work completed

1. <change, commit, PR, issue update, or analysis result>
2. ...

## Work not completed

- <unfinished item and why>
- <external dependency or decision still required>

## Decisions and rationale

Record only reasoning that would otherwise be lost.

- <decision> — <why, alternatives rejected, evidence>

If the decision is durable project truth, also name the ADR/domain owner/issue where it was promoted. A handoff alone does not ratify it.

## Unsafe assumptions

- <claim relied on but not independently verified>
- <environment or external-state assumption>

Write `None known` only after considering this explicitly.

## Durable promotion check

For every important discovery, answer where it now lives:

| Discovery | Durable surface | Promoted? |
|---|---|---|
| <invariant/defect/decision/current-control fact> | <test / owner / ADR / issue / registry / none> | <yes/no + why> |

Anything important whose durable surface is `none` is context-loss risk.

## Suggested resume point

This is a **recommendation, not current truth**.

1. Reverify checkout and `origin/main`.
2. Requery the linked issue/PR/reviews/checks.
3. Re-resolve the relevant truth owner(s).
4. If the premise still holds, <suggested next action>.

## Reverification targets

The next session must recheck at least:

- `<PR / issue / branch / CI / runtime claim>`
- `<other volatile dependency>`

## Files and surfaces touched

- `<path>` — <why>

## Closing classification

- Work level: <analysis-only / docs / test/library-only / implementation / integration / migration / deployment>
- Semantic contract changed: <yes/no; owner or proposal if yes>
- Merge performed: <yes/no>
- Deploy/release/migration performed: <yes/no>
````

## Rules

- Prefer `docs/dev/handoff-YYYY-MM-DD-<topic>.md` with a descriptive topic.
- Do not automatically commit or push a handoff unless the maintainer's workflow explicitly asks for it.
- Never copy a live PR table or sprint board merely to make the handoff look complete. Link/queryable state is better than a stale snapshot unless the snapshot is evidence for a specific conclusion.
- Never say "next session should merge" without the required revalidation and merge authorization caveat.
- If the only useful content is "PR is open, CI is green," skip the handoff. GitHub already remembers that better.
- Handoffs are deliberately safe to archive or delete without destroying project truth.
