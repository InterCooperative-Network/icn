---
name: icn-preflight
description: Session startup — load canonical docs, latest handoff, produce session frame, then verify repo/branch/auth/toolchain/compile.
user-invocable: true
allowed-tools: "Bash, Read"
truth_contract:
  canonical_sources:
    - docs/ai/ICN_CONSTITUTIONAL_CORE.md  # reasoning foundation
    - docs/STATE.md                        # declared project state
    - docs/PHASE_PROGRESS.md               # phase tracking
    - docs/dev/                            # latest handoff by date
  live_load_required:
    - "git branch --show-current"
    - "gh auth status"
    - "rustc --version"
  examples_only: []
  note: "This skill reads rust-toolchain.toml for the expected toolchain version. Do not hardcode the version."
---

Session startup. Load canonical docs, produce session frame, then verify environment. Stop on first mismatch.

## Step 0 — Load canonical docs and latest handoff

1. Read `docs/ai/ICN_CONSTITUTIONAL_CORE.md` (reasoning foundation — scan, do not restate).
2. Read `docs/STATE.md` and `docs/PHASE_PROGRESS.md` (declared project truth).
3. Find and read the latest handoff: `ls -t docs/dev/handoff-*.md | head -1`.
4. Note any divergences between declared state and handoff execution state.

## Step 0.5 — Produce abbreviated session frame

Using the template from `docs/ai/ICN_SESSION_FRAME_TEMPLATE.md`, fill in at minimum:
- Task (from user or inferred)
- Branch
- Current canonical phase
- Current execution target
- Main risk

This frame is required for non-trivial work. Skip only for single-file typo fixes.

## Steps

1. **Repo**: confirm `pwd` is under `icn/` or a worktree. Print branch via `git branch --show-current`.
2. **gh auth**: `gh auth status` — stop if not authenticated.
3. **Port**: `ss -tlnp 2>/dev/null | grep :8080` — report if gateway is listening. Warn if anything is on 8000.
4. **Toolchain**: `rustc --version` — compare against `icn/rust-toolchain.toml`. Warn on mismatch.
5. **Compile**: `REPO_ROOT="$(git rev-parse --show-toplevel)" && cd "${REPO_ROOT}/icn" && cargo check --workspace 2>&1 | tail -1` — report pass/fail.

## Output

One line per check. Prefix each with pass/fail/warn symbol. Stop on fatal mismatch (wrong repo, no auth). Example:

```
/icn-preflight
  branch: main
  gh: authenticated as mattdlong
  port 8080: not listening (gateway not running)
  port 8000: clear
  toolchain: 1.88.0 (matches rust-toolchain.toml)
  cargo check: ok
```

Do NOT run `cargo build`. Do NOT fix anything. Report only.
