---
name: icn-gateway-openapi-sync
description: "Keep ICN gateway API behavior, exported OpenAPI spec, and generated TypeScript SDK types in sync. Use when modifying icn-gateway routes, request/response models, auth behavior, or any API shape that can cause OpenAPI/SDK drift."
---

# ICN Gateway OpenAPI Sync

Treat API drift as a blocking integration defect.

## Trigger Conditions

Use this skill when changes touch:
- `icn/crates/icn-gateway/**`
- shared API models consumed by gateway endpoints
- auth/scopes/status codes that affect endpoint contracts

## Workflow

1. Validate gateway behavior.
```bash
cd icn
cargo test -p icn-gateway --features sled-storage
```

2. Regenerate OpenAPI from source of truth.
```bash
cd icn
cargo build -p icnctl
./target/debug/icnctl api export-openapi -o ../docs/api/openapi.generated.yaml
```

3. Regenerate TypeScript types.
```bash
cd sdk/typescript
npm ci
npm run generate-types
npm run check-types
npm run build
npm test
npm run lint
```

4. Confirm drift status.
```bash
git status --short docs/api/openapi.generated.yaml sdk/typescript
```
- If generated artifacts changed, include them in the same PR.
- If artifacts did not change, state that explicitly.

5. Validate docs alignment.
- Update docs for changed semantics or response behavior.
- Ensure auth gating semantics match implementation.

## Guardrails

- Do not hand-edit generated OpenAPI/types.
- Do not hide API changes by skipping regeneration.
- Keep error/status semantics explicit and documented.

## Output Contract

Return:
1. whether gateway API surface changed,
2. generated files changed or unchanged,
3. commands run with pass/fail outcome,
4. follow-up docs/spec tasks if any.
