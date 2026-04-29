# Cross-Repo PR Stack Protocol

Coordinates pull requests across ICN, NYCN, and ICN Academy when a
single idea, concept, or feature touches more than one repo.

> **Order matters.** Stacks merged out of order produce stale PR
> bodies, broken cross-references, and review-thread confusion. This
> protocol prevents that by fixing the merge order.

## Cross-repo merge order

For any change that spans more than one repo:

1. **ICN canonical first.** Generic primitives, design direction
   docs, ADRs, RFCs, runtime, and shared types land in
   [`InterCooperative-Network/icn`](https://github.com/InterCooperative-Network/icn).
2. **NYCN application second.** Institution-specific application,
   package shapes, and operating material land in
   [`InterCooperative-Network/nycn`](https://github.com/InterCooperative-Network/nycn).
3. **ICN Academy teaching third.** Cross-role orientation packets,
   role packets, tracks, and teaching surfaces land in
   [`InterCooperative-Network/icn-learn`](https://github.com/InterCooperative-Network/icn-learn).

Reasons:

- ICN canonical wording is what the other repos cite. Citing a
  not-yet-merged ICN PR produces broken or stale links.
- Teaching surfaces are downstream of canonical truth. A packet that
  cites a canonical doc that has not landed will go stale on the
  first edit.
- Public website changes are downstream of all three (per
  ADR-0032/-0033) and are gated separately on evidence, not on the
  stack.

## When to deviate

Almost never. Two cases:

- **Hot fix in NYCN that does not depend on an ICN change.** Fine
  to merge alone.
- **Pure repo-hygiene change in icn-learn.** Fine to merge alone.

If the deviation requires a citation back to the canonical ICN doc,
the deviation does not apply — wait for the ICN PR.

## PR body discipline

Every PR body must include:

1. **Summary.** What this PR does in one or two sentences.
2. **Layer classification.** ICN core / ICN app / NYCN package /
   icn-learn / website / private overlay.
3. **Boundary check.** What stays out of this PR (the explicit
   non-goals — institution-specific meaning out of ICN core; private
   data out of any repo; etc.).
4. **What changed.** Files touched and why.
5. **What did not change.** Especially adjacent surfaces that a
   reader might expect to change but should not.
6. **Evidence / validation.** Commands run, gates that passed.
7. **Issue status.** Use `Refs #NNNN`. **Never** use `Fixes #NNNN`,
   `Closes #NNNN`, or `Resolves #NNNN` near issue numbers unless you
   are intentionally closing the issue. GitHub auto-close on those
   keywords is a recurring source of accidental issue closes.
8. **Review-thread status.** Whether prior review threads are
   resolved or marked outdated, with reason.
9. **Cross-repo dependency status.** Names the upstream/downstream
   PRs in the stack and whether this PR depends on them merging
   first.

## The "Refs vs close-keyword" warning

GitHub treats the following keywords (case-insensitive) followed by
a `#NNNN` reference as auto-close instructions when the PR merges:

- `close`, `closes`, `closed`
- `fix`, `fixes`, `fixed`
- `resolve`, `resolves`, `resolved`

If you do not intend to close the issue, **always** use `Refs #NNNN`
or write the relationship in plain prose ("relates to issue #NNNN",
"part of #NNNN").

This is enforced by convention, not by tooling. Reviewers should
flag any PR body that uses an auto-close keyword unless the closure
is intentional and noted in the summary.

## Stale PR body detection

PR bodies decay during long review cycles. Before merge:

- Re-read the PR body against the actual diff.
- Confirm every "Files added" / "Files changed" entry still matches
  the current commits.
- Update wording for any in-flight reference (e.g. "PR #1663 in
  flight") to the current state ("PR #1663 merged 2026-04-27").
- If the PR body claims a non-goal that the diff has since violated,
  fix one or the other before merge.

## Mid-stack drift

If you discover that the canonical ICN wording shifts during the
stack (for example, ADR alignment changes the term used in a
companion repo), update the dependent PRs **before** merging the
canonical PR. Otherwise the dependent PRs land citing stale wording.

## Idea refinery interaction

Ideas in `ops/ideas/ideas.yaml` are pre-RFC. They may promote into a
PR stack via promotion review. When a single idea promotes to
multiple repos (e.g. a generic ICN primitive plus a NYCN dogfood
slice plus an icn-learn packet), the stack follows this protocol.

## Reference

- [`ops/coordination/README.md`](README.md) — the post-promotion
  pipeline (RFC → ADR → issue → tests → website).
- [`ops/ideas/README.md`](../ideas/README.md) — the pre-RFC
  refinery.
- [ADR-0032 — Website Truth Boundary](../../docs/adr/ADR-0032-website-truth-boundary.md)
- [ADR-0033 — Public Maturity Claims and Evidence Links](../../docs/adr/ADR-0033-public-maturity-claims-and-evidence-links.md)
