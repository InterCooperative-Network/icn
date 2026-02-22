# ICN Pilot Cluster Operator Runbook

**Audience**: Operators running the ICN pilot on the K3s homelab cluster.  
**Last updated**: 2026-02-22

---

## Table of Contents

1. [Prerequisites](#1-prerequisites)
2. [Environment Setup](#2-environment-setup)
3. [Cluster Health Check](#3-cluster-health-check)
4. [Authentication — Getting a JWT](#4-authentication--getting-a-jwt)
5. [Governance Lifecycle](#5-governance-lifecycle)
6. [Ledger Verification](#6-ledger-verification)
7. [Smoke Test](#7-smoke-test)
8. [Reset Test Data](#8-reset-test-data)
9. [Troubleshooting](#9-troubleshooting)

---

## 1. Prerequisites

### What must be running

| Component | Where | How to verify |
|-----------|-------|---------------|
| K3s control plane | `k3s-control` (10.8.10.40) | `kubectl get nodes` |
| ICN gateway pods | `icn` namespace | `kubectl get pods -n icn` |
| Atlas NFS | 10.8.10.25 | `kubectl get pvc -n icn` — must be `Bound` |

### Tools required on your workstation

```
curl       >= 7.68
jq         >= 1.6
kubectl    >= 1.28   (optional — for pod inspection)
icnctl              (optional — for DID operations)
```

Check:

```bash
curl --version | head -1
jq --version
```

### Gateway NodePorts

| Port  | Instance        | URL |
|-------|----------------|-----|
| 30080 | Default gateway | http://10.8.10.40:30080 |
| 30081 | Coop instance 1 | http://10.8.10.40:30081 |
| 30082 | Coop instance 2 | http://10.8.10.40:30082 |
| 30083 | Coop instance 3 | http://10.8.10.40:30083 |
| 30084 | Coop instance 4 | http://10.8.10.40:30084 |

---

## 2. Environment Setup

Set these in your shell before running any commands:

```bash
export HOST="http://10.8.10.40:30080"   # default gateway
export COOP_ID="pilot-coop-1"           # your cooperative ID
export TOKEN=""                         # filled in after auth (§4)
```

All `curl` examples below assume these are set.

---

## 3. Cluster Health Check

### Single gateway

```bash
curl -sf "$HOST/v1/health" | jq .
```

Expected response:

```json
{"status": "ok"}
```

HTTP 200 means the gateway is up and accepting requests.

### All five gateway ports

```bash
for port in 30080 30081 30082 30083 30084; do
    code=$(curl -s -o /dev/null -w "%{http_code}" --max-time 5 "http://10.8.10.40:${port}/v1/health")
    echo "port $port → $code"
done
```

All should return `200`. A `000` means the port is not listening — check the pod:

```bash
kubectl get pods -n icn -l app=icnd
kubectl logs -n icn -l app=icnd --tail=50
```

### Quick K3s node check (if you have kubectl)

```bash
kubectl get nodes -o wide
kubectl get pods -n icn
kubectl get pvc -n icn
```

All nodes `Ready`, all pods `Running`, all PVCs `Bound`.

---

## 4. Authentication — Getting a JWT

The gateway uses DID-based challenge–response auth. You need an Ed25519 DID keypair.

### Option A: Use icnctl (recommended)

```bash
# Generate a DID keypair if you don't have one
icnctl id init

# Show your DID
icnctl id show

# Get a token (icnctl handles challenge + sign + verify internally)
TOKEN=$(icnctl auth login --gateway "$HOST" --output token)
export TOKEN
```

### Option B: Manual curl flow

**Step 1 — Request a challenge**

```bash
DID="did:icn:<your-base58-pubkey>"

CHALLENGE=$(curl -s -X POST "$HOST/v1/auth/challenge" \
    -H "Content-Type: application/json" \
    -d "{\"did\": \"$DID\"}" | jq -r '.challenge')

echo "Challenge: $CHALLENGE"
```

**Step 2 — Sign the challenge**

Sign the raw challenge string with your Ed25519 private key. The signature must be base64-encoded (standard, no padding).

With icnctl:

```bash
SIG=$(icnctl id sign "$CHALLENGE" --base64)
```

**Step 3 — Verify and get JWT**

```bash
TOKEN=$(curl -s -X POST "$HOST/v1/auth/verify" \
    -H "Content-Type: application/json" \
    -d "{\"did\": \"$DID\", \"challenge\": \"$CHALLENGE\", \"signature\": \"$SIG\"}" \
    | jq -r '.token')

export TOKEN
echo "Token: ${TOKEN:0:40}..."
```

A token is a signed JWT valid for the session. Pass it as `Authorization: Bearer $TOKEN` on all authenticated requests.

---

## 5. Governance Lifecycle

These steps exercise the full governance flow: domain creation → proposal → vote → close.

### 5.1 Create a governance domain

A domain is the container for proposals and membership.

```bash
DOMAIN_ID="ops-test-$(date +%s)"

curl -s -X POST "$HOST/v1/gov/domains" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{
        \"id\": \"$DOMAIN_ID\",
        \"name\": \"Ops Test Domain\",
        \"profile\": \"cooperative_default\",
        \"quorum_percent\": 1,
        \"approval_percent\": 51,
        \"voting_period_days\": 1,
        \"members\": []
    }" | jq .
```

Expected: HTTP 201 with domain object. A 400 with "already exists" is safe to ignore if the domain was created earlier.

### 5.2 Submit a text proposal

```bash
PROPOSAL_TITLE="Test Proposal $(date +%s)"

PROPOSAL_ID=$(curl -s -X POST "$HOST/v1/gov/proposals" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d "{
        \"domain_id\": \"$DOMAIN_ID\",
        \"title\": \"$PROPOSAL_TITLE\",
        \"description\": \"Manual ops test proposal.\",
        \"payload\": {
            \"type\": \"text\",
            \"body\": \"Approve routine maintenance window.\"
        }
    }" | jq -r '.id')

echo "Proposal ID: $PROPOSAL_ID"
```

### 5.3 Open the proposal for voting

```bash
curl -s -X POST "$HOST/v1/gov/proposals/$PROPOSAL_ID/open" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"voting_period_seconds": 3600}' | jq '.state'
```

Expected: proposal state transitions to `open`.

### 5.4 Cast a vote

```bash
curl -s -X POST "$HOST/v1/gov/proposals/$PROPOSAL_ID/vote" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"choice": "for", "comment": "Approved by ops."}' | jq .
```

Valid choices: `"for"`, `"against"`, `"abstain"`.

### 5.5 Close the proposal

```bash
curl -s -X POST "$HOST/v1/gov/proposals/$PROPOSAL_ID/close" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{}' | jq '{state, outcome: .outcome}'
```

Expected: state is one of `accepted`, `rejected`, or `no_quorum`.

### 5.6 Read the result

```bash
curl -s "$HOST/v1/gov/proposals/$PROPOSAL_ID" \
    -H "Authorization: Bearer $TOKEN" | jq '{id, title, state}'
```

---

## 6. Ledger Verification

### Check a balance

```bash
DID="did:icn:<member-did>"

curl -s "$HOST/v1/ledger/$COOP_ID/balance/$DID" \
    -H "Authorization: Bearer $TOKEN" | jq .
```

### Ledger entries linked to a decision

After governance closes a proposal that triggers economic effects, the decision hash links to ledger entries.

```bash
DECISION_HASH="<hex-hash-from-governance-receipt>"

curl -s "$HOST/v1/ledger/$COOP_ID/entries/by-decision?decision_hash=$DECISION_HASH" \
    -H "Authorization: Bearer $TOKEN" | jq .
```

An empty result is normal for text proposals (no economic effect). Budget proposals should produce entries.

### Decision registry trace

The registry tracks the full provenance chain:

```bash
# List decisions for a coop
curl -s "$HOST/v1/registry/decisions?coop_id=$COOP_ID" \
    -H "Authorization: Bearer $TOKEN" | jq '.[].decision_receipt_id'

# Get a specific decision with trace
RECEIPT_ID="<receipt-id-from-above>"

curl -s "$HOST/v1/registry/decisions/$RECEIPT_ID" \
    -H "Authorization: Bearer $TOKEN" | jq .

curl -s "$HOST/v1/registry/decisions/$RECEIPT_ID/trace" \
    -H "Authorization: Bearer $TOKEN" | jq .
```

The `/trace` endpoint shows the linkage: `DecisionReceipt → AllocationReceipt → LedgerEntry`.

---

## 7. Smoke Test

The smoke test script runs the full governance lifecycle in one command and exits non-zero on any failure. It is safe to run against the live pilot cluster — it creates unique IDs per run and does not modify persistent state beyond the in-memory governance store.

```bash
HOST=$HOST TOKEN=$TOKEN COOP_ID=$COOP_ID ./scripts/smoke-test.sh
```

### What it tests

| Step | Endpoint | Success criteria |
|------|----------|-----------------|
| 1 | `GET /v1/health` | 200 |
| 2 | `GET /v1/gov/domains` (no auth) | 401 |
| 3 | `POST /v1/gov/domains` | 201 or 200 |
| 4 | `POST /v1/gov/proposals` | 201 or 200, `id` field present |
| 5 | `POST /v1/gov/proposals/{id}/open` | 200 |
| 6 | `POST /v1/gov/proposals/{id}/vote` | 200 or 201 |
| 7 | `POST /v1/gov/proposals/{id}/close` | 200 |
| 8 | `GET /v1/gov/proposals/{id}` | 200 |
| 9 | `GET /v1/registry/decisions` | 200 |
| 10 | `GET /v1/ledger/{coop}/entries/by-decision` | 200 or 404 |
| 11 | `GET /v1/gov/proposals` | 200 |

### Interpreting output

```
PASS Gateway health check (200)
PASS Unauthenticated request rejected (401)
PASS Create governance domain (201)
PASS Create proposal (201) → id=prop-abc123
PASS Open proposal (200) state=open
PASS Cast vote (200)
PASS Close proposal (200) outcome=accepted
PASS Get closed proposal (200) state=accepted
PASS List registry decisions (200) count=0
PASS Ledger entries-by-decision endpoint reachable (200)
PASS List proposals (200)

Results: 11 passed, 0 failed, 0 skipped (11 total)
SMOKE PASS
```

A `FAIL` line means that specific HTTP call returned an unexpected status. The body is printed below the FAIL line for diagnosis. The script exits `1`.

A `WARN` line is non-fatal (e.g., domain already exists from a previous run).

---

## 8. Reset Test Data

The governance store is in-memory per pod restart. To reset all governance state:

```bash
# Rolling restart of ICN pods (drops in-memory state, PVC data is preserved)
kubectl rollout restart deployment/icnd -n icn

# Wait for pods to come back
kubectl rollout status deployment/icnd -n icn
```

Ledger entries on NFS-backed PVCs survive restarts. To clear ledger data for a specific coop, there is no REST endpoint — this requires direct Sled store access (see ops team).

---

## 9. Troubleshooting

### Gateway returns 000 (connection refused)

The pod is not running or the NodePort is not bound.

```bash
kubectl get pods -n icn
kubectl describe pod <pod-name> -n icn
kubectl logs <pod-name> -n icn --tail=100
```

Common causes:
- PVC not bound (NFS unreachable) — check `kubectl get pvc -n icn`
- OOMKilled — check `kubectl describe pod` for exit code 137
- CrashLoopBackOff — check logs for keystore unlock failure

### 401 Unauthorized on authenticated requests

Your JWT has expired or the wrong secret was used.

```bash
# Decode the JWT payload (no verification, just inspect)
echo "$TOKEN" | cut -d. -f2 | base64 -d 2>/dev/null | jq '{sub, exp}'
```

`exp` is a Unix timestamp. Re-authenticate if expired (§4).

### 403 Forbidden on domain/proposal operations

The DID in your JWT is not a member of the target domain. Either:
1. Add your DID to the domain: `POST /v1/gov/domains/{id}/members`
2. Create a new domain with your DID in the `members` array

### 400 on create domain — "already exists"

Safe to ignore. The domain with that ID was created in a previous run. Use a different `DOMAIN_ID` or proceed with the existing domain.

### Proposal stuck in `draft` state

A proposal must be explicitly opened before votes can be cast:

```bash
curl -s -X POST "$HOST/v1/gov/proposals/$PROPOSAL_ID/open" \
    -H "Authorization: Bearer $TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"voting_period_seconds": 3600}'
```

### Close returns 404

The proposal ID is wrong or the gateway was restarted (in-memory state lost). Re-create via §5.

### Ledger entries/by-decision returns empty

Text proposals do not generate ledger entries — this is correct. Only budget proposals (payload type `budget`) trigger ledger writes. Verify the proposal payload type:

```bash
curl -s "$HOST/v1/gov/proposals/$PROPOSAL_ID" \
    -H "Authorization: Bearer $TOKEN" | jq '.payload.type'
```

### Checking metrics

Prometheus scrapes on port 30090:

```bash
curl -s http://10.8.10.40:30090/metrics | grep gateway_governance
```

Key metrics:
- `gateway_governance_proposals_created_total`
- `gateway_governance_proposals_opened_total`
- `gateway_governance_proposals_closed_total`
- `gateway_governance_votes_cast_total`

### Pod logs for a specific operation

Enable debug logging for a single pod:

```bash
kubectl exec -n icn <pod-name> -- sh -c 'kill -USR1 1'  # toggles log level if supported
kubectl logs -n icn <pod-name> -f --tail=200
```

Or filter for a specific proposal ID:

```bash
kubectl logs -n icn <pod-name> --tail=500 | grep "$PROPOSAL_ID"
```
