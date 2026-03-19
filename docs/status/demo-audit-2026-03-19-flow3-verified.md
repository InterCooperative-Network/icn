# Flow 3 Post-Fix Verification — 2026-03-19

**Branch:** fix/s15-t3-flow3-clearing-schema (PR #1344)
**Verified:** 2026-03-19, after reseed applied federation init

## Result: PROVEN

Steps 5, 6, and 7 of flow-3-federation.sh now return 2xx on live K3s.

| Step | Action | HTTP | Evidence |
|------|--------|------|----------|
| 5 (BrightWorks) | `POST /v1/federation/coops` | 201 | `{"coop_id":"brightworks-cooperative","status":"registered"}` |
| 6a | `POST /v1/federation/coops/river-city-tool-library/vouch` | 201 | `{"status":"vouched","trust_score":0.85}` |
| 6b | `POST /v1/federation/coops/brightworks-cooperative/vouch` | 201 | `{"status":"vouched","trust_score":0.85}` |
| 7 | `POST /v1/federation/clearing` | 201 | `{"agreement_id":"rc-bw-equipment-90273","status":"created"}` |
| 8 | `GET /v1/federation/clearing/{id}/position` | 200 | Position returned |

Governance (steps 3-4): River City and BrightWorks both ACCEPTED — unchanged.
Step 5a (River City registration): 400 "already registered" — tolerated (already registered from pre-fix test). Functionally correct.

## What fixed it

Three issues found and fixed:

1. `did` → `public_did` in `RegisterCoopRequest`
2. Added `"gateway_endpoints":[]` (required field, no `serde(default)`)
3. Vouch body: `{attested_by,attestation}` → `{target_coop_id,trust_score,expires_in_days}`
4. Clearing: trilateral body → bilateral `CreateAgreementRequest`
5. `reseed-federation-demo.sh`: added `_ensure_federation_init` for all 4 nodes
6. `lib-demo-ports.sh`: added `federation:admin` to `DEMO_DEFAULT_SCOPES`

## Flow 3 classification: PROVEN (federation API calls work)
