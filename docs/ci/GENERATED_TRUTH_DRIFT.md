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
- **Promotion candidates.** `check-agent-context-spine.py`,
  `check-claude-plugin.py`, and `check-claude-plugin-root-resolution.py` are
  green-on-main and deterministic; they are candidates to graduate from
  observational to blocking once a clean CI baseline is confirmed and they are
  added to branch protection.
