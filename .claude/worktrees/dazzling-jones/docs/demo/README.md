# ICN Demo: Start Here

**Last Updated**: 2026-02-11

---

## Canonical Demo Ports

| Mode | Gateway | UI | Notes |
|------|---------|----|----|
| **Local single-node** | http://localhost:8080 | http://localhost:3000 | One-click demo |
| **Devnet (3 nodes)** | http://localhost:8000, http://localhost:8001, http://localhost:8002 | - | Docker cluster |
| **LAN mode** | http://\<lan-ip\>:8080 | http://\<lan-ip\>:3000 | Cross-machine access |

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

Optional overrides:
- `ICN_DEMO_DATA_DIR`
- `ICN_DEMO_GATEWAY_HOST`
- `ICN_DEMO_GATEWAY_PORT`
- `ICN_DEMO_UI_PORT`
- `ICN_DEMO_COOP_ID`
- `ICN_DEMO_RPC_ENDPOINT`
- `ICN_DEMO_MDNS_ENABLED` (set `true` to enable mDNS discovery)

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

Nodes: node-a http://localhost:8000, node-b http://localhost:8001, node-c http://localhost:8002

### Reset Everything

```bash
./demo/scripts/reset-demo.sh
```

---

## Authentication

**Demo mode**: The one-click script generates a JWT token and displays it (when identity and coop are initialized in the demo data dir).

**Manual mode**: Set `ICN_GATEWAY_JWT_SECRET` env var (min 32 bytes):

```bash
ICN_GATEWAY_JWT_SECRET="0123456789abcdef0123456789abcdef" ./target/release/icnd --gateway-enable
```

API endpoints require auth header (except `/v1/health`):

```bash
curl -H "Authorization: Bearer $TOKEN" http://localhost:8080/v1/coops
```

---

## LAN Mode (Workstation Access)

To access the demo from another machine on your network:

### 1. Bind gateway to all interfaces

```bash
./target/release/icnd --gateway-enable --gateway-bind 0.0.0.0:8080
```

### 2. Set CORS origins for your LAN IP

```bash
export ICN_CORS_ORIGINS="http://10.8.10.45:3000,http://10.8.10.45:8080"
```

### 3. Start UI server on all interfaces

```bash
cd web/pilot-ui
python3 -m http.server 3000 --bind 0.0.0.0
```

### 4. Open firewall ports (if needed)

```bash
sudo ufw allow 8080/tcp
sudo ufw allow 3000/tcp
```

### 5. Access from workstation

- UI: `http://<server-lan-ip>:3000`
- Gateway: `http://<server-lan-ip>:8080`
- In UI login, use LAN IP for gateway URL

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
