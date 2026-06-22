---
name: route-impact
description: ICN route/API impact analysis. This skill should be used when the user explicitly invokes "/icn-agent-pack:route-impact", or asks "what does this route change affect", "do I need to regen OpenAPI / the TS SDK", or "route impact for <paths/PR>". Given changed route/API files (or paths/PR context in arguments), determines whether OpenAPI, the route inventory, the TS SDK, runtime-surface-map, docs, or website claims need updates.
disable-model-invocation: true
user-invocable: true
allowed-tools: "Bash, Read, Grep, Glob"
argument-hint: "[changed paths | PR# | branch]"
---

Given changed route/API files, determine the downstream artifacts that must be regenerated or re-checked. The chain is: **gateway route → OpenAPI spec → route inventory → TS SDK types → runtime-surface-map → docs/website claims.** A change early in the chain that skips the second half causes CI drift.

Arguments: `$ARGUMENTS` may be a list of changed paths, a PR number, or a branch name. If empty, derive the changed set from `git diff --name-only origin/main...HEAD`.

## Evidence limits (state this up front)

A declared route is **evidence-limited**. A route appearing in source or in the generated inventory means the handler is *declared* — it is **not** proof that the route is correct, authorized, wired to live state, or running in production. Never let "the route exists" become "the feature is live / secure." Pair liveness/readiness claims with `/icn-agent-pack:truth-sync`.

## Step 1 — Resolve the changed set

```bash
# From explicit args, or fall back to the branch diff
git diff --name-only origin/main...HEAD
```

Classify each changed path (see `reference.md` for the full trigger table). The high-signal triggers:
- `icn/crates/icn-gateway/src/routes/**`, `.../api/**`, `icn-api/src/**handler**` → gateway API changed
- `docs/api/openapi.generated.yaml` → OpenAPI changed
- `sdk/typescript/**` → SDK surface changed
- `docs/reference/project-index/**`, website/docs public-claim files → claim surface changed

## Step 2 — Run the checks that apply

```bash
# Always safe: is the committed route inventory stale vs. source?
python3 docs/scripts/route_inventory.py --check     # exit 1 if stale
```

If a **gateway API** route changed, regenerate OpenAPI:
```bash
cd "$(git rev-parse --show-toplevel)/icn" && cargo build -p icnctl
./target/debug/icnctl api export-openapi > ../docs/api/openapi.generated.yaml
```

If **OpenAPI** changed (directly or via the export above), regenerate/check the TS SDK:
```bash
cd "$(git rev-parse --show-toplevel)/sdk/typescript"
npm ci && npm run generate-types     # writes src/generated/api-types.ts only
npm run build                         # generate-types && tsc
```

## Step 3 — Report

Report which downstream artifacts need updating and which are clean, plus any docs/website claims (via `runtime-surface-map.md` / `website-truth-map.md`) that the route change may invalidate. Keep generated-file commits isolated (`chore(sdk): regenerate ...`, generated paths only).

See `reference.md` for the full trigger table, the drift-chain rules, and the generated-commit gate.
