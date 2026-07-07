# Stack PR body template

Copy into the PR description and fill every section — all nine are required by
[`ops/coordination/PR_STACK_PROTOCOL.md`](../PR_STACK_PROTOCOL.md). Use `Refs #NNNN`
only; never `close`/`fix`/`resolve` near an issue number unless closure is
intended and stated in the summary.

```markdown
## Summary
<what this PR does, one or two sentences>

## Layer classification
<ICN core / ICN app / ops coordination / NYCN package / icn-learn / website / private overlay>

## Boundary check / non-goals
- <what deliberately stays out of this PR>

## What changed
- <files touched and why>

## What did not change
- <adjacent surfaces a reader might expect to change but should not>

## Validation evidence
- <commands run and their results — cite, don't summarize from memory>

## Issue status
Refs #NNNN — left open; closure is the issue owner's call.

## Review-thread status
<none yet / resolved with reason / outdated with reason>

## Cross-repo dependency status
- Upstream: <repo PR/commit + status: planned | implemented | reviewed | merged | adopted>
- Downstream: <repo + status — say "planned, not started" when that is the truth>

## Review notes / known warnings
- <known warnings and why they are acceptable>
```
