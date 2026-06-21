---
name: preflight
description: ICN session preflight. This skill should be used when the user explicitly invokes "/icn-agent-pack:preflight", or asks to "run preflight", "orient me on ICN", or "check the ICN session environment". Loads canonical docs and the latest handoff, then verifies branch, gh auth, ports, toolchain, and a light cargo check. Read-only; reports, never fixes.
disable-model-invocation: true
user-invocable: true
allowed-tools: "Bash, Read"
---

Plugin-namespaced session preflight. Load canonical truth, then verify the environment. Report only — do not fix anything, do not run `cargo build`. Stop on a fatal mismatch (wrong repo, no auth).

This is the portable companion to the project-local `/icn-preflight` skill in `.claude/skills/icn-preflight/`. It does not replace it. When both are present, either is fine; this one ships with the plugin so it works from any checkout.

## Step 0 — Load canonical truth (read, do not restate)

1. Read `docs/STATE.md` and `docs/PHASE_PROGRESS.md` (declared project state — canonical).
2. If present, read `docs/ai/ICN_CONSTITUTIONAL_CORE.md` (reasoning foundation — scan).
3. Find and read the latest handoff: `ls -t docs/dev/handoff-*.md 2>/dev/null | head -1`. If none exists (fresh clone), note "no prior handoff found" and continue.
4. Note any divergence between declared state and the handoff's execution state.
5. Generate the bounded **live-state overlay** for one-screen, source/freshness-bound grounding (no overclaims): `python3 scripts/generate-live-state-overlay.py --no-gh` (drop `--no-gh` to add live PR/issue state). Read it before planning repo work; do not cache it across sessions. Details: `docs/ai/ICN_LIVE_STATE_OVERLAY_TEMPLATE.md`.

## Step 1 — Verify environment (one line per check, prefix pass/warn/fail)

1. **Repo**: confirm `pwd` is the ICN monorepo root or a worktree of it. Print branch: `git branch --show-current`.
2. **gh auth**: `gh auth status` — FAIL and stop if not authenticated.
3. **Ports**: `ss -tlnp 2>/dev/null | grep -E ':8080|:8000'` — report whether the gateway is listening on 8080. WARN if anything is on 8000 (the gateway binds 8080, never 8000).
4. **Toolchain**: `rustc --version`, compared against `icn/rust-toolchain.toml` (read it; do not hardcode the version). WARN on mismatch.
5. **Light compile**: from the Rust workspace, `cargo check --workspace 2>&1 | tail -1`. The Rust workspace is `icn/` inside the monorepo root — `cd "$(git rev-parse --show-toplevel)/icn"` first. Report pass/fail from the last line.

## Output

```
/icn-agent-pack:preflight
  branch: <branch>
  gh: authenticated as <user>
  port 8080: <listening | not listening>
  port 8000: <clear | WARN occupied>
  toolchain: <version> (<matches | MISMATCH> rust-toolchain.toml)
  cargo check: <ok | fail>
  state: <one line on STATE.md / handoff divergence, if any>
```

Do NOT run `cargo build`. Do NOT modify anything. Report only.
