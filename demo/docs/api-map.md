# ICN Gateway API Map

**Discovered:** 2026-03-07
**Cluster:** K3s at 10.8.30.40 (four coop namespaces: alpha, beta, gamma, delta)
**Method:** Live cluster probing + source-code analysis of `crates/icn-gateway`

---

## Critical Finding: Gateway Not Deployed

The four K3s coop pods (`icn-alpha`, `icn-beta`, `icn-gamma`, `icn-delta`) **do not run the HTTP gateway API**. They run `icnd` (the P2P daemon) without the `--gateway-enable` flag.

**What the pods actually expose:**

| Port | Protocol | What's There |
|---|---|---|
| 7827 | QUIC/UDP | P2P gossip transport |
| 5651 | TCP | gRPC RPC |
| 9150 | TCP | Prometheus metrics (also responds 200 to any path with metrics body) |
| 8080 | TCP | Declared in K8s pod spec but **not bound** inside the container |

**Evidence:**
- `/proc/net/tcp` inside `icn-alpha` shows only port `0x23BE` (9150) listening
- Port-forward to 8080 → connection refused from inside pod
- `icn.toml` has no `[gateway]` section; only `[observability]` with `health_port = 8080`
- `icnd --gateway-enable` is an opt-in flag, absent from pod args

**Port-forward probes attempted (HTTP status codes):**

```
kubectl port-forward -n icn-coop-alpha svc/icn-alpha 18081:8080  → connection refused (8080 not bound)
kubectl port-forward -n icn-coop-alpha svc/icn-alpha 19150:9150  → HTTP 200 (Prometheus metrics for all paths)
```

Port 9150 returns Prometheus exposition format for every path — it is **not** a REST API:
```
# TYPE icn_supervisor_errors_total counter
icn_supervisor_errors_total{operation="identity_bundle_missing"} 1
# TYPE icn_system_actors_active gauge
icn_system_actors_active 0
# TYPE icn_system_uptime_seconds gauge
icn_system_uptime_seconds 609070
```

---

## Auth

The gateway uses **DID-based challenge-response** (not simple bearer token issuance).

**Two-step flow:**

```bash
# Step 1: Get a nonce challenge for your DID
POST /v1/auth/challenge
Content-Type: application/json
{"did": "did:icn:<your-pubkey>"}

# Response:
# {"nonce": "<64-char hex>", "expires_in": 300}

# Step 2: Sign the nonce with your Ed25519 key, post signature
POST /v1/auth/verify
Content-Type: application/json
{
  "did": "did:icn:<your-pubkey>",
  "signature": "<hex-encoded 64-byte Ed25519 signature of nonce bytes>",
  "coop_id": "brightworks-cooperative",
  "scopes": ["ledger:read", "governance:write"]
}

# Response:
# {"token": "<JWT>", "expires_in": 3600}
```

**Token format:** JWT, 1-hour expiry. Pass as `Authorization: Bearer <token>` on subsequent requests.

**Scopes used in the codebase:**
- `ledger:read`, `ledger:write`
- `governance:read`, `governance:write`
- `treasury:read`, `treasury:write`
- `compute:submit`
- `federation:read`, `federation:write`

**Auth could not be obtained against the running cluster** — the gateway is not deployed, so there are no live endpoints to test. The mechanism above is sourced from `src/api/auth.rs` and `src/auth.rs`.

---

## Endpoint Inventory

All paths are prefixed with `/v1`. Source: `crates/icn-gateway/src/server.rs` (router) + individual handler files.

### Public Endpoints (No Auth)

| Method | Path | Notes |
|---|---|---|
| GET | /v1/healthz | Liveness: `{"status":"alive","timestamp":"..."}` |
| GET | /v1/readyz | Readiness: checks CoopManager, returns 503 if unhealthy |
| GET | /v1/ready | Detailed readiness: `{"status":"ok","uptime_seconds":N,"components":{...}}` |
| GET | /v1/health | Basic health: `{"status":"ok","version":"..."}` |
| GET | /v1/health/detailed | Full component health with latency measurements |
| POST | /v1/auth/challenge | Request DID challenge nonce |
| POST | /v1/auth/verify | Submit signed challenge, receive JWT |
| GET | /v1/ws/{coop_id} | WebSocket event stream (upgrade) |
| POST | /v1/sessions | Create QR login session |
| GET | /v1/sessions/{session_id} | Get QR session status |
| GET | /v1/identity/resolve/{did} | Resolve DID document |
| GET | /v1/identity/health | Identity subsystem health |
| GET | /v1/members/{coop_id}/{did} | Get member profile (read-only) |
| GET | /v1/coops/{id}/stats | Cooperative statistics |
| GET | /v1/sdis/health | SDIS (social identity) subsystem health |
| POST | /v1/sdis/verify/level1 | SDIS level-1 verification |
| POST | /v1/sdis/verify/level2 | SDIS level-2 verification |
| POST | /v1/sdis/ephemeral/generate | Generate ephemeral identity |
| POST | /v1/sdis/enrollment/start | Begin enrollment ceremony |
| POST | /v1/sdis/enrollment/verify/level1 | Enrollment step: verify level 1 |
| POST | /v1/sdis/enrollment/verify/level2 | Enrollment step: verify level 2 |
| POST | /v1/sdis/enrollment/complete | Complete enrollment |

### Protected Endpoints (Auth Required)

#### Sessions

| Method | Path | Notes |
|---|---|---|
| POST | /v1/sessions/{session_id}/approve | Approve a QR login session |

#### Cooperatives (`/v1/coops`)

| Method | Path | Notes |
|---|---|---|
| POST | /v1/coops | Create cooperative |
| GET | /v1/coops/{id} | Get cooperative |
| PUT | /v1/coops/{id}/settings | Update settings |
| DELETE | /v1/coops/{id} | Delete cooperative |
| POST | /v1/coops/{id}/members | Add member |
| DELETE | /v1/coops/{id}/members/{did} | Remove member |
| PUT | /v1/coops/{id}/members/{did}/role | Update member role |
| POST | /v1/coops/{coop_id}/proposals | Propose treasury spend (Flow C alias) |
| GET | /v1/coops/{coop_id}/treasury/position | Treasury position (Flow C alias) |

#### Members

| Method | Path | Notes |
|---|---|---|
| PUT | /v1/members/{coop_id}/{did}/profile | Update member profile |

#### Ledger (`/v1/ledger`)

| Method | Path | Notes |
|---|---|---|
| GET | /v1/ledger/{coop_id}/position/{did} | Credit balance / position for a DID |
| POST | /v1/ledger/{coop_id}/settle | Create settlement |
| GET | /v1/ledger/{coop_id}/history | Transaction history |
| GET | /v1/ledger/{coop_id}/entries/by-decision | Entries filtered by decision hash |
| POST | /v1/ledger/{coop_id}/settle/convert | Cross-currency settlement |
| POST | /v1/ledger/{coop_id}/settle/convert/quote | Quote for cross-currency settlement |

#### Treasury (`/v1/treasury`)

| Method | Path | Notes |
|---|---|---|
| GET | /v1/treasury/{coop_id} | Treasury overview |
| GET | /v1/treasury/{coop_id}/position | Treasury position |
| GET | /v1/treasury/{coop_id}/nonce | Current nonce |
| GET | /v1/treasury/{coop_id}/budgets | List budgets |
| GET | /v1/treasury/{coop_id}/budgets/{budget_id} | Get budget |
| POST | /v1/treasury/{coop_id}/budgets | Create budget |
| GET | /v1/treasury/{coop_id}/spending-rules | Spending rules |
| GET | /v1/treasury/{coop_id}/audit | Treasury audit log |
| POST | /v1/treasury/{coop_id}/deposit | Deposit to treasury |
| POST | /v1/treasury/{coop_id}/spend | Spend from treasury |

#### Governance (two systems)

**Legacy GovernanceManager (`/v1/proposals` + `/v1/coops`):**

| Method | Path | Notes |
|---|---|---|
| POST | /v1/proposals/{id}/vote | Cast vote (Flow C alias) |
| GET | /v1/proposals/{id}/proof | Get governance proof (Flow C alias) |

**GovernanceActor (`/v1/gov/*`) — primary governance system:**

| Method | Path | Notes |
|---|---|---|
| POST | /v1/gov/domains | Create governance domain |
| GET | /v1/gov/domains | List domains |
| GET | /v1/gov/domains/{domain_id} | Get domain |
| POST | /v1/gov/domains/{domain_id}/members | Add domain member |
| DELETE | /v1/gov/domains/{domain_id}/members | Remove domain member |
| POST | /v1/gov/proposals | Create proposal |
| GET | /v1/gov/proposals | List proposals |
| GET | /v1/gov/proposals/{proposal_id} | Get proposal |
| POST | /v1/gov/proposals/{proposal_id}/open | Open proposal for voting |
| POST | /v1/gov/proposals/{proposal_id}/close | Close proposal |
| POST | /v1/gov/proposals/{proposal_id}/vote | Cast vote |
| GET | /v1/gov/proposals/{proposal_id}/tally | Get vote tally |
| GET | /v1/gov/proposals/{proposal_id}/proof | Get governance proof/receipt |
| GET | /v1/gov/proposals/{proposal_id}/discussion | Get discussion thread |
| POST | /v1/gov/proposals/{proposal_id}/discussion/comments | Add comment |
| GET | /v1/gov/proposals/{proposal_id}/discussion/comments | List comments |
| PUT | /v1/gov/proposals/{proposal_id}/discussion/comments/{comment_id} | Edit comment |
| DELETE | /v1/gov/proposals/{proposal_id}/discussion/comments/{comment_id} | Delete comment |
| POST | /v1/gov/proposals/{proposal_id}/discussion/comments/{comment_id}/reactions | Add reaction |
| DELETE | /v1/gov/proposals/{proposal_id}/discussion/comments/{comment_id}/reactions | Remove reaction |
| POST | /v1/gov/delegations | Create vote delegation |
| GET | /v1/gov/delegations | List delegations |
| DELETE | /v1/gov/delegations/{delegation_id} | Revoke delegation |
| POST | /v1/gov/domains/{domain_id}/action-items | Create action item |
| GET | /v1/gov/domains/{domain_id}/action-items | List action items |
| GET | /v1/gov/domains/{domain_id}/action-items/{item_id} | Get action item |
| PUT | /v1/gov/domains/{domain_id}/action-items/{item_id} | Update action item |
| DELETE | /v1/gov/domains/{domain_id}/action-items/{item_id} | Delete action item |
| PUT | /v1/gov/domains/{domain_id}/action-items/{item_id}/status | Update status |
| POST | /v1/gov/domains/{domain_id}/action-items/{item_id}/notes | Add note |
| POST | /v1/gov/proposals/federation/join | Propose joining federation |
| POST | /v1/gov/proposals/federation/leave | Propose leaving federation |
| POST | /v1/gov/proposals/federation/clearing/establish | Propose clearing agreement |
| POST | /v1/gov/proposals/federation/clearing/terminate | Propose clearing termination |
| POST | /v1/gov/proposals/federation/vouch | Propose vouch for coop |
| POST | /v1/gov/proposals/federation/vouch/revoke | Propose revoking vouch |
| POST | /v1/gov/proposals/federation/policy | Propose federation policy update |

#### Federation (`/v1/federation`)

| Method | Path | Notes |
|---|---|---|
| GET | /v1/federation/status | Federation status |
| POST | /v1/federation/init | Initialize federation |
| GET | /v1/federation/coops | List federated coops |
| GET | /v1/federation/coops/{coop_id} | Get federated coop |
| POST | /v1/federation/coops | Register coop |
| GET | /v1/federation/coops/{coop_id}/vouches | Get vouches for coop |
| POST | /v1/federation/coops/{coop_id}/vouch | Vouch for coop |
| GET | /v1/federation/attestations/{member_did} | Get attestations for DID |
| POST | /v1/federation/attestations | Issue attestation |
| GET | /v1/federation/clearing | List clearing agreements |
| GET | /v1/federation/clearing/{agreement_id} | Get clearing agreement |
| POST | /v1/federation/clearing | Create clearing agreement |
| GET | /v1/federation/clearing/{agreement_id}/position | Position in clearing |
| POST | /v1/federation/clearing/{agreement_id}/settle | Trigger settlement |
| POST | /v1/federation/clearing/settle-scheduled | Process scheduled settlements |
| POST | /v1/federation/clearing/netting/{unit} | Compute multilateral netting |
| POST | /v1/federation/clearing/netting/{unit}/apply | Apply netting result |
| POST | /v1/federation/connect | Connect to remote federation |

#### Execution Records (`/v1/execution`)

| Method | Path | Notes |
|---|---|---|
| GET | /v1/execution/records | List execution records |
| GET | /v1/execution/records/{decision_hash} | Get execution record by decision hash |

#### Economic Receipts (`/v1/receipts`)

| Method | Path | Notes |
|---|---|---|
| GET | /v1/receipts/allocations/{hash} | Get allocation receipt |
| GET | /v1/receipts/intents/{hash} | Get settlement intent |
| GET | /v1/receipts/chain | List receipt chain |
| GET | /v1/receipts/chain/{decision_hash} | Get receipt chain for decision |
| GET | /v1/receipts/allocations | List all allocation receipts |
| GET | /v1/receipts/intents | List all settlement intents |

#### Compute (`/v1/compute`)

| Method | Path | Notes |
|---|---|---|
| POST | /v1/compute/submit | Submit compute task |
| GET | /v1/compute/status/{task_hash} | Get task status |
| POST | /v1/compute/wasm/upload | Upload WASM module |
| GET | /v1/compute/wasm | List WASM modules |
| GET | /v1/compute/wasm/{hash} | Get WASM module metadata |

#### Trust (`/v1/trust`)

| Method | Path | Notes |
|---|---|---|
| GET | /v1/trust/{did} | Get trust score for DID |
| GET | /v1/trust/{did}/edges | Get trust graph edges for DID |
| POST | /v1/trust/attest | Create trust attestation |
| POST | /v1/trust/revoke | Revoke trust attestation |
| GET | /v1/trust/{did}/network | Get trust network (multi-hop) |

#### Exchange Rate Oracle (`/v1/oracle`)

| Method | Path | Notes |
|---|---|---|
| GET | /v1/oracle/rate/{from}/{to} | Get exchange rate |
| POST | /v1/oracle/convert | Convert amount between units |
| GET | /v1/oracle/sources | List rate sources |
| POST | /v1/oracle/rate | Submit rate update |

#### Miscellaneous Protected

| Method | Path | Notes |
|---|---|---|
| GET | /v1/rights/summary | Rights summary for authenticated user |
| POST | /v1/invites | Create invite |
| GET | /v1/invites | List invites |
| POST | /v1/invites/join | Join via invite code |
| GET | /v1/names/{path:.*} | Name resolution (catch-all) |
| POST | /v1/notifications/register | Register push notification device |
| DELETE | /v1/notifications/unregister | Unregister device |
| POST | /v1/settlements/recurring | Create recurring settlement |
| GET | /v1/settlements/recurring | List recurring settlements |
| GET | /v1/settlements/recurring/{id} | Get recurring settlement |
| PUT | /v1/settlements/recurring/{id} | Update recurring settlement |
| DELETE | /v1/settlements/recurring/{id} | Delete recurring settlement |
| POST | /v1/payments/recurring | Create recurring payment |
| GET | /v1/payments/recurring | List recurring payments |
| GET | /v1/payments/recurring/{id} | Get recurring payment |
| PUT | /v1/payments/recurring/{id} | Update recurring payment |
| DELETE | /v1/payments/recurring/{id} | Delete recurring payment |
| POST | /v1/escrow | Create escrow |
| GET | /v1/escrow | List escrows |
| GET | /v1/escrow/{id} | Get escrow |
| POST | /v1/escrow/{id}/release | Release escrow |
| POST | /v1/escrow/{id}/refund | Refund escrow |
| POST | /v1/budgets | Create budget |
| GET | /v1/budgets | List budgets |
| GET | /v1/budgets/{id} | Get budget |
| PUT | /v1/budgets/{id} | Update budget |
| DELETE | /v1/budgets/{id} | Delete budget |
| GET | /v1/governance/{charter_id}/dashboard | Governance dashboard |

---

## Key Endpoints for Demo Flows

### Flow 1 — Governance (Harbor Homes roof repair proposal)

| Operation | Method | Path | Auth | Payload / Notes |
|---|---|---|---|---|
| Authenticate | POST | /v1/auth/challenge | No | `{"did":"<did>"}` → nonce |
| Get token | POST | /v1/auth/verify | No | `{"did":"...","signature":"...","coop_id":"harbor-homes","scopes":["governance:write","governance:read"]}` |
| Create proposal | POST | /v1/gov/proposals | Yes | `{"domain_id":"harbor-homes-governance","title":"Roof Repair Fund","description":"...","type":"spend"}` |
| Open for voting | POST | /v1/gov/proposals/{id}/open | Yes | body optional |
| Cast vote | POST | /v1/gov/proposals/{id}/vote | Yes | `{"choice":"for","comment":"..."}` |
| Alias: cast vote | POST | /v1/proposals/{id}/vote | Yes | Same payload, Flow C shortcut |
| Get tally | GET | /v1/gov/proposals/{id}/tally | Yes | — |
| Close proposal | POST | /v1/gov/proposals/{id}/close | Yes | — |
| Get proof | GET | /v1/gov/proposals/{id}/proof | Yes | GovernanceReceipt with attestations |
| Alias: get proof | GET | /v1/proposals/{id}/proof | Yes | Flow C shortcut |

### Flow 2 — Patronage (BrightWorks Q1 contributions)

| Operation | Method | Path | Auth | Notes |
|---|---|---|---|---|
| Get member balance | GET | /v1/ledger/{coop_id}/position/{did} | Yes | Returns credit balance |
| Get history | GET | /v1/ledger/{coop_id}/history | Yes | Paginated, query params: `limit`, `offset` |
| Get by decision | GET | /v1/ledger/{coop_id}/entries/by-decision | Yes | Query: `decision_hash=...` |
| Create settlement | POST | /v1/ledger/{coop_id}/settle | Yes | Patronage allocation |
| Treasury position | GET | /v1/treasury/{coop_id}/position | Yes | Collective reserves |
| Alias: treasury | GET | /v1/coops/{coop_id}/treasury/position | Yes | Flow C shortcut |
| Propose spend | POST | /v1/coops/{coop_id}/proposals | Yes | `{"amount":N,"recipient":"did:icn:...","memo":"...","unit":"credits"}` |
| Get receipts | GET | /v1/receipts/allocations | Yes | Allocation receipts for audit |

### Flow 3 — Federation (River City + BrightWorks equipment agreement)

| Operation | Method | Path | Auth | Notes |
|---|---|---|---|---|
| Federation status | GET | /v1/federation/status | Yes | — |
| Register coop | POST | /v1/federation/coops | Yes | Register coop in federation |
| Vouch for coop | POST | /v1/federation/coops/{coop_id}/vouch | Yes | Trust establishment |
| Create clearing | POST | /v1/federation/clearing | Yes | Cross-coop clearing agreement |
| Get position | GET | /v1/federation/clearing/{id}/position | Yes | Net position in clearing |
| Settle | POST | /v1/federation/clearing/{id}/settle | Yes | Trigger settlement |
| Issue attestation | POST | /v1/federation/attestations | Yes | Cross-coop trust attestation |
| Get attestations | GET | /v1/federation/attestations/{did} | Yes | All attestations for a DID |
| Fed proposal: join | POST | /v1/gov/proposals/federation/join | Yes | Governance vote to join |
| Fed proposal: vouch | POST | /v1/gov/proposals/federation/vouch | Yes | Governance vote to vouch |

### Flow 4 — Reporting (Finger Lakes CDN audit view)

| Operation | Method | Path | Auth | Notes |
|---|---|---|---|---|
| Receipt chain | GET | /v1/receipts/chain | Yes | Full allocation receipt chain |
| Receipt by decision | GET | /v1/receipts/chain/{decision_hash} | Yes | Provenance for specific decision |
| Execution records | GET | /v1/execution/records | Yes | Compute execution audit log |
| Trust network | GET | /v1/trust/{did}/network | Yes | Multi-hop trust graph |
| Treasury audit | GET | /v1/treasury/{coop_id}/audit | Yes | Treasury transaction audit |
| Governance proof | GET | /v1/gov/proposals/{id}/proof | Yes | Verifiable governance receipt |

---

## Gaps / Missing Endpoints

The demo task descriptions referenced several paths that **do not exist** as named in the gateway:

| Demo Reference | Actual Status |
|---|---|
| `GET /v1/ledger/balance` | Does not exist. Use `/v1/ledger/{coop_id}/position/{did}` |
| `GET /v1/ledger/journal` | Does not exist. Use `/v1/ledger/{coop_id}/history` |
| `GET /v1/ledger/contributions` | Does not exist. Use `/v1/ledger/{coop_id}/history` or `/v1/ledger/{coop_id}/entries/by-decision` |
| `GET /v1/ledger/patronage` | Does not exist. Patronage is tracked via ledger history + decision refs |
| `GET /v1/ledger/commons-credit` | Does not exist. Commons credits are in the `/v1/commons/*` namespace (holder affiliations, status) |
| `GET /v1/ledger/provenance` | Does not exist. Provenance is at `/v1/receipts/chain/{decision_hash}` and `/v1/gov/proposals/{id}/proof` |
| `GET /v1/governance` | Does not exist as a top-level. Use `/v1/gov/domains` and `/v1/gov/proposals` |
| `GET /v1/governance/proposals` | Does not exist at this path. Use `/v1/gov/proposals` |
| `GET /v1/federation/agreements` | Does not exist. Agreements are at `/v1/federation/clearing` |
| `GET /v1/federation/settlements` | Does not exist. Use `/v1/federation/clearing/{id}/position` and `/v1/settlements/recurring` |
| `POST /v1/auth/token` | Does not exist. Auth is two-step: challenge + verify |
| `GET /v1/coops` (plain list) | Does not exist. Coop listing is federation-scoped at `/v1/federation/coops` |

---

## Per-Node Consistency

All four pods (`icn-alpha`, `icn-beta`, `icn-gamma`, `icn-delta`) run the **same container image** (`10.8.30.40:30500/icn:latest`, sha256 `b431dbbaf...`) and **same config structure** (only `[network]`, `[observability]`, `[rate_limiting]`, `[topology]` sections differ by role/region values). The gateway API is not deployed on any of them.

If/when the gateway is enabled, all four nodes will expose the same API surface — the gateway is coop-namespace-isolated at the application layer (the `coop_id` claim in the JWT, not at the routing level).

---

## How to Enable the Gateway (for Demo)

The gateway is available in the `icnd` binary but requires:

1. Add `--gateway-enable` and `--gateway-bind 0.0.0.0:8080` to the pod command args
2. Add `--gateway-jwt-secret <secret>` (or use env var)
3. Rebuild/redeploy the pod, or patch the Deployment

Once enabled, the auth flow works via DID-challenge (see Auth section above). No pre-seeded tokens exist — each demo session needs to authenticate a DID that has been registered in the coop.

The existing `demo/flow-c-treasury-governance.sh` and scripts in `demo/scripts/` show the full auth + API call pattern used in the Flow C demo.
