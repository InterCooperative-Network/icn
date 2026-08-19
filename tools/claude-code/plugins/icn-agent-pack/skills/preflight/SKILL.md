---
name: preflight
description: Portable ICN session grounding. Verifies checkout, loads the truth-owner map, optionally queries live GitHub state, and points to task-specific context without treating handoffs as authority.
disable-model-invocation: true
user-invocable: true
allowed-tools: "Bash, Read"
---

Portable companion to the repository-local `/icn-preflight` workflow.

This skill reports only. It does not fix, merge, build the whole workspace, or infer current project state from a handoff.

## 1. Resolve the repository

```bash
REPO_ROOT="$(git rev-parse --show-toplevel)" || exit 1
git branch --show-current
git rev-parse HEAD
git status --short
git rev-parse origin/main 2>/dev/null || true
```

Do not compare against a hardcoded machine path. When topology matters, read `ops/state/config/repo-map.json`.

## 2. Load the operating contract and truth map

Read:

- `AGENTS.md`
- `ops/state/truth/sources.json`

Resolve only the fact domains relevant to the user's task. Do not automatically load `docs/STATE.md`, `docs/PHASE_PROGRESS.md`, or the latest handoff.

## 3. Generate orientation

If available:

```bash
python3 scripts/generate-live-state-overlay.py
```

The overlay is owner-derived orientation. It is not canonical truth.

When target paths are known:

```bash
python3 scripts/generate-agent-context-spine.py --brief <paths>
```

Read the source/owner paths the tools point to rather than treating generated summaries as stronger evidence.

## 4. Live GitHub

When the task concerns an issue, PR, review, or CI state:

```bash
gh auth status
```

Then query the specific item live. Do not trust a handoff's old state.

## 5. Environment checks only when relevant

For Rust work, compare the active toolchain to `icn/rust-toolchain.toml` and run the smallest package-level check needed to establish the starting state.

Do not run a whole-workspace compile merely because preflight was invoked. Do not probe gateway ports unless the task concerns a live gateway/runtime.

## Output

```text
/icn-agent-pack:preflight
  checkout: <root> @ <head> (<branch>, clean/dirty)
  origin/main: <sha or not checked>
  truth domains: <owners relevant to task>
  live state: <specific item checked / not required / unavailable>
  path context: <brief paths / not required>
  evidence next: <smallest starting verification>
```

If the checkout, truth ownership, or required live access is materially ambiguous, stop and report the ambiguity before mutation.
