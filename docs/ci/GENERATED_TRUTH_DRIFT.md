# Generated-truth drift gate

**Status Type**: Operational reference (not a canonical truth source)
**Workflow**: [`.github/workflows/generated-truth.yml`](../../.github/workflows/generated-truth.yml)

## Purpose

ICN's generated **orientation / truth-layer** artifacts (the agent context spine,
the repo file-record snapshot, the Claude agent pack, and the live-state overlay
generator) are reference aids that agents and developers navigate by. They are
**`Canonical: no`** — they are NOT truth roots. The canonical project state
remains [`docs/STATE.md`](../STATE.md) + [`docs/PHASE_PROGRESS.md`](../PHASE_PROGRESS.md);
everything below orients toward those and must not exceed them.

Because these artifacts are generated, they can silently diverge from source
between manual regenerations. This gate makes that drift **visible** in CI. It
mirrors the existing route-inventory guard in
[`docs-freshness.yml`](../../.github/workflows/docs-freshness.yml): drift is a
**warning**, not a merge blocker.

## What is checked

| Check (script) | Artifact / invariant | Regenerate | Current posture |
|---|---|---|---|
| `scripts/check-agent-context-spine.py` | `docs/reference/project-index/generated/agent-context-spine.json` is structurally valid and matches a fresh generation | `python3 scripts/generate-agent-context-spine.py --write` | green on main |
| `scripts/generate_repo_record.py --repo icn=. --check` | `docs/reference/project-index/generated/icn-file-record.{json,md}` matches the working tree (timestamp/branch ignored) | `python3 scripts/generate_repo_record.py --repo icn=. --out docs/reference/project-index/generated` | **stale-warn** (periodic snapshot; see below) |
| `scripts/check-claude-plugin.py` | Claude agent pack skeleton is portable (no machine paths) | n/a (validation only) | green on main |
| `scripts/check-claude-plugin-root-resolution.py` | Agent-pack repo-root resolver behaves across worktree layouts | n/a (validation only) | green on main |
| `scripts/generate-live-state-overlay.py --check --no-gh` | Live-state overlay generator self-check (13 sections, claim discipline). The overlay is **on-demand with NO committed snapshot**, so this is a runnable-smoke, not a drift check. | n/a (never committed) | green on main |
| `scripts/check-state-lag.py` | **Canonical-state lag** — the *newest* `docs/STATE.md` sync block asserts nothing as "open / not merged / not on `main`" that git history shows merged (the #2128 recurrence). Not a generated artifact: it guards the canonical doc against stale not-merged claims. STATE.md is append-only, so only the current view is checked; `--all` audits history. | n/a (refresh the STATE.md sync block) | green on main |

## Rule: every committed generated artifact is covered or declared on-demand

When you add a new **generated** artifact (an orientation/reference file produced
by a script, not hand-authored), it MUST do one of:

1. **Be covered here.** Add a row to the table above, add its `--check` (or
   validator) step to [`generated-truth.yml`](../../.github/workflows/generated-truth.yml),
   and add its path to that workflow's `paths:` triggers. A committed generated
   artifact with no drift check silently rots.
2. **Be declared on-demand / no committed snapshot.** If the artifact is
   regenerated fresh each use and never committed (like the live-state overlay),
   say so explicitly in its generator docstring and in the table above, and gate
   only a runnable-smoke (`--check`) — there is no snapshot to drift.

Do not commit a generated artifact that is neither checked nor declared
on-demand. Reviewers should reject a new `docs/**/generated/**` file that adds
no corresponding entry here. (This rule is documentation, not yet an automated
check; a `paths:`-scoped CI assertion that new `generated/**` files appear in
this table is a reasonable future tightening.)

## Blocking vs observational

**All checks are observational** in this gate (v1). The job emits `::warning::`
on drift and **succeeds**; it only **fails** if a checker itself errors (e.g.
`generate_repo_record.py --check` exits ≥2, distinct from `1`=stale). The
workflow is not a branch-protection–required check, so it does not block merge.

This conservative start matches the repo convention for orientation artifacts
(route-inventory is warn-only — issue #2112). See
[`GATE_RATCHET_PLAN.md`](GATE_RATCHET_PLAN.md) for how observational checks
graduate to enforceable gates.

## Regenerating artifacts

```bash
# Agent context spine
python3 scripts/generate-agent-context-spine.py --write

# Repo file-record snapshot (generated-only commit; do not hand-edit)
python3 scripts/generate_repo_record.py --repo icn=. --out docs/reference/project-index/generated

# Live-state overlay — on-demand only, NEVER commit a snapshot
python3 scripts/generate-live-state-overlay.py            # markdown to stdout
```

## Known follow-ups (out of scope for the gate itself)

- **File-record snapshot is periodically stale.** It is a point-in-time
  snapshot refreshed by a dedicated generated-only commit (e.g. #2126/#2130),
  not on every PR. The observational check flags when it lags `main`; refreshing
  it is a separate commit, intentionally not bundled here.
- **Spine subsystem coverage is partial.** The agent-context-spine currently
  derives crate→subsystem membership for ~7 of the ~14 subsystems the live-state
  overlay lists (the rest are curated v0 with no spine `owned_by_subsystem`
  node). Completing that derivation is a separate lane.
- **Promotion candidates (baseline confirmed clean 2026-06-21).** The
  PR-stable deterministic checks — `check-agent-context-spine.py`,
  `check-claude-plugin.py`, `check-claude-plugin-root-resolution.py`,
  `generate-live-state-overlay.py --check`, and `docs/scripts/route_inventory.py
  --check` — all exit 0 on `main` as of 2026-06-21, so the clean baseline this
  doc asked for now holds. They are ready to graduate from observational to
  blocking. **Promotion is a branch-protection (repo-settings) change and a
  cadence decision — deliberately NOT taken in this docs pass.**
  - **NOT a promotion candidate:** `generate_repo_record.py --check`. The
    file-record snapshot inventories per-file SHAs, so it drifts on *any* PR
    that touches an inventoried file and is refreshed by a separate
    generated-only commit (the #2126/#2130 cadence) — making it stale-warn by
    design, not PR-stable. Promoting it to blocking would red-flag every normal
    PR until a snapshot refresh. It stays observational.
  - Doc freshness (`freshness-check.py`) stays **advisory** until the repo
    explicitly decides to make it blocking.
  - `check-state-lag.py` is the newest observational check and should bake on
    `main` before it is considered for promotion.
