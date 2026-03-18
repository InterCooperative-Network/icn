---
name: icn-preflight
description: 30-60s session startup check — repo, branch, gh auth, port 8080, toolchain, cargo check. Stop on mismatch.
user-invocable: true
allowed-tools: "Bash, Read"
---

Quick preflight. 10 lines max output. Stop on first mismatch.

## Steps

1. **Repo**: confirm `pwd` is under `icn/` or a worktree. Print branch via `git branch --show-current`.
2. **gh auth**: `gh auth status` — stop if not authenticated.
3. **Port**: `ss -tlnp 2>/dev/null | grep :8080` — report if gateway is listening. Warn if anything is on 8000.
4. **Toolchain**: `rustc --version` — compare against `icn/rust-toolchain.toml`. Warn on mismatch.
5. **Compile**: `cd icn/icn && cargo check --workspace 2>&1 | tail -1` — report pass/fail.

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
