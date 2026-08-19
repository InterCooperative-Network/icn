# Generated-orientation drift gate

**Status Type**: Operational reference (not a canonical truth source)  
**Workflow**: [`.github/workflows/generated-truth.yml`](../../.github/workflows/generated-truth.yml)

## Purpose

ICN has generated **orientation/reference artifacts** that agents and developers use to navigate the repository. They are projections, not truth roots.

Their inputs/owners are resolved through [`ops/state/truth/sources.json`](../../ops/state/truth/sources.json), current repository content, and live Git/GitHub where appropriate. There is **no universal canonical project-state document** behind this gate.

Because generated artifacts can silently diverge from their inputs, this workflow makes that drift visible. It also retains one observational check over the legacy `docs/STATE.md` narrative so an existing sync block does not contradict merge history. That check protects historical/current narrative consistency; it does not make `STATE.md` a universal owner again.

## What is checked

| Check | Artifact / invariant | Regenerate / response |
|---|---|---|
| `scripts/check-agent-context-spine.py` | Agent Context Spine is structurally valid and matches a fresh generation | `python3 scripts/generate-agent-context-spine.py --write` |
| `scripts/generate_repo_record.py --repo icn=. --check` | Periodic file-record snapshot matches the working tree | regenerate in a dedicated generated-only update |
| `scripts/check-claude-plugin.py` | Claude agent-pack skeleton remains portable | fix plugin source |
| `scripts/check-claude-plugin-root-resolution.py` | Plugin repo-root resolver works across supported layouts | fix resolver/tests |
| `scripts/generate-live-state-overlay.py --check --no-gh` | Owner-derived live-overlay generator is structurally valid and offline mode invents no GitHub state | fix generator; overlay output itself is never committed |
| `scripts/check-state-lag.py` | Legacy `docs/STATE.md` newest sync block does not assert "not merged/not on main" for work git history proves merged | repair that narrative block; do not infer broader state ownership |

## Generated artifact rule

Every committed generated orientation/reference artifact must either:

1. have a reproducible freshness/validation check; or
2. be explicitly declared on-demand with no committed snapshot.

The live-state overlay is in category 2. It is generated fresh, includes observation time, and labels its handoff pointer as memory-only.

## Blocking versus observational

This workflow is an **observational drift lane**. Staleness normally emits warnings; a checker that cannot execute/parse its inputs is a real workflow failure.

Merge policy/branch protection is owned by `ops/state/truth/policy.json` plus live repository settings. Do not infer required-check status from this document.

## Regenerating / validating

```bash
# Agent Context Spine
python3 scripts/generate-agent-context-spine.py --write

# Repo file-record snapshot
python3 scripts/generate_repo_record.py --repo icn=. --out docs/reference/project-index/generated

# Owner-derived live-state overlay: on demand, never commit the output
python3 scripts/generate-live-state-overlay.py
python3 scripts/generate-live-state-overlay.py --check --no-gh

# Historical STATE narrative consistency check
python3 scripts/check-state-lag.py
```

## Interpretation

- Agent Context Spine / repo records tell you **where things are**, not what all facts mean.
- The live overlay tells you **which owners/live sources to query**, not a curated worldview.
- A green `check-state-lag.py` says only that the scanned `STATE.md` text does not contradict known merge history in the narrow way the checker detects.
- None of these checks upgrades a subsystem from implemented to integrated/deployed/production-ready.

## Follow-up direction

As the modern owner model matures, prefer checks that compare generated projections directly with their registered inputs. Retain legacy narrative checks only while those narratives remain active enough that contradictory text would mislead readers.
