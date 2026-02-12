# ICN Demo Scripts & Data

This directory contains everything needed to run ICN demos.

## Canonical Demo Ports

- **Local single-node**: Gateway http://localhost:8080, UI http://localhost:3000
- **Devnet (3 nodes)**: node-a http://localhost:8000, node-b http://localhost:8001, node-c http://localhost:8002
- **LAN mode**: Gateway http://\<lan-ip\>:8080, UI http://\<lan-ip\>:3000 (set `ICN_CORS_ORIGINS`)

## Quick Start

```bash
# From repository root
./demo/scripts/run-tool-library-demo.sh
```

Then open http://localhost:3000 and login with the displayed credentials.

---

## Demo Documentation

| Document | Purpose |
|----------|---------|
| [Start Here](../docs/demo/README.md) | Front door - what to run, which mode |
| [DEMO_SCRIPT.md](../docs/demo/DEMO_SCRIPT.md) | 20-minute presenter walkthrough |
| [QUICK_START.md](../docs/demo/QUICK_START.md) | 5-minute setup guide |
| [ARCHITECTURE_OVERVIEW.md](../docs/demo/ARCHITECTURE_OVERVIEW.md) | Visual architecture diagrams |
| [FAQ.md](../docs/demo/FAQ.md) | Common questions & talking points |

---

## Directory Structure

```
demo/
├── scripts/          # Executable demo scripts
│   ├── run-tool-library-demo.sh    # Main demo runner (one-click)
│   ├── setup-demo-env.sh           # Environment setup
│   ├── verify-demo.sh              # Verification (13 checks)
│   ├── quick-test.sh               # Pre-demo readiness test
│   ├── reset-demo.sh               # Clean and reset
│   ├── load-sample-data.sh         # Load members/transactions
│   └── test-ui-integration.sh      # UI integration testing
├── data/             # Sample data files
│   ├── tool-library-members.json   # 12 member profiles
│   └── tool-library-history.json   # 10 sample transactions
├── configs/          # Demo configurations
│   └── tool-library.toml           # Tool library demo config
└── docs/             # Documentation
    ├── API_INTEGRATION.md          # API endpoint reference
    └── UI_FIXES_APPLIED.md         # Recent fixes log
```

---

## Scripts

### 1. run-tool-library-demo.sh ⭐ **Use This First**

**Purpose:** One-click demo startup

**What it does:**
- Starts ICN daemon with gateway
- Generates authentication token
- Starts pilot UI
- Displays all access information
- Keeps everything running until Ctrl+C

**Usage:**
```bash
./demo/scripts/run-tool-library-demo.sh
```

**Output:**
- Gateway: http://localhost:8080
- UI: http://localhost:3000
- Login credentials displayed
- JWT token for authentication

### 2. quick-test.sh

**Purpose:** Pre-demo verification

**What it tests:**
- Infrastructure (binaries, files, data)
- Services (gateway, UI)
- API endpoints (health, auth, cooperative)
- UI integration (fixed endpoints)

**Usage:**
```bash
./demo/scripts/quick-test.sh
```

**Exit codes:**
- 0: All tests passed, ready to demo
- 0: Warnings but usable
- 1: Failed tests, fix before demo

### 3. verify-demo.sh

**Purpose:** Comprehensive verification (13 checks)

**Usage:**
```bash
./demo/scripts/verify-demo.sh
```

**Checks:**
- Build system
- Binaries exist
- Demo infrastructure
- Sample data
- UI files
- Configuration files

### 4. reset-demo.sh

**Purpose:** Clean everything and start fresh

**What it does:**
- Stops all services (daemon, UI)
- Deletes demo data directory
- Cleans logs
- Creates fresh identity
- Ready for clean demo run

**Usage:**
```bash
./demo/scripts/reset-demo.sh
```

**WARNING:** Deletes all demo data!

### 5. setup-demo-env.sh

**Purpose:** Initialize demo environment

**Usage:**
```bash
./demo/scripts/setup-demo-env.sh
```

### 6. load-sample-data.sh

**Purpose:** Load sample members and transactions

**Usage:**
```bash
./demo/scripts/load-sample-data.sh
```

**Status:** Currently a guide - requires member identity creation

### 7. test-ui-integration.sh

**Purpose:** UI → API integration test guide

**Usage:**
```bash
./demo/scripts/test-ui-integration.sh
```

---

## Sample Data

### tool-library-members.json

**12 realistic members:**
- Alice Chen (Tool Coordinator)
- Bob Martinez (Member)
- Carol Johnson (Member)
- David Lee (Treasurer)
- Elena Rodriguez (Member)
- Frank Wilson (Member)
- Grace Park (Board Member)
- Henry Brown (Member)
- Isabel Garcia (Member)
- Jack Thompson (Member)
- Kelly O'Brien (Member)
- Luis Sanchez (Member)

**Data includes:**
- Names and roles
- Skills (3 per member)
- Contact information
- Join dates

### tool-library-history.json

**10 sample transactions:**
- Date range: Nov 1 - Dec 10, 2024
- Various activities (instruction, repair, maintenance)
- Hour amounts: 1.0 to 3.0 hours
- All confirmed status

---

## Configuration

### tool-library.toml

**Demo configuration for:**
- Rochester Tool Library cooperative
- Network: 127.0.0.1:7777
- API: 127.0.0.1:5601
- Gateway: 127.0.0.1:8080
- Metrics: 127.0.0.1:9100
- Timebank ledger type
- CORS enabled for localhost:3000

---

## Typical Demo Flow

### Step 1: Verify Readiness
```bash
./demo/scripts/quick-test.sh
```

### Step 2: Start Demo
```bash
./demo/scripts/run-tool-library-demo.sh
```

### Step 3: Access UI
1. Open http://localhost:3000
2. Click "Sign In"
3. Enter displayed credentials
4. Copy/paste JWT token
5. Click "Sign In"

### Step 4: Demo Features
- View balance (0.0 hours initially)
- Browse transaction history
- Check member directory
- Test transaction creation
- Explore governance features

### Step 5: Stop Demo
Press Ctrl+C in terminal running demo script

### Step 6: Reset (Optional)
```bash
./demo/scripts/reset-demo.sh
```

---

## Troubleshooting

### "Gateway not responding"
```bash
# Check if daemon is running
curl http://localhost:8080/v1/health

# If not, check logs
tail -f /tmp/icnd-demo.log

# Restart demo
./demo/scripts/run-tool-library-demo.sh
```

### "UI not loading"
```bash
# Check if port 3000 is available
lsof -i :3000

# Check logs
tail -f /tmp/pilot-ui-demo.log

# Restart demo
./demo/scripts/run-tool-library-demo.sh
```

### "Token expired" or "Authentication failed"
```bash
# Get fresh token
cd icn
ICN_PASSPHRASE=demo123 ./target/release/icnctl \
  -d "$(pwd)/.demo-data/tool-library" \
  -e 127.0.0.1:15602 \
  auth token \
  --coop-id rochester-tool-library \
  --scopes "coop:write,coop:read,ledger:read,ledger:write"
```

### "Can't create transaction"
Ensure you have multiple members in the cooperative. The founder can log hours, but needs recipients to send to.

### Complete Reset
```bash
./demo/scripts/reset-demo.sh
./demo/scripts/run-tool-library-demo.sh
```

---

## Environment

### Default Locations
- **Repo root:** `<this-repo>`
- **Data Directory:** `<repo>/.demo-data/tool-library`
- **RPC Endpoint:** `127.0.0.1:15602`
- **Gateway:** `http://localhost:8080`
- **UI:** `http://localhost:3000`

You can override defaults with:
- `ICN_DEMO_DATA_DIR`
- `ICN_DEMO_GATEWAY_HOST`
- `ICN_DEMO_GATEWAY_PORT`
- `ICN_DEMO_UI_PORT`
- `ICN_DEMO_COOP_ID`
- `ICN_DEMO_RPC_ENDPOINT`
- `ICN_DEMO_MDNS_ENABLED` (default `false` for constrained environments)

### Default Credentials
- **Cooperative:** `rochester-tool-library`
- **DID:** `did:icn:zBFnhJhgvRjgukhQmkq9ddBz5wiEt32ptkQkBDjWx6uPh`
- **Passphrase:** `demo123`

**NOTE:** DID will change if you run reset-demo.sh

---

## Development

### Adding New Demo Scenarios

1. Create new sample data JSON files in `demo/data/`
2. Create new config in `demo/configs/`
3. Copy and modify `run-tool-library-demo.sh`
4. Update this README

### Testing Changes

```bash
# Full verification
./demo/scripts/verify-demo.sh

# Quick test
./demo/scripts/quick-test.sh

# Manual test
./demo/scripts/run-tool-library-demo.sh
```

---

## Documentation

### In This Directory
- `docs/API_INTEGRATION.md` - Complete API reference
- `docs/UI_FIXES_APPLIED.md` - Recent bug fixes

### In Main Docs
- `/docs/ARCHITECTURE.md` - System architecture
- `/docs/GETTING_STARTED.md` - Developer guide
- `/web/pilot-ui/*.md` - UI documentation

---

## Support

### If Demo Fails

1. Check logs: `/tmp/icnd-demo.log` and `/tmp/pilot-ui-demo.log`
2. Run verification: `./demo/scripts/quick-test.sh`
3. Try reset: `./demo/scripts/reset-demo.sh`
4. Check documentation in repository root

### Common Issues
- **Port conflicts:** Kill existing processes or use different ports
- **Passphrase issues:** Ensure "demo123" is set correctly
- **Token issues:** Regenerate token with icnctl
- **Data issues:** Run reset-demo.sh for clean state

---

## Status

**Created:** 2025-12-18  
**Version:** 1.0  
**Completeness:** 95%  
**Ready for:** Demo testing and use

**Complete:**
- ✅ All core scripts
- ✅ Sample data
- ✅ Configuration
- ✅ Documentation
- ✅ Verification

**TODO:**
- ⏳ Automated member addition
- ⏳ Historical transaction creation
- ⏳ Multi-node demo script

---

**Quick Commands:**

```bash
# Run demo
./demo/scripts/run-tool-library-demo.sh

# Test readiness
./demo/scripts/quick-test.sh

# Reset everything
./demo/scripts/reset-demo.sh

# Verify infrastructure
./demo/scripts/verify-demo.sh
```

**Happy demoing!** 🎉
