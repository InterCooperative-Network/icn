# ICN Demo: Start Here

**Last Updated**: 2026-07-13

---

## Rehearsal Node v0.1 (current headline: the two-role organizer→member loop)

> **DEV/DEMO only — local, single-node, fictional institution data.** Not
> production, not federation, not a formal pilot, no real member or funds
> data. Since #2406–#2408 the headline surface is the **two-role rehearsal
> loop**: an organizer reviews fictional proposed work in a browser, approves,
> previews a digest-bound plan, and confirms (creating one real local action
> item through the ADR-0026 receipt ladder); a fresh least-privilege member
> session completes it and the completion receipt + value-withheld evidence
> validate. Witnessed on a fresh assembled image at `8c0fe926` (2026-07-13).
> Start with [rehearsal-node-appliance-loop.md](rehearsal-node-appliance-loop.md)
> and [ICN_REHEARSAL_NODE_V0.1_RUNBOOK.md](ICN_REHEARSAL_NODE_V0.1_RUNBOOK.md).

## July Demo Candidate 0.1 (single-actor proof loop — superseded as headline)

> **DEV/DEMO only — local, single-node, single-actor, fictional institution
> data.** Not production, not federation, not a formal pilot, no real member or
> funds data. It demonstrates the cooperative-participation spine — standing →
> action card → discharge → receipt → evidence/audit — as **proof of path, not
> deployment readiness.**

| Doc | Use it for |
|-----|-----------|
| [JULY_DEMO_CANDIDATE_0.1_RELEASE_PACKET.md](JULY_DEMO_CANDIDATE_0.1_RELEASE_PACKET.md) | **Read first — hand-off packet** (a candidate hand-off for review, *not* a shipped software release) — what it is/proves/doesn't, who it's for, how to run, how to review evidence, safe-to-share vs never-share, known gaps, next lanes. |
| [JULY_DEMO_HANDS_ON.md](JULY_DEMO_HANDS_ON.md) | **Detailed presenter/operator flow** — click-by-click, "what to say" at each step, and the full failure-mode table. |
| [JULY_DEMO_OPERATOR_CHECKLIST.md](JULY_DEMO_OPERATOR_CHECKLIST.md) | **Keep-open live-demo card** — preflight, launch command, expected states, and panic fixes. |
| [JULY_DEMO_CANDIDATE_0.1_OPERATOR_SCRIPT.md](JULY_DEMO_CANDIDATE_0.1_OPERATOR_SCRIPT.md) | **Reviewer / evidence-hygiene handoff** — claim boundary by proof level, secret/evidence capture, known-gaps map, reviewer checklist. |
| [deploy/appliance/DEMO_QUICKSTART.md](../../deploy/appliance/DEMO_QUICKSTART.md) | **Appliance build/run** — image build, the one-command launcher, and the manual fallback. |
| [ICN_REHEARSAL_NODE_V0.1_RUNBOOK.md](ICN_REHEARSAL_NODE_V0.1_RUNBOOK.md) | **Rehearsal Node v0.1 operator runbook** — the named wrapper entrypoint over the appliance DEV/DEMO profile (smoke an image, open a running node, print verification steps), what it proves / does not prove, and how it relates to the other run paths. |
| [JULY_DEMO_CANDIDATE_0.1_ACCESSIBILITY_WALKTHROUGH.md](JULY_DEMO_CANDIDATE_0.1_ACCESSIBILITY_WALKTHROUGH.md) | **Accessibility + rendered-browser evidence** — the 12-category organizer/member gate outcome, automated axe + keyboard results, what a reviewer can verify, and the human screen-reader/zoom pass still owed. |

> The sections below describe the older one-click tool-library / devnet demo,
> not the July Candidate appliance.

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

Set `ICN_LAN_HOST` to the workstation/LAN address that browsers use to reach the
UI and gateway (this is the request Origin that must be allow-listed — not
necessarily the in-cluster gateway host).

```bash
export ICN_CORS_ORIGINS="http://${ICN_LAN_HOST}:3000,http://${ICN_LAN_HOST}:8080"
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
| [ICN_SYSTEM_DEMO_READINESS_MAP.md](ICN_SYSTEM_DEMO_READINESS_MAP.md) | Demo-readiness diagnosis + planned PR sequence (banner / landing / cards / receipts / fixture mode) |
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
