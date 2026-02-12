# ICN Demo: Start Here

**Last Updated**: 2026-02-11

---

## Canonical Demo Ports

| Mode | Gateway | UI | Notes |
|------|---------|----|----|
| **Local single-node** | http://localhost:8080 | http://localhost:3000 | One-click demo |
| **Devnet (3 nodes)** | :8000, :8001, :8002 | - | Docker cluster |

---

## Fastest Path: One-Click Demo

```bash
# From repository root
./demo/scripts/run-tool-library-demo.sh
```

Then open:
- **UI**: http://localhost:3000
- **Gateway**: http://localhost:8080/v1/health

Use the credentials printed by the script.

---

## What This Demo Proves

| Feature | What You'll See |
|---------|-----------------|
| **Identity** | Member-controlled DID + keys (no corporate account) |
| **Governance** | Proposals → votes → close → cryptographic proof |
| **Ledger** | Mutual credit journaling + deterministic receipts |
| **Trust** | Attestations → trust-gated access/resource policy |

---

## Demo Modes

### Local Single-Node (5 minutes)

```bash
./demo/scripts/run-tool-library-demo.sh
```

Starts daemon + gateway + UI. Displays credentials. Press Ctrl+C to stop.

### Devnet 3-Node Cluster (10 minutes)

```bash
cd deploy/devnet
make build && make up
make status   # verify all healthy
make demo     # run demo against node-a
```

Nodes: node-a :8000, node-b :8001, node-c :8002

### Reset Everything

```bash
./demo/scripts/reset-demo.sh
```

---

## Authentication

**Demo mode**: The one-click script generates a JWT token and displays it.

**Manual mode**: Set `ICN_GATEWAY_JWT_SECRET` env var (min 32 bytes):

```bash
ICN_GATEWAY_JWT_SECRET="demo-secret-at-least-32-bytes!!" ./target/release/icnd --gateway-enable
```

All `curl` examples require:

```bash
-H "Authorization: Bearer $TOKEN"
```

---

## Documentation

| Document | Purpose |
|----------|---------|
| [DEMO_SCRIPT.md](DEMO_SCRIPT.md) | 20-minute presenter walkthrough with timing |
| [QUICK_START.md](QUICK_START.md) | 5-minute clone-to-running |
| [ARCHITECTURE_OVERVIEW.md](ARCHITECTURE_OVERVIEW.md) | Visual diagrams for all subsystems |
| [FAQ.md](FAQ.md) | Talking points for all audiences |

---

## Troubleshooting

### Gateway not responding

```bash
curl http://localhost:8080/v1/health
# Should return {"status":"ok"}
```

If not, check if daemon is running or restart the demo.

### Port already in use

```bash
lsof -i :8080
lsof -i :3000
```

Stop conflicting processes or use `./demo/scripts/reset-demo.sh`.

### Build fails

```bash
rustc --version  # Needs 1.88+
rustup update
```

### Full reset

```bash
./demo/scripts/reset-demo.sh
./demo/scripts/run-tool-library-demo.sh
```

---

## System Requirements

- **OS**: Linux or macOS
- **RAM**: 2 GB minimum
- **Rust**: 1.88.0+
- **Python**: 3.x (for UI server)

---

## Further Reading

- **Full Architecture**: `docs/ARCHITECTURE.md`
- **Getting Started**: `docs/GETTING_STARTED.md`
- **API Reference**: `docs/api/`
- **Demo Infrastructure**: `demo/README.md`
