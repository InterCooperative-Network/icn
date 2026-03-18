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

## K3s Cluster Demo (4-Flow Federation)

For the full 4-coop federation demo running on the homelab K3s cluster.

**Prerequisites:** `kubectl` configured (`~/.kube/config`, server `10.8.30.40`), Python 3 + `cryptography`, ports 18081–18084 free.

### Port Mapping

| Namespace | Coop | Port |
|-----------|------|------|
| icn-coop-alpha | BrightWorks Cooperative | 18081 |
| icn-coop-beta  | River City Tool Library | 18082 |
| icn-coop-gamma | Harbor Homes Cooperative | 18083 |
| icn-coop-delta | Finger Lakes CDN | 18084 |

### Quick Start

```bash
cd /home/ubuntu/projects/icn

# Reseed canonical state (manages port-forwards automatically)
bash demo/scripts/reseed-federation-demo.sh

# Full 19-step governance demo (proof included)
python3 demo/scripts/demo-governance.py http://localhost:18081 --presenter

# Four federation flows
bash demo/scripts/flow-1-governance.sh --present    # Harbor Homes capital vote
bash demo/scripts/flow-2-patronage.sh --present     # BrightWorks patronage distribution
bash demo/scripts/flow-3-federation.sh --present    # Cross-coop federation agreement
bash demo/scripts/flow-4-reporting.sh --present     # Funder-facing audit trail
```

Reseed before each flow run — flows consume state that was seeded.

### If Pods Have Restarted

The `copy-keystore` init container runs automatically on every pod start. No manual keystore copy needed. Just reseed to restore in-memory state:

```bash
bash demo/scripts/reseed-federation-demo.sh
```

### If the Binary Is Stale

Build from pre-compiled binaries (`icn/target/release/icnd` must exist):

```bash
# Build and push (from repo root)
TAG=$(date +%Y%m%d)
docker build -f Dockerfile.fast -t 10.8.30.40:30500/icn:$TAG .
docker push 10.8.30.40:30500/icn:$TAG

# Roll out (imagePullPolicy: IfNotPresent — new tag required, not :latest)
for ns_deploy in "icn-coop-alpha icn-alpha" "icn-coop-beta icn-beta" "icn-coop-gamma icn-gamma" "icn-coop-delta icn-delta"; do
  ns=$(echo $ns_deploy | cut -d" " -f1); deploy=$(echo $ns_deploy | cut -d" " -f2)
  kubectl set image deployment/$deploy icnd=10.8.30.40:30500/icn:$TAG -n $ns
done
for ns in icn-coop-alpha icn-coop-beta icn-coop-gamma icn-coop-delta; do
  kubectl rollout status deployment -n $ns --timeout=120s
done
```

### Known Issues (non-blocking)

- **#1334** — Flow 2 Step 11: `/v1/receipts/allocations` returns 400 (`missing decision_hash`). Settlement succeeds; only receipt chain query fails.
- **#1335** — Flow 3: clearing agreement ID shown as `(failed)`. Both coops ratify; Flow 4 runs without the ID.

### Verify Cluster Health

```bash
kubectl get pods -A | grep icn-coop          # all 4 should be Running
curl -s http://localhost:18081/v1/health      # requires port-forward active
```
