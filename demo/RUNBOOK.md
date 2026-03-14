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

All 18 steps run without pausing. Exit code 0 = success, 1 = failure.

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

The demo is split into three phases:
- **Phase 1: Setup** — Create cooperative, generate identities, add members
- **Phase 2: Govern** — Submit proposal, cast votes
- **Phase 3: Verify** — Close vote, show tally, display cryptographic receipt

Press Enter between phases to control pacing.

## Browser Demo

After starting the gateway (via either script), open:

```
http://localhost:8080/static/demo.html
```

The browser demo has three "Run" buttons matching the three phases. Click each one in sequence. No terminal needed.

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
| `demo/scripts/demo-governance.py` | 18-step governance flow (automated or `--presenter`) |
| `demo/scripts/present-governance.sh` | All-in-one presenter launcher |
| `demo/scripts/record-demo.sh` | Record terminal demo with asciinema |
| `icn/crates/icn-gateway/static/demo.html` | Browser-based demo UI |
| `demo/RUNBOOK.md` | This file |
