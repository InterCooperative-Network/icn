# route-impact — reference

Trigger table, drift-chain rules, and the generated-commit gate for the `route-impact` skill.

## Trigger table

| Changed path pattern | What it means | Downstream actions |
|----------------------|---------------|--------------------|
| `icn/crates/icn-gateway/src/routes/**` | Gateway route added/changed | Regen OpenAPI → regen SDK types → check inventory → review runtime-surface-map |
| `icn/crates/icn-gateway/src/api/**` | Gateway API surface changed | Same as above |
| `icn/crates/icn-api/src/**handler**` | Shared handler logic changed | Same as above (validation/error shape may shift) |
| `docs/api/openapi.generated.yaml` | OpenAPI spec changed | Regen SDK types (`generate-types`), `npm run build` |
| `sdk/typescript/src/generated/**` | Generated SDK types | Confirm they match current OpenAPI; commit alone as `chore(sdk):` |
| `docs/reference/project-index/runtime-surface-map.md` | Runtime surface claims | Cross-check against actual routes; pair with truth-sync |
| `docs/reference/project-index/website-truth-map.md`, website/docs public-claim files | Public claim surface | Run `/icn-agent-pack:truth-sync` |
| `docs/reference/project-index/generated/route-inventory.md` | Generated inventory | Should be produced by the script, not hand-edited; run `--check` |

## The full drift chain

```
shared crate / gateway change
   → gateway/API match + handler updates
      → OpenAPI regen  (icnctl api export-openapi)
         → TS SDK type regen  (npm run generate-types)
            → CI passes (Check API Types Drift)
```

Do not stop at the first half. The most common CI failure here is "Check API Types Drift": OpenAPI
or SDK types left stale after a route change.

## Route inventory check

```bash
python3 docs/scripts/route_inventory.py --check    # exit 1 if the committed artifact is stale
python3 docs/scripts/route_inventory.py --write     # regenerate the committed artifact
```

The `--check` form ignores non-deterministic lines (timestamp/commit churn), so it is safe to run
anytime. A stale exit means source routes and the committed inventory disagree — regenerate with
`--write` and commit the inventory on its own.

## Generated-commit gate

- A regen commit must touch **only** generated paths:
  - OpenAPI: `docs/api/openapi.generated.yaml`
  - SDK types: `sdk/typescript/src/generated/api-types.ts`
  - Route inventory: `docs/reference/project-index/generated/route-inventory.md`
- No lockfile changes unless `npm ci` actually updated deps (rare).
- No mixed "refactor + regen" commits. Label generated commits `chore(sdk): regenerate ...` /
  `docs(project-index): regenerate route inventory`.

## Evidence discipline

When reporting impact, distinguish three things and never conflate them:
1. **Declared** — the route/handler exists in source or in the inventory.
2. **Documented** — OpenAPI / SDK / docs describe it consistently.
3. **Live & authorized** — it actually runs, enforces auth, and is reachable. This is an ops/runtime
   claim, not settled by this skill. Route declarations are evidence of (1), partial evidence of (2),
   and **no** evidence of (3).
