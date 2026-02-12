# ICN Quick Start

**Time to running**: 5 minutes
**Prerequisites**: Git, Rust toolchain (1.88+), Python 3

---

## Option A: One-Click Demo (Recommended)

```bash
# 1. Clone
git clone https://github.com/InterCooperative-Network/icn.git
cd icn

# 2. Build
cd icn && cargo build --release
cd ..

# 3. Run demo
./demo/scripts/run-tool-library-demo.sh

# 4. Open browser
# → http://localhost:3000
# → Use credentials shown in terminal
```

**That's it.** The demo script handles everything.

---

## Option B: Manual Setup

### Step 1: Build

```bash
git clone https://github.com/InterCooperative-Network/icn.git
cd icn/icn
cargo build --release
```

### Step 2: Initialize Identity

```bash
./target/release/icnctl id init
# Enter passphrase when prompted (e.g., "demo123")
```

### Step 3: Start Daemon

```bash
./target/release/icnd --gateway-enable
```

### Step 4: Start UI

```bash
# New terminal
cd ../web/pilot-ui
python3 -m http.server 3000
```

### Step 5: Get Auth Token

```bash
# New terminal
cd icn
./target/release/icnctl auth token --coop-id my-coop
```

### Step 6: Open Browser

- Go to http://localhost:3000
- Sign in with displayed credentials

---

## Verify Everything Works

```bash
# Health check
curl http://localhost:8080/v1/health

# Should return: {"status":"ok"}
```

---

## What's Running

| Service | URL | Purpose |
|---------|-----|---------|
| Daemon | - | Core ICN node |
| Gateway | http://localhost:8080 | REST API |
| UI | http://localhost:3000 | Web interface |

---

## Next Steps

- **Explore the UI**: Dashboard, Governance, Transactions
- **Read the Demo Script**: `docs/demo/DEMO_SCRIPT.md`
- **Understand Architecture**: `docs/ARCHITECTURE.md`
- **Join a Pilot**: Contact the team

---

## Troubleshooting

### Build fails

```bash
# Check Rust version
rustc --version  # Needs 1.88+

# Update if needed
rustup update
```

### Port already in use

```bash
# Find what's using the port
lsof -i :8080
lsof -i :3000

# Kill or use different ports
```

### Passphrase issues

```bash
# Reset identity
rm -rf ~/.icn
./target/release/icnctl id init
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
- **Disk**: 500 MB for build artifacts
- **Rust**: 1.88.0 or later
- **Python**: 3.x (for UI server)

---

## Getting Help

- **Docs**: `docs/` directory
- **Demo Details**: `demo/README.md`
- **Architecture**: `docs/ARCHITECTURE.md`
