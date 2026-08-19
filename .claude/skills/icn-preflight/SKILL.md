---
name: icn-preflight
description: Ground an ICN session in the correct checkout, registered truth owners, live execution state, and task-specific verification.
user-invocable: true
allowed-tools: "Bash, Read"
truth_contract:
  canonical_sources:
    - AGENTS.md
    - ops/state/truth/sources.json
    - ops/state/truth/policy.json
    - ops/state/config/repo-map.json
  live_load_required:
    - "git rev-parse --show-toplevel"
    - "git branch --show-current"
    - "git rev-parse HEAD"
    - "git status --short"
  examples_only: []
  never_hardcode:
    - current phase
    - active issue or PR numbers
    - required CI check set
    - cluster addresses
    - toolchain version
---

Ground the session before non-trivial work. This skill **does not load a universal project-state narrative** and does not treat the latest handoff as current state.

## 1. Verify the checkout

```bash
REPO_ROOT="$(git rev-parse --show-toplevel)" || exit 1
printf 'repo=%s\n' "$REPO_ROOT"
git branch --show-current
git rev-parse HEAD
git status --short
git rev-parse origin/main 2>/dev/null || true
```

If the worktree is unexpected or dirty in a way that could collide with the task, stop and report it before mutation.

Read `ops/state/config/repo-map.json` when you need to determine whether this path is a canonical worktree or legacy checkout. Do not compare against a hardcoded home-directory path.

## 2. Load the operating contract and ownership map

Read, do not restate:

- `AGENTS.md`
- `ops/state/truth/sources.json`

Identify the fact domains relevant to the user's actual task and load only those registered owners.

Do not automatically load `docs/STATE.md`, `docs/PHASE_PROGRESS.md`, or the newest handoff. They are consulted only when the question being answered actually requires them as evidence/history.

## 3. Refresh live execution state if relevant

Check GitHub authentication when the task needs GitHub:

```bash
gh auth status
```

For an existing PR/issue, query that exact item and its reviews/checks. For work with no existing PR/issue, do not manufacture a repository-wide status dump.

Live PR, issue, review, and CI state always outrank an old handoff or prompt snapshot.

## 4. Load path context

When paths are known, prefer:

```bash
python3 scripts/generate-agent-context-spine.py --brief <paths>
```

Use the brief to discover relevant crates/docs/checks/boundaries, then inspect the source files it points to.

## 5. Toolchain/environment checks are task-specific

If Rust will be built or changed:

```bash
cd "${REPO_ROOT}/icn"
rustc --version
cargo check -p <affected-package>
```

Compare the active toolchain with `icn/rust-toolchain.toml`; do not hardcode the expected version in this skill.

Do not run a workspace compile merely because a session started. Do not probe gateway ports unless the task is about a live gateway/runtime. Preflight should establish relevance, not consume resources ritualistically.

## 6. Produce the compact session frame

Use `docs/ai/ICN_SESSION_FRAME_TEMPLATE.md` and record at minimum:

- task and scope boundary;
- checkout/HEAD/dirty state;
- truth domains and owners loaded;
- live execution state queried, if any;
- smallest evidence checked or still needed;
- invariant/compatibility risk;
- authorization boundary.

## Output

Keep the summary compact:

```text
ICN preflight
  checkout: <root> @ <head> (<branch>, clean/dirty)
  origin/main: <sha or not checked>
  truth domains: <owners loaded>
  live state: <specific PR/issue checked or not required>
  path context: <brief paths or not required>
  evidence next: <smallest verification needed>
  mutation: <authorized / review-only / unclear>
```

Stop on a material checkout/truth-owner/auth mismatch. Do not fix anything during preflight.
