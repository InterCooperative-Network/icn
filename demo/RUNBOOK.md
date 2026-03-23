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

| Namespace | Cooperative | NodePort |
|-----------|-------------|----------|
| icn-coop-brightworks | Brightworks Collective | 30081 |
| icn-coop-harbor | Harbor Freight Workers Cooperative | 30083 |
| icn-coop-clearinghouse | Rochester Cooperative Clearinghouse | 30082 |
| icn-coop-newengland | New England Mesh Network | 30084 |
| icn-coop-delta | Finger Lakes CDN | 30085 |

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

# Clearinghouse mutual credit (Rochester)
bash demo/scripts/flow-3-clearinghouse.sh --present

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
for ns in icn-coop-brightworks icn-coop-harbor icn-coop-clearinghouse icn-coop-newengland icn-coop-delta; do
  kubectl rollout restart deployment -n $ns
done
for ns in icn-coop-brightworks icn-coop-harbor icn-coop-clearinghouse icn-coop-newengland icn-coop-delta; do
  kubectl rollout status deployment -n $ns --timeout=120s
done
```

### Known Issues

None blocking as of Sprint 27. All Sprint 26 known issues (#1334 decision_hash gap,
#1335 clearing agreement ID) are resolved.

### Verify Cluster Health

```bash
kubectl get pods -A | grep icn-coop     # all 5 namespaces should show Running
# Gateway health check (requires port-forward or NodePort access):
curl -s http://10.8.10.40:30081/v1/health | python3 -m json.tool
```

### Flow 5 Compute Trust Note

Compute trust lives in the daemon's in-memory `TrustGraph` and resets on pod restart.
The reseed script re-seeds it via gRPC. If Flow 5 fails with trust score 0.0, run:

```bash
kubectl exec -n icn-coop-delta deploy/icn-delta -- \
  icnctl --endpoint "[::1]:5655" trust add \
  did:icn:zE5E8bz7XrJGr6WozTbUNfSN3he3sUqYaCo4jifFKi4Ln 0.85 \
  --label "compute-demo"
```

### Current Limitations (Sprint 28 work)

- **Flow 5 task execution**: compute tasks are admitted (trust gate proven) but remain `Pending`.
  No executor node is registered in K3s. Executor wiring is Sprint 28 scope.
- **Settlement receipts from compute**: requires task completion; Sprint 28 dependency.
