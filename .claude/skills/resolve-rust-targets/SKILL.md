---
name: resolve-rust-targets
description: Resolve Rust package names and verification scope from cargo metadata. Never guess package IDs.
argument-hint: "[file-path | package-name | --touched]"
user-invocable: true
allowed-tools: "Bash"
---

Use `cargo metadata` as the authoritative source for package names, manifests, and verification scope.
Run before clippy, test, or build commands when the target package is even slightly uncertain.

## The Problem This Solves

The transcript hit `ledger` → `icn-ledger-app` → `icn-ledger-actor` before landing on the right package.
That cost two dead-end compile attempts. `cargo metadata` answers this in one call.

## Steps

### Mode 1: Resolve from changed files (`$ARGUMENTS` is `--touched` or empty)

```bash
# Get files changed on this branch
git diff --name-only $(git merge-base HEAD origin/main)..HEAD

# Get all workspace package names and their root dirs
cargo metadata --no-deps --format-version 1 \
  | python3 -c "
import sys, json
md = json.load(sys.stdin)
for p in md['packages']:
    root = p['manifest_path'].replace('/Cargo.toml','')
    print(f\"{p['name']:40s}  {root}\")
" | sort
```

Then for each changed file, find its containing package by matching path prefix to manifest root.

### Mode 2: Resolve a human label (`$ARGUMENTS` is a name like "ledger")

```bash
cargo metadata --no-deps --format-version 1 \
  | python3 -c "
import sys, json, difflib
name = '$ARGUMENTS'
md = json.load(sys.stdin)
packages = [(p['name'], p['manifest_path']) for p in md['packages']]
# Exact match first
exact = [p for p in packages if p[0] == name]
if exact:
    print('EXACT:', exact[0][0])
else:
    # Near-match suggestions
    names = [p[0] for p in packages]
    close = difflib.get_close_matches(name, names, n=5, cutoff=0.4)
    for c in close:
        m = next(p for p in packages if p[0] == c)
        print(f'NEAR:  {m[0]:40s}  {m[1]}')
"
```

### Mode 3: Generate verification commands for a set of packages

Given a list of resolved package names, produce the minimal cargo command set:

```bash
# For scoped verification (preferred — faster):
cargo clippy -p <pkg1> -p <pkg2> --all-targets -- -D warnings
cargo test -p <pkg1> -p <pkg2>

# For cross-cutting changes (bins, icnd, core):
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

## Package taxonomy (ICN workspace)

| Location | Pattern | Example |
|----------|---------|---------|
| `icn/crates/<name>/` | `icn-<name>` | `icn-ledger`, `icn-obs`, `icn-security` |
| `icn/apps/<name>/` | `icn-<name>-actor` or `icn-<name>-app` | `icn-ledger-actor`, `icn-governance-actor` |
| `icn/bins/<name>/` | `<name>` | `icnd`, `icnctl`, `icn-console` |

Common confusions from the transcript:
- `"ledger"` → `icn-ledger` (crate) or `icn-ledger-actor` (app) — both exist
- `"ledger app"` → `icn-ledger-actor`
- `"icn-ledger-app"` → does not exist; nearest is `icn-ledger-actor`
- `"obs"` → `icn-obs`
- `"security"` → `icn-security`
- `"compute"` → `icn-compute`

## Output

```
Changed files → packages:
  icn/crates/icn-obs/src/attestation.rs     icn-obs
  icn/crates/icn-core/src/config/obs.rs     icn-core

Suggested scoped command:
  cargo clippy -p icn-obs -p icn-core --all-targets -- -D warnings
  cargo test -p icn-obs -p icn-core
```

## Guardrails

- Never run `cargo clippy --workspace` when scoped commands are sufficient.
- When a package name yields "error: package ID specification did not match", immediately run
  this skill with the human label to find the correct name before retrying.
- Bins (`icnd`) depend on everything; touching `icnd/src/main.rs` requires workspace-wide verify.
