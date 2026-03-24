# ICN Governance Demo — Operator Runbook

## Prerequisites

- **Rust toolchain** (stable 1.88.0 — pinned in `icn/rust-toolchain.toml`)
- **Python 3.8+** with `cryptography` package (`pip install cryptography`)
- **Port 8080** available (check: `ss -tlnp | grep :8080`)
- **curl** (for health checks)

First-time setup:
```bash
pip install cryptography    # Ed25519 key generation
cd icn && cargo build -p icnd   # ~2 min first build, ~10s incremental
```

## Quick Start (automated, for CI/testing)

```bash
bash demo/scripts/start-demo.sh /tmp/icn-demo
python3 demo/scripts/demo-governance.py http://localhost:8080
pkill -f icnd
```

All 19 steps run without pausing. Exit code 0 = success, 1 = failure.

## Presenter Mode (interactive, for live demos)

```bash
bash demo/scripts/present-governance.sh
```

This will:
1. Build `icnd` (skips if binary is fresh)
2. Initialize a clean data directory at `/tmp/icn-demo`
3. Start the gateway on port 8080
4. Run the demo script with colored output and Enter-to-continue pauses
5. Leave the gateway running for the browser demo
6. Clean up on Ctrl+C

The demo is split into four phases:
- **Phase 1: Founding Assembly** — Alice, Bob, Carol form the cooperative as equal members. Alice is elected temporary coordinator (not a permanent admin role).
- **Phase 2: Charter Ratification** — The first democratic act: all three members vote to ratify their own cooperative charter.
- **Phase 3: Democratic Decision** — Bob (not the coordinator) proposes $12,000 for kitchen equipment. All three vote.
- **Phase 4: Verification** — Carol (not the coordinator, not the proposer) closes the vote. Tally and cryptographic proof generated.

Press Enter between phases to control pacing.

## Browser Demo

After starting the gateway (via either script), open:

```
http://localhost:8080/static/demo.html
```

The browser demo has four "Run Phase N" buttons matching the four phases. Click each one in sequence. No terminal needed.

The browser UI highlights the cooperative-first design at each phase — coordinator role is labeled "Temporary", each member's equal voting weight is shown, and the receipt card explicitly names Carol as the closing member.

To present over Zoom: share the browser tab. The UI is dark-themed and readable at 1080p.

## Recording a Demo (asciinema)

```bash
bash demo/scripts/record-demo.sh
```

Plays back with: `asciinema play demo/recordings/governance-demo.cast`

## Troubleshooting

### "Gateway not reachable" / connection refused

**Cause:** Gateway isn't running or isn't on port 8080.

```bash
# Check if gateway is running
pgrep -f icnd
# Check what's on port 8080
ss -tlnp | grep :8080
# Check gateway logs
cat /tmp/icn-demo/gateway.log | tail -20
```

**Fix:** Kill stale processes and restart:
```bash
pkill -9 -f icnd
bash demo/scripts/start-demo.sh /tmp/icn-demo-fresh
```

### "could not acquire lock" / sled lock error

**Cause:** A previous gateway process didn't shut down cleanly and left a lock file.

**Fix:** Use a fresh data directory:
```bash
pkill -9 -f icnd
bash demo/scripts/start-demo.sh /tmp/icn-demo-$(date +%s)
```

### "JWT secret not configured"

**Cause:** The `ICN_GATEWAY_JWT_SECRET` env var or `--gateway-jwt-secret` flag is missing.

**Fix:** The demo scripts set this automatically. If running manually:
```bash
ICN_KEYSTORE_PASSPHRASE=demo \
ICN_GATEWAY_JWT_SECRET=demo-secret-key-for-testing-only-32bytes \
./target/debug/icnd \
  --gateway-enable \
  --gateway-bind 0.0.0.0:8080 \
  --gateway-jwt-secret demo-secret-key-for-testing-only-32bytes \
  --data-dir /tmp/icn-demo
```

### Browser demo shows "Error: Failed to fetch" or CORS error

**Cause:** The browser demo must be served from the gateway (same origin). Opening the HTML file directly from disk (`file://`) will fail due to CORS.

**Fix:** Access via `http://localhost:8080/static/demo.html` (not `file://`).

### Python script fails with "ModuleNotFoundError: cryptography"

**Fix:**
```bash
pip install cryptography
# or with pip3 explicitly:
pip3 install cryptography
```

### Demo shows stale data from a previous run

**Fix:** Always use a fresh data directory. The demo scripts handle this automatically, but if running manually:
```bash
rm -rf /tmp/icn-demo && bash demo/scripts/start-demo.sh /tmp/icn-demo
```

### Build fails with SIGSEGV

**Cause:** Incremental compilation cache corruption (known issue on this machine).

**Fix:**
```bash
cd icn && cargo clean && cargo build -p icnd
```

## Cleanup

```bash
pkill -f icnd
rm -rf /tmp/icn-demo /tmp/icn-demo-*
```

## File Reference

| File | Purpose |
|------|---------|
| `demo/scripts/start-demo.sh` | Cold-start: build, init, start, health check |
| `demo/scripts/demo-governance.py` | 19-step governance flow (automated or `--presenter`) |
| `demo/scripts/present-governance.sh` | All-in-one presenter launcher |
| `demo/scripts/record-demo.sh` | Record terminal demo with asciinema |
| `icn/crates/icn-gateway/static/demo.html` | Browser-based demo UI |
| `demo/RUNBOOK.md` | This file |

---

## K3s Cluster Demo — Five-Flow Federation

Five cooperatives on the homelab K3s cluster (10.8.30.40–42).

**Prerequisites:** `kubectl` configured (`~/.kube/config`), access from icn-dev (10.8.30.45).

### Cooperative Registry

| Namespace | Cooperative | gRPC NodePort |
|-----------|-------------|---------------|
| icn-coop-alpha | BrightWorks Collective | 30651 |
| icn-coop-beta | River City Tool Library | 30658 |
| icn-coop-gamma | Harbor Homes | 30649 |
| icn-coop-delta | Finger Lakes CDN | 30655 |

HTTP gateways are accessed via `kubectl port-forward` (scripts call `demo_ports_up` automatically). No HTTP NodePorts are required.

### Before Demo Day (run once per day)

```bash
cd /home/ubuntu/projects/icn

# Reseed all federation state — seeds identity, trust, proposals, compute trust
bash demo/scripts/reseed-federation-demo.sh

# Verify pods
kubectl get pods -A | grep icn-coop | grep -v Running
# (should print nothing — all pods Running)
```

### Five-Flow Sequence (recommended order)

```bash
# Governance + Settlement (Brightworks — run back to back)
bash demo/scripts/flow-1-governance.sh --present
bash demo/scripts/flow-2-patronage.sh --present

# Federation agreement (River City, BrightWorks, Finger Lakes)
bash demo/scripts/flow-3-federation.sh --present

# Regulatory reporting (Harbor)
bash demo/scripts/flow-4-reporting.sh --present

# Commons compute — strongest closer (Finger Lakes CDN)
bash demo/scripts/flow-5-compute.sh --present
```

All flows support `--present` (pause-on-beat) or `--narrated` (full explanatory output).

### If Pods Have Restarted

```bash
# Keystore is restored automatically by init container on pod start.
# Reseed to restore in-memory state (trust graph, governance proposals):
bash demo/scripts/reseed-federation-demo.sh
```

### If the Binary Is Stale

```bash
# Build and push (from repo root)
TAG=$(date +%Y%m%d)
docker build -f Dockerfile.fast -t 10.8.30.40:30500/icn:$TAG .
docker push 10.8.30.40:30500/icn:$TAG

# Roll out
for ns in icn-coop-alpha icn-coop-beta icn-coop-gamma icn-coop-delta; do
  kubectl rollout restart deployment -n $ns
done
for ns in icn-coop-alpha icn-coop-beta icn-coop-gamma icn-coop-delta; do
  kubectl rollout status deployment -n $ns --timeout=120s
done
```

### Known Issues

None blocking as of Sprint 28. All Sprint 26 known issues (#1334 decision_hash gap,
#1335 clearing agreement ID) are resolved.

### Proof Endpoint — Signing Key Dependency

`GET /v1/gov/proposals/{id}/proof` requires the node to have generated a
`GovernanceProofV2` at proposal-close time. This only happens when the governance
actor was initialized with a signing key.

**Dependency chain:**
1. Node starts with a software-backed keystore (Age-encrypted, unlocked at startup)
2. `identity_bundle.keypair()` succeeds → signing key extracted
3. At proposal close, node signs a `GovernanceDecisionReceipt` → proof stored
4. Proof endpoint returns the attestation

**If proof endpoint returns 404:**
Check startup logs for:
```
WARN GovernanceProof signing key unavailable — proposals will close and execute without cryptographic attestations.
```
If present, the keystore was hardware-backed or not fully unlocked at start time.
The governance gate and allocation path still work correctly without signing; only
proof attestations are affected.

**Note:** Governance gate (Invariant 7) runs regardless of signing key state — allocation
effects are blocked or allowed based on vote outcome, not proof availability.

### Verify Cluster Health

```bash
kubectl get pods -A | grep icn-coop     # all 4 namespaces should show Running
# Gateway health check (runs curl inside the pod — no port-forward needed):
kubectl exec -n icn-coop-gamma deploy/icn-gamma -- curl -s localhost:8080/v1/health | python3 -m json.tool
```

### Flow 5 Compute Trust Note

Compute trust lives in the daemon's in-memory `TrustGraph` and resets on pod restart.
The reseed script re-seeds it via gRPC. If Flow 5 fails with trust score 0.0, run:

### Flow 5 Compute Queue Accumulation

The compute gossip store (`compute:submit` topic) is sled-backed and **persists across pod
restarts**. Each demo run adds one task entry. After several runs, the queue accumulates
stale entries and the executor processes them in order — so the demo task may show `Pending`
during the status check while older tasks drain ahead of it.

**This is expected behavior.** The executor IS live; it just claimed an earlier task first.

To observe your demo task actually completing, wait 15–30 seconds after submission:
```bash
# Check status manually after submission (replace HASH with the task_hash from Step 2):
curl -s -H "Authorization: Bearer $TOKEN" \
  http://localhost:18084/v1/compute/status/$TASK_HASH | python3 -m json.tool
```

To present cleanly without a stale queue: cancel stale tasks by hash before the demo, or
accept that `Pending` is a valid demo state — the admission gate passed, the executor is
live, and the architecture claim is true. Describe it to the audience as:
*"The task is admitted and queued. In a production environment without prior demo state,
you'd see it move to Completed within seconds."*

```bash
kubectl exec -n icn-coop-delta deploy/icn-delta -- \
  icnctl --endpoint "[::1]:5655" trust add \
  did:icn:zE5E8bz7XrJGr6WozTbUNfSN3he3sUqYaCo4jifFKi4Ln 0.85 \
  --label "compute-demo"
```

### Current State (Sprint 28 complete)

- **Flow 5 task execution**: Gossip fan-out bug fixed in Sprint 28. The compute actor now
  receives submitted tasks via gossip loopback and the CCL executor is live. Tasks move
  `Pending → Processing → Completed` in the K3s cluster. Admission gate and execution path
  are both real.
- **Settlement receipts from compute**: Generated on task completion. Full provenance chain
  `task_hash → execution_receipt → credit_settlement` is anchored. Expanding to distributed
  multi-executor nodes is the next scaling step.
