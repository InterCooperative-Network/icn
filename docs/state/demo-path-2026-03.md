# ICN Demo Path — March 2026

**Date**: 2026-03-22
**Branch**: main
**Commit**: 20e0527190d51f78b5417ca41c4efe6565e015b8
**Sprint**: 23 (Baseline Lock + Narrative Surface)
**Author**: Claude Code (s23-t10)

---

## Scope Note

This document covers the validated demo flows as of Sprint 23. The Sprint 23
design spec (s23-t10) references "Flow A (WASM), Flow B (discovery), Flow C
(treasury governance)" as the minimum required coverage. Those labels do not
correspond to named scripts on main as of 2026-03-22.

The actual executable flows are:

| Spec label        | Actual script(s)                                  | Status      |
|-------------------|---------------------------------------------------|-------------|
| Flow A (WASM)     | No flow-a script exists; WASM routes in API map   | No script   |
| Flow B (discovery)| No flow-b script exists; devnet gossip auto-peers | No script   |
| Flow C (treasury) | `demo/flow-c-treasury-governance.sh`              | Script present, K3s-only token path |
| Flow 1 (governance)| `demo/scripts/flow-1-governance.sh`              | PROVEN on K3s (2026-03-19 audit) |
| Flow 2 (patronage)| `demo/scripts/flow-2-patronage.sh`               | FRAGILE (ledger routes 404) |
| Flow 3 (federation)| `demo/scripts/flow-3-federation.sh`              | PROVEN on K3s post PR #1344 |

The following Sprint 23 items do NOT affect demo execution:

- **#1095** (CRDT OrSet + LwwRegister): Deferred to Sprint 24. The CRDT types
  (`OrSet`, `LwwRegister`, etc.) exist as enum variants in `icn-gossip` but have
  no concrete merge implementations. None of the demo flows invoke CRDT state.
  Not required for any of Flows 1, 2, 3, or C.

- **#1096** (ContainerRuntime trait): Trait interface `ContainerRuntime` is
  defined in `icn-kernel-api/src/container.rs`. No concrete runtime
  implementation exists. The demo flows use the existing governance and compute
  layer; no container runtime is invoked. Not required for any demo flow.

---

## Prerequisites

**For K3s cluster flows (flow-1, flow-2, flow-3)**:
- `kubectl` with access to the K3s cluster at 10.8.30.40
- `icnctl` binary present inside each cooperative pod (confirmed present)
- `python3` (for JSON parsing in scripts)
- All four gateways reachable via `kubectl port-forward`

**For devnet flows (flow-c, run-all)**:
- `docker` and `docker compose`
- `python3` with `cryptography` package (`pip3 install cryptography`)
- Devnet image built (see Devnet Startup below)
- Gateway image includes `icnctl` at `/usr/local/bin/icnctl`

---

## Devnet Startup

The three-node local devnet is defined in `deploy/devnet/docker-compose.yml`.

**Build the image (one-time, or after Rust changes):**
```bash
cd /home/ubuntu/projects/icn/deploy/devnet
docker compose build
```

**Start the devnet:**
```bash
cd /home/ubuntu/projects/icn/deploy/devnet
docker compose up -d
```

**Wait for nodes to be healthy:**
```bash
# Each node reports HTTP 200 on /v1/health when ready
curl -sf http://localhost:8080/v1/health   # node-a
curl -sf http://localhost:8081/v1/health   # node-b
curl -sf http://localhost:8082/v1/health   # node-c
```

**Port assignments (devnet)**:
- `localhost:8080` — node-a gateway (container: `icn-devnet-node-a`)
- `localhost:8081` — node-b gateway (container: `icn-devnet-node-b`)
- `localhost:8082` — node-c gateway (container: `icn-devnet-node-c`)
- `localhost:9090` — Prometheus metrics

**Port**: Gateway binds `localhost:8080` in devnet mode (NOT 30080, which is the
K3s NodePort for the production cluster).

**Keystore passphrase**: `devnet-insecure` (baked into `docker-compose.yml` via
`ICN_KEYSTORE_PASSPHRASE`). Do not use in non-demo environments.

**Full suite shortcut (devnet boot + governance demo on node-a and node-b)**:
```bash
cd /home/ubuntu/projects/icn
bash demo/run-all.sh
# Options:
#   --no-boot       skip docker compose up (devnet already running)
#   --presenter     interactive pause between phases
```

`run-all.sh` runs `demo/scripts/demo-governance.py` against node-a and node-b.
It does NOT run flow-1/2/3 scripts — those target the K3s cluster via kubectl.

**Stop the devnet:**
```bash
cd /home/ubuntu/projects/icn/deploy/devnet
docker compose down
# To also remove data volumes (fresh identities on next start):
docker compose down -v
```

---

## K3s Cluster Startup (for flow-1, flow-2, flow-3)

**Seed canonical demo state before running any flow:**
```bash
cd /home/ubuntu/projects/icn
bash demo/scripts/reseed-federation-demo.sh
```

This script is idempotent. It:
- Starts `kubectl port-forward` for all four gateways (ports 18081-18084)
- Creates coop records and governance domains on each node if absent
- Seeds the Q1 patronage proposal on BrightWorks (required for flow-2)
- Updates self-DID display names

**Note**: ICN gateway state is in-memory and does not survive pod restarts. If
pods have been restarted, run `reseed-federation-demo.sh` again before any flow.

**Gateway port mapping (K3s via kubectl port-forward)**:
```
localhost:18081 → icn-coop-alpha (BrightWorks Cooperative)
localhost:18082 → icn-coop-beta  (River City Tool Library)
localhost:18083 → icn-coop-gamma (Harbor Homes Cooperative)
localhost:18084 → icn-coop-delta (Finger Lakes CDN)
```

---

## Flow 1 — Harbor Homes Governance (PROVEN)

**Script**: `demo/scripts/flow-1-governance.sh`
**Target**: K3s cluster (kubectl required)
**Narrative**: Harbor Homes Cooperative roof repair authorization
**Duration**: ~5 minutes live
**Audience**: All — especially housing and worker coops

**Command:**
```bash
cd /home/ubuntu/projects/icn
bash demo/scripts/flow-1-governance.sh
# Presenter mode (keypress between beats):
bash demo/scripts/flow-1-governance.sh --present
# Narrated mode (auto-advance with 4s pause):
bash demo/scripts/flow-1-governance.sh --narrated
```

**What it demonstrates:**
1. Governance domain creation (quorum 51%, approval 60%, 7-day voting period)
2. Proposal creation with full inspection report context
3. Opening the proposal for member voting
4. Casting a vote (For) anchored to a member DID
5. Vote tally (public to all members)
6. Proposal close and final state (Accepted)
7. GovernanceReceipt retrieval (Ed25519 signed proof)
8. Full governance record retrieval
9. Authorization boundary — governance decision linked to authorized action

**Expected outcome:**
All 10 steps return 2xx. Final proposal state: `Accepted`. GovernanceReceipt
returns HTTP 200 with `"algorithm": "Ed25519"` signature (PR #1327 merged).

**Known constraints:**
- Single seeded member DID represents Harbor Homes board quorum. Production
  deployment would have one DID per voting member.
- If GovernanceReceipt returns 404, the signing key is not in the pod — check
  init container logs. The proposal record remains a valid audit trail.
- Flow 1A is the current scope. Flow 1B (cryptographic execution receipt binding
  the treasury spend to the governance decision) is planned.

**Verification basis**: Audited live on K3s 2026-03-19. Exit code 0, all 10
steps passed. See `docs/status/demo-audit-2026-03-19.md`.

---

## Flow 2 — BrightWorks Patronage Distribution (FRAGILE)

**Script**: `demo/scripts/flow-2-patronage.sh`
**Target**: K3s cluster (kubectl required)
**Narrative**: BrightWorks Cooperative Q1 patronage distribution
**Duration**: ~5 minutes live
**Audience**: Worker coops, food co-ops, intermediate orgs
**Prereq**: `reseed-federation-demo.sh` must have been run (seeds Q1 proposal)

**Command:**
```bash
cd /home/ubuntu/projects/icn
bash demo/scripts/flow-2-patronage.sh
```

**What it demonstrates (governance steps — PROVEN):**
- Steps 1-7: Q1 patronage proposal → vote → close → GovernanceReceipt

**Known broken steps as of 2026-03-19 audit:**
- Step 8: `POST /v1/ledger/brightworks-cooperative/payment` → HTTP 404 (route renamed or missing)
- Step 9: `GET /v1/ledger/brightworks-cooperative/balance/{did}` → HTTP 404
- Step 11: `GET /v1/receipts/allocations` → HTTP 400, missing `decision_hash` field (Bug #1334)

**Expected outcome:**
Governance steps (1-7) exit 0. Script exits non-zero due to ledger 404s. The
governance narrative (transparent allocation formula, member voting) is fully
demonstrable. The settlement receipt chain is broken pending ledger route fixes.

**Note on API surface detection**: `demo_api_preflight()` reports the deployed
binary SHA and detects whether the gateway uses the old API (`/payment`,
`/balance/{did}`) or new API (`/settle`, `/position/{did}`). If SHA detection
returns `UNKNOWN`, manual probe with `demo/scripts/rehearsal-probe.sh` is needed.

---

## Flow 3 — River City / BrightWorks Federation Agreement (PROVEN)

**Script**: `demo/scripts/flow-3-federation.sh`
**Target**: K3s cluster (kubectl required)
**Narrative**: River City Tool Library and BrightWorks equipment agreement
**Duration**: ~7 minutes live
**Audience**: Federations, cooperative developers, ecosystem builders
**Prereq**: `reseed-federation-demo.sh` must have been run

**Command:**
```bash
cd /home/ubuntu/projects/icn
bash demo/scripts/flow-3-federation.sh
```

**What it demonstrates:**
1. Three autonomous nodes with separate identities and governance
2. Cross-coop coordination without a central authority
3. River City governance: equipment-sharing proposal → vote → Accepted
4. BrightWorks governance: maintenance-labor proposal → vote → Accepted
5. Cooperative registration in the federation layer (`POST /v1/federation/coops`)
6. Vouching between coops (`POST /v1/federation/coops/{id}/vouch`)
7. Federation clearing agreement creation (`POST /v1/federation/clearing`)
8. Clearing position retrieval (`GET /v1/federation/clearing/{id}/position`)
9. Delegation and gossip sync status checks

**Expected outcome:**
All steps return 2xx. Both governance proposals reach Accepted. Federation
registration, vouching, and bilateral clearing agreement creation all succeed.

**Key API shapes (as fixed by PR #1344):**
```
POST /v1/federation/coops
Body: {"coop_id":"...","name":"...","public_did":"<DID>","gateway_endpoints":[]}

POST /v1/federation/coops/{coop_id}/vouch
Body: {"target_coop_id":"<coop_id>","trust_score":0.85,"expires_in_days":365}

POST /v1/federation/clearing
Body: {"agreement_id":"<client-id>","partner_coop_id":"<coop_id>",
       "partner_did":"<DID>","max_imbalance":1000,"settlement":"monthly"}
```

**Verification basis**: Post-fix audit on K3s 2026-03-19 (PR #1344). Steps 5, 6,
7, 8 all 2xx. See `docs/status/demo-audit-2026-03-19-flow3-verified.md`.

---

## Flow C — Treasury Governance (devnet, single node)

**Script**: `demo/flow-c-treasury-governance.sh`
**Target**: Any single gateway (devnet or K3s, but token must be pre-obtained)
**Narrative**: Treasury balance → propose spend → vote → governance proof

**Command (against devnet node-a):**
```bash
# Obtain a token first (devnet mode via icnctl):
TOKEN=$(docker exec icn-devnet-node-a \
  env ICN_KEYSTORE_PASSPHRASE=devnet-insecure \
  icnctl auth token \
  --gateway http://localhost:8080 \
  --coop-id demo-coop \
  --scopes "treasury:read,treasury:write,governance:read,governance:write" \
  2>/dev/null | grep -oE 'eyJ[A-Za-z0-9_.-]+\.[A-Za-z0-9_.-]+\.[A-Za-z0-9_.-]+' | head -1)

ICN_BASE_URL=http://localhost:8080 \
ICN_TOKEN="$TOKEN" \
ICN_COOP_ID=demo-coop \
  bash demo/flow-c-treasury-governance.sh
```

**Environment variables:**
- `ICN_BASE_URL` — gateway URL (default: `http://127.0.0.1:7845`)
- `ICN_TOKEN` — JWT bearer token (required, no default)
- `ICN_COOP_ID` — cooperative ID (default: `demo-coop`)
- `ICN_RECIPIENT_DID` — spend recipient DID (default: `did:icn:zBobDemoRecipient123456789`)
- `ICN_SPEND_AMOUNT` — spend amount (default: `100`)
- `ICN_SPEND_CURRENCY` — credit unit (default: `credits`)

**What it demonstrates:**
1. `GET /health` — node liveness
2. `GET /v1/coops/{coop_id}/treasury/balance` — treasury position
3. `POST /v1/coops/{coop_id}/proposals` — propose treasury spend
4. `POST /v1/proposals/{id}/vote` — cast vote (For)
5. `GET /v1/proposals/{id}/proof` — retrieve GovernanceReceipt with `decision_hash`

**Expected output markers:**
```
FLOW_C_START
HEALTH_OK
FLOW_C_PROPOSAL_ID=<id>
FLOW_C_DECISION_HASH=<hex>
```

**Note**: This script targets a single gateway and requires an externally
provided token. It does not use `lib-demo-ports.sh` or `kubectl`. It is suitable
for quick smoke-testing a single node without the K3s cluster.

---

## Status of Spec-Named Flows (Flow A and Flow B)

The Sprint 23 acceptance criteria names "Flow A (WASM)" and "Flow B
(discovery)" as required coverage. As of 2026-03-22, no scripts matching those
descriptions exist.

**Flow A — WASM compute**: The API map (`demo/docs/api-map.md`) documents WASM
routes at `POST /v1/compute/wasm/upload`, `GET /v1/compute/wasm`, and
`GET /v1/compute/wasm/{hash}`. No demo script exercises these routes. WASM
compute is implemented in `icn-compute` but is not on the critical path for
the cooperative governance narrative (Flows 1, 2, 3, C). Flow A script
implementation is Sprint 24 work.

**Flow B — Peer discovery**: Peer discovery happens automatically in the devnet
via `ICN_BOOTSTRAP_PEERS` set in `docker-compose.yml` (node-b and node-c
bootstrap from node-a). The gossip protocol handles subsequent discovery. There
is no interactive demo script that exercises discovery as a named flow. The
devnet `status` make target (`cd deploy/devnet && make status`) shows all three
nodes responding. Discovery verification is implicit in multi-node flows (Flow 3)
where gossip sync is confirmed. A dedicated Flow B script is Sprint 24 work.

---

## Verification Basis

This document was authored by reading the demo scripts and audit records directly
on 2026-03-22. No live devnet or cluster execution was performed.

Sources:
- `demo/scripts/lib-demo-ports.sh` — port library, devnet mode overrides
- `demo/scripts/flow-1-governance.sh` — Harbor Homes governance flow
- `demo/scripts/flow-2-patronage.sh` — BrightWorks patronage flow
- `demo/scripts/flow-3-federation.sh` — federation coordination flow
- `demo/flow-c-treasury-governance.sh` — treasury governance (single node)
- `demo/run-all.sh` — devnet boot + demo-governance.py suite runner
- `deploy/devnet/docker-compose.yml` — 3-node devnet configuration
- `deploy/devnet/Makefile` — devnet management commands
- `docs/status/demo-audit-2026-03-19.md` — live K3s audit results
- `docs/status/demo-audit-2026-03-19-flow3-verified.md` — flow-3 post-fix verification
- `docs/state/ICN-Platform-Baseline-2026-03.md` — #1095/#1096 disposition
