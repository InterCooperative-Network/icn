# ICN Gateway - Quick Start Guide

## Fastest Way to Start

```bash
cd icn

# Option 1: Use the startup script (recommended)
./start-gateway.sh

# Option 2: Set secrets and run manually
export ICN_GATEWAY_JWT_SECRET="demo-secret"
export ICN_KEYSTORE_PASSPHRASE="your-passphrase-here"  # preferred
./target/debug/icnd --gateway-enable --gateway-bind 127.0.0.1:9090
```

Then open your browser to: **http://localhost:9090**

Note: Port 9090 avoids common local conflicts on 8080.

## Prerequisites

Build required binaries from the Rust workspace:

```bash
cd icn
cargo build --bin icnd --bin icnctl
```

If you do not have an identity yet:

```bash
# Create a new identity (prompts for passphrase)
./target/debug/icnctl id init

# Show your DID (needed for login)
./target/debug/icnctl id show
```

## Environment Variables

| Variable | Purpose | Example |
|----------|---------|---------|
| `ICN_KEYSTORE_PASSPHRASE` | Keystore passphrase (preferred) | `"my-secure-pass"` |
| `ICN_PASSPHRASE` | Legacy passphrase env var (fallback) | `"my-secure-pass"` |
| `ICN_GATEWAY_JWT_SECRET` | JWT signing secret | `"demo-secret-123"` |
| `ICN_STATIC_DIR` | Custom static files path | `"/custom/path"` |

## Starting the Server

### Method 1: Interactive (prompt for passphrase)

```bash
export ICN_GATEWAY_JWT_SECRET="demo-secret"
./target/debug/icnd --gateway-enable --gateway-bind 127.0.0.1:9090
# Enter passphrase when prompted
```

### Method 2: Non-interactive (preferred env var)

```bash
export ICN_KEYSTORE_PASSPHRASE="your-passphrase"
export ICN_GATEWAY_JWT_SECRET="demo-secret"
./target/debug/icnd --gateway-enable --gateway-bind 127.0.0.1:9090
```

### Method 3: Isolated test data directory (no existing identity)

```bash
mkdir -p /tmp/icn-demo
export ICN_GATEWAY_JWT_SECRET="demo-secret"
./target/debug/icnd --data-dir /tmp/icn-demo --gateway-enable --gateway-bind 127.0.0.1:9090

# This runs with limited functionality because no keystore exists in /tmp/icn-demo.
```

## Using the Web UI

1. Open browser: http://localhost:9090
2. Login screen appears.
3. Enter credentials:
   - DID: from `./target/debug/icnctl id show`
   - Cooperative ID: any string (for example `test-coop`)
4. Explore:
   - Dashboard
   - Cooperatives
   - Governance
   - Ledger

## Troubleshooting

### Server won't start / connection refused

```bash
# Check process
ps aux | grep icnd

# Run with logs
./target/debug/icnd --gateway-enable 2>&1 | tee server.log
```

### Forgot your passphrase?

```bash
# If you cannot recover the passphrase, remove keystore and create a new identity.
# Default Linux keystore path from Config::default() is ~/.local/share/icn/identity.age
rm ~/.local/share/icn/identity.age
./target/debug/icnctl id init
```

### Port 9090 already in use

```bash
./target/debug/icnd --gateway-enable --gateway-bind 127.0.0.1:8080
# Then visit: http://localhost:8080
```

### Keystore not found

```bash
# Default data dir on Linux
ls -la ~/.local/share/icn/

# If missing, create identity
./target/debug/icnctl id init
```

## Common Commands

```bash
# Build everything (from icn/ workspace)
cargo build

# Build just daemon and CLI
cargo build --bin icnd --bin icnctl

# Run tests
cargo test

# Show DID
./target/debug/icnctl id show

# Create a backup
./target/debug/icnctl backup create backup.tar.gz.age

# Check health endpoint
curl http://localhost:9090/v1/health
```

## Customizing the UI

The default web UI files are in:

```text
crates/icn-gateway/static/
├── index.html      # Main HTML
├── style.css       # Styles
├── app.js          # Application logic + API calls
└── README.md       # UI notes
```

Edit these files and reload the browser (no frontend build step required for these static assets).

## Next Steps

- Create a cooperative in the UI
- Set up governance domains
- Make ledger transactions
- Explore API details at `/v1/health/detailed`

## Need Help?

- Check server logs in the terminal running `icnd`
- Open browser devtools console for UI errors
- See `crates/icn-gateway/static/README.md` for UI details

---

Ready to start? Run: `./start-gateway.sh`
