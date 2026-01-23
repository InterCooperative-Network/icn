# ICN Gateway - Quick Start Guide

## 🚀 Fastest Way to Start

```bash
cd icn

# Option 1: Use the startup script (recommended)
./start-gateway.sh

# Option 2: Set passphrase and run manually
export ICN_PASSPHRASE="your-passphrase-here"
export ICN_GATEWAY_JWT_SECRET="demo-secret"
./target/debug/icnd --gateway-enable --gateway-bind 127.0.0.1:9090
```

Then open your browser to: **http://localhost:9090**

> **Note**: Port 9090 is used to avoid conflicts with common development servers on port 8080.

## 📋 Prerequisites

### If you don't have an identity yet:

```bash
# Create a new identity
./target/debug/icnctl id init
# (It will prompt you to create a passphrase)

# Show your DID (you'll need this for login)
./target/debug/icnctl id show
```

## 🔑 Environment Variables

| Variable | Purpose | Example |
|----------|---------|---------|
| `ICN_PASSPHRASE` | Your keystore passphrase | `"my-secure-pass"` |
| `ICN_GATEWAY_JWT_SECRET` | JWT signing secret | `"demo-secret-123"` |
| `ICN_STATIC_DIR` | Custom static files path | `"/custom/path"` |

## 🎯 Starting the Server

### Method 1: Interactive (with passphrase prompt)
```bash
export ICN_GATEWAY_JWT_SECRET="demo-secret"
./target/debug/icnd --gateway-enable --gateway-bind 127.0.0.1:9090
# Enter passphrase when prompted
```

### Method 2: Non-interactive (background)
```bash
export ICN_PASSPHRASE="your-passphrase"
export ICN_GATEWAY_JWT_SECRET="demo-secret"
./target/debug/icnd --gateway-enable --gateway-bind 127.0.0.1:9090
```

### Method 3: Without Identity (testing only)
```bash
# Temporarily rename keystore
mv ~/.icn/identity.age ~/.icn/identity.age.backup

export ICN_GATEWAY_JWT_SECRET="demo-secret"
./target/debug/icnd --gateway-enable --gateway-bind 127.0.0.1:9090

# Gateway will run with limited functionality
# Restore: mv ~/.icn/identity.age.backup ~/.icn/identity.age
```

## 🌐 Using the Web UI

1. **Open Browser**: Navigate to http://localhost:9090

2. **Login Screen**: You'll see a clean login form

3. **Enter Credentials**:
   - **DID**: Your decentralized identifier (get with `icnctl id show`)
   - **Cooperative ID**: Any string (e.g., "test-coop", "demo")

4. **Explore**:
   - 📊 **Dashboard** - Overview and metrics
   - 🏘️ **Cooperatives** - Create and manage coops
   - 🗳️ **Governance** - Proposals and voting
   - 💰 **Ledger** - Balances and transactions

## 🐛 Troubleshooting

### Server won't start - "Connection Refused"
```bash
# Check if server is running
ps aux | grep icnd

# Check logs for errors
./target/debug/icnd --gateway-enable 2>&1 | tee server.log
```

### Forgot your passphrase?
```bash
# Create new identity (old one will be lost)
rm ~/.icn/identity.age
./target/debug/icnctl id init
```

### Port 9090 already in use?
```bash
# Use a different port (e.g., 8080 if available)
./target/debug/icnd --gateway-enable --gateway-bind 127.0.0.1:8080
# Then visit: http://localhost:8080
```

### "Keystore not found" error?
```bash
# Check if keystore exists
ls -la ~/.icn/

# If not, create identity
./target/debug/icnctl id init
```

## 📝 Common Commands

```bash
# Build everything
cargo build

# Build just the daemon
cargo build --bin icnd

# Build just the CLI tool
cargo build --bin icnctl

# Run all tests
cargo test

# Show your DID
./target/debug/icnctl id show

# Create a backup
./target/debug/icnctl backup create backup.tar.gz.age

# Check server health
curl http://localhost:9090/v1/health
```

## 🎨 Customizing the UI

The web UI files are located in:
```
crates/icn-gateway/static/
├── index.html       # Main HTML
├── css/style.css    # Styles
└── js/
    ├── api.js      # API client
    └── app.js      # Application logic
```

Edit these files and reload the browser to see changes (no build step needed).

## 📚 Next Steps

- Create a cooperative: Click "Create Cooperative" in the UI
- Set up governance: Create a domain for democratic decisions
- Make transactions: Use the Ledger tab to send/receive credits
- Explore the API: Check `/v1/health/detailed` for system status

## 🆘 Need Help?

- Check server logs in the terminal where `icnd` is running
- Browser console (F12) shows JavaScript errors
- API errors appear as toast notifications in the UI
- See `crates/icn-gateway/static/README.md` for UI details

---

**Ready to start?** Run: `./start-gateway.sh`
