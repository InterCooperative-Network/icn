---
paths:
  - "icn/crates/icn-gateway/**"
  - "icn/crates/icn-api/**"
  - "icn/crates/icn-rpc/**"
---

# Gateway & API Rules

## Gateway Architecture

- REST + WebSocket API for cooperative applications
- Uses `GatewayError` with machine-readable error codes
- Protocol rejections use `ErrCode` from `icn-kernel-api`
- Input validation at the boundary (never trust external input)

## API Changes

When modifying gateway API endpoints:
1. Update OpenAPI spec: `cd icn && cargo build -p icnctl && ./target/debug/icnctl api export-openapi -o ../docs/api/openapi.generated.yaml`
2. Regenerate TypeScript types: `cd sdk/typescript && npm ci && npm run generate-types && npm run check-types`
3. Check for breaking changes and document in PR

## Error Handling

- All gateway errors must map to stable HTTP status codes
- Use `ErrCode` for protocol-level rejections
- Include machine-readable `code` field in JSON error responses
- Never expose internal error details to clients

## Security

- Validate all input at the gateway boundary
- Never weaken authentication/authorization to fix tests
- Rate limiting is trust-gated (applied per `ConstraintSet`)
- Never hardcode trust thresholds - use PolicyOracle decisions

## Testing

```bash
cd icn
cargo test -p icn-gateway --features sled-storage
cargo test -p icn-api
```
