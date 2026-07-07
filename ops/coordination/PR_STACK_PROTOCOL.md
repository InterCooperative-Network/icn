# Cross-Repo PR Stack Protocol

Coordinates pull requests across the InterCooperative-Network org
repos when a single idea, concept, or feature touches more than one
repo.

> **Order matters.** Stacks merged out of order produce stale PR
> bodies, broken cross-references, and review-thread confusion. This
> protocol prevents that by fixing the merge order.

## Cross-repo merge order

For any change that spans more than one repo, the full ecosystem
order is:

1. **`icn` — canonical first.** Generic primitives, design direction
   docs, ADRs, RFCs, runtime, shared types, and canonical wording
   land in
   [`InterCooperative-Network/icn`](https://github.com/InterCooperative-Network/icn).
2. **`icn-infra` — org contracts second.** Cross-repo boundary
   definitions, public-claims gates, and source classification bind
   what the rest of the org may say and ship.
3. **`nycn` — institution application third.** Institution-specific
   application, package shapes, and operating material, pinned to
   icn via a human-signed upstream lock.
4. **`icn-learn` — teaching fourth.** Orientation packets, role
   packets, tracks, and teaching surfaces cite canonical truth and
   go stale if they land before it.
5. **`icn-community-bridge` — outreach fifth.** Onboarding/outreach
   scaffold, gated by icn-infra operating contracts; never canonical.
6. **`.github` — mirror last.** The org profile mirrors public truth
   that already landed in icn; it originates nothing.

Reasons:

- ICN canonical wording is what the other repos cite. Citing a
  not-yet-merged ICN PR produces broken or stale links.
- Teaching and outreach surfaces are downstream of canonical truth.
  A packet that cites a canonical doc that has not landed will go
  stale on the first edit.
- Public website changes are downstream of all of the above (per
  ADR-0032/-0033) and are gated separately on evidence, not on the
  stack.

Stage numbers are the **default** dependency direction: lower stages
are upstream of higher ones. A specific stack may skip stages (no
work in that repo) or declare a narrower dependency set in its
manifest — `depends_on_stages` makes explicit what the numbering
leaves implicit. Example: a dashboard that consumes downstream
status envelopes legitimately depends on stages 3–5 even though it
lives at stage 2.

## Stack status vocabulary

Every stack PR and manifest stage distinguishes, and never
conflates:

| Status | Means | Does NOT mean |
|---|---|---|
| `planned` | scoped, not started | done, or even started |
| `implemented` | change exists on a branch/PR | reviewed or merged |
| `reviewed` | review completed | merged |
| `merged` | landed on that repo's default branch | adopted downstream |
| `adopted` | a downstream consumer actually uses it (caller workflow on its default branch, lock citing the new ref) | anything about runtime |
| `none` | a stage in the order with no work planned in this stack | that the stage/repo is irrelevant in general |

These are the only values a manifest stage's `status` may take;
`scripts/check-pr-stack.sh` rejects any other. A partly-done stage (some
rungs merged, more in flight) is `implemented`, not `merged` — `merged`
means the whole stage is done.

No readiness claim may be inferred from any of these. A stack stage
existing — or reaching `merged` or `adopted` — says nothing about
runtime state, deployment, pilots, or live federation. Claim
discipline belongs to the claim firewall (readiness-overclaim
linter, PUBLIC-CLAIMS gates), not to this protocol.

## Stack manifests (`stacks/`)

Live cross-repo stacks are recorded as machine-readable manifests:
`ops/coordination/stacks/<slug>.stack.yaml`, format `icn-pr-stack/v1`.
A manifest encodes one stack's stage order, per-stage status and
dependencies, known PRs, and dependency policy.

Manifests are **coordination metadata, not architectural or runtime
truth**. `ops/state/truth/sources.json` remains the arbiter of truth
ownership: it names this directory as owner of the
`cross_repo_pr_stacks` domain — merge order and stack state, nothing
else. On any conflict, sources.json wins.

Validate manifests with:

```bash
bash scripts/check-pr-stack.sh --repo-root . --offline   # shape + ordering, no network
bash scripts/check-pr-stack.sh --repo-root .             # + gh-backed PR/issue checks
bash scripts/check-pr-stack.sh --repo-root . --strict    # findings become failures
```

The checker is run manually and from VM sessions, not wired into CI
(as of the PR that introduced it): the checks that matter most —
close-keyword and Refs discipline in PR bodies across private
downstream repos — need an authenticated cross-repo `gh` context
that public CI does not have, and wiring only the offline subset
would read as more enforcement than it is. This protocol is a
planner plus checker, not an enforcement gate. Wiring the offline
subset into the drift workflow is a possible later ratchet.

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
2. **Layer classification.** ICN core / ICN app / ops coordination /
   NYCN package / icn-learn / website / private overlay.
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
   PRs in the stack, states each one's status using the stack status
   vocabulary above, and says whether this PR depends on them
   merging first.

Template: [`templates/stack-pr-body-template.md`](templates/stack-pr-body-template.md).

## The "Refs vs close-keyword" warning

GitHub treats the following keywords (case-insensitive) followed by
a `#NNNN` reference as auto-close instructions when the PR merges:

- `close`, `closes`, `closed`
- `fix`, `fixes`, `fixed`
- `resolve`, `resolves`, `resolved`

If you do not intend to close the issue, **always** use `Refs #NNNN`
or write the relationship in plain prose ("relates to issue #NNNN",
"part of #NNNN").

This is enforced by convention, with `scripts/check-pr-stack.sh` as
a backstop for manifests and manifest-listed PR bodies. Reviewers
should still flag any PR body that uses an auto-close keyword unless
the closure is intentional and noted in the summary.

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

- [`ops/coordination/stacks/`](stacks/) — live stack manifests
  (`icn-pr-stack/v1`), owner of the `cross_repo_pr_stacks` truth
  domain.
- [`ops/coordination/templates/`](templates/) — stack PR body and
  handoff templates.
- [`ops/coordination/README.md`](README.md) — the post-promotion
  pipeline (RFC → ADR → issue → tests → website).
- [`ops/ideas/README.md`](../ideas/README.md) — the pre-RFC
  refinery.
- [ADR-0032 — Website Truth Boundary](../../docs/adr/ADR-0032-website-truth-boundary.md)
- [ADR-0033 — Public Maturity Claims and Evidence Links](../../docs/adr/ADR-0033-public-maturity-claims-and-evidence-links.md)
