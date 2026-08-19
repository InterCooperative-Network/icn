---
name: preflight
description: Minimal read-only ICN checkout grounding. Verifies repo/branch/HEAD/dirty state and truth-owner availability without ritual workspace builds.
user-invocable: true
allowed-tools: "Bash, Read, Grep, Glob"
truth_contract:
  canonical_sources:
    - AGENTS.md
    - ops/state/truth/sources.json
    - ops/state/config/repo-map.json
  live_load_required:
    - "git rev-parse --show-toplevel"
    - "git branch --show-current"
    - "git rev-parse HEAD"
    - "git status --short"
  examples_only: []
  never_hardcode:
    - current phase
    - current PR/issue list
    - toolchain version
    - cluster addresses
---

Run a **minimal, read-only** session grounding check. Use `icn-preflight` when the task needs the full owner/live-state/path-context session frame.

## Steps

1. Verify the checkout:

```bash
REPO_ROOT="$(git rev-parse --show-toplevel)" || exit 1
git branch --show-current
git rev-parse HEAD
git status --short
git rev-parse origin/main 2>/dev/null || true
```

2. Confirm these foundational files exist and are readable:

- `AGENTS.md`
- `ops/state/truth/sources.json`
- `ops/state/config/repo-map.json`

3. If the task already names a PR/issue or requires GitHub, run `gh auth status` and query that specific item. Otherwise do not dump repository-wide live state.

4. If the task is about a runtime/service, inspect the relevant listener/service then. Do not probe ports by default.

5. If the task will build Rust, defer compile/toolchain verification to the affected package/path after context is resolved. Do not run `cargo check --workspace` merely because a session started.

## Output

Report in five compact bullets:

- repo root;
- branch + HEAD;
- working tree clean/dirty;
- observed `origin/main` or not available;
- truth-owner files present + any task-specific live item checked.

Do not modify anything.
