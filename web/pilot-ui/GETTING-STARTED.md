# Getting Started - ICN Pilot UI

**Quick 5-minute guide** to get the ICN Pilot UI running on your local machine for testing.

---

## For the Impatient

```bash
# 1. Navigate to project root
cd /path/to/icn

# 2. Start everything with Docker
cd deploy
./quickstart.sh "My Timebank"

# 3. Open browser
# Visit: http://localhost:3000
```

That's it! The quickstart script handles everything automatically.

---

## Manual Setup (If You Prefer Control)

### Step 1: Start ICN Daemon

**Option A: With Docker (Easiest)**

```bash
cd /path/to/icn/deploy
docker compose up -d icnd
```

**Option B: From Source**

```bash
cd /path/to/icn/icn

# Build
cargo build --release

# Start daemon
export ICN_GATEWAY_JWT_SECRET="your-strong-secret-here"
./target/release/icnd --gateway-enable --gateway-bind 127.0.0.1:8080
```

### Step 2: Create Identity

```bash
# If using Docker
docker compose exec icnd icnctl id init

# If running locally
./target/release/icnctl id init
```

**Important**: Use a strong passphrase. You'll need this every time you restart the daemon.

Your DID will be displayed. Save it somewhere:
```
Created new identity: did:icn:abc123xyz...
```

### Step 3: Create a Cooperative

```bash
# Replace with your DID from step 2
DID="did:icn:YOUR_DID_HERE"

# Create cooperative
icnctl coops create --id "my-coop" --name "My Timebank"

# Add yourself as a member
icnctl coops member add --coop "my-coop" --did "$DID" --role admin
```

### Step 4: Get Authentication Token

```bash
# Get token for your DID
TOKEN=$(icnctl auth login \
    --gateway http://localhost:8080 \
    --coop "my-coop")

echo "Your token: $TOKEN"
```

**Save this token!** You'll need to paste it into the web UI.

Tokens expire after 24 hours. When it expires, run this command again to get a new one.

### Step 5: Start Web UI

**Choose your preferred method:**

**A. Simple Python Server** (Good for testing)

```bash
cd /path/to/icn/web/pilot-ui
python3 -m http.server 3000
```

**B. Node.js Serve** (Slightly better)

```bash
cd /path/to/icn/web/pilot-ui
npx serve -s . -l 3000
```

**C. Docker with nginx** (Production-like)

```bash
cd /path/to/icn/web/pilot-ui
./deploy-ui.sh 3000
# Select option 3 (Docker)
```

### Step 6: Open in Browser

1. Navigate to: **http://localhost:3000**
2. Enter connection details:
   - **Gateway URL**: `http://localhost:8080`
   - **Cooperative ID**: `my-coop`
   - **Your DID**: `did:icn:abc123...` (from step 2)
   - **JWT Token**: `eyJ0eXAi...` (from step 4)
3. Click **"Connect"**

You should see the dashboard!

---

## What's Next?

### Add Demo Data

To populate your timebank with sample members and transactions for testing:

```bash
cd /path/to/icn/web/pilot-ui

# Seed demo data
./seed-demo-data.sh http://localhost:8080 my-coop YOUR_TOKEN_HERE
```

This creates:
- 5 sample members (Alice, Bob, Carol, Dave, Eve)
- 10 sample transactions
- 3 sample governance proposals

### Add Real Members

To add real cooperative members:

```bash
# Member needs to create their identity first
icnctl id init

# Admin adds them to the cooperative
icnctl coops member add \
    --coop "my-coop" \
    --did "did:icn:MEMBER_DID" \
    --role member
```

Each member then gets their own token:

```bash
icnctl auth login --gateway http://localhost:8080 --coop "my-coop"
```

### Explore Features

Now that you're connected, try these features:

**Keyboard Shortcuts**:
- `Ctrl+1` → Dashboard
- `Ctrl+2` → Log Hours
- `Ctrl+3` → History
- `Ctrl+4` → Members
- `Ctrl+5` → Governance

**Log Your First Hours**:
1. Press `Ctrl+2` or click "Log Hours"
2. Select a member
3. Enter hours and description
4. Click "Log Hours"

**View Transactions**:
- Press `Ctrl+3` or click "History"
- Use the dropdown to filter by time period
- Click "Export CSV" for reports

**Vote on Proposals**:
- Press `Ctrl+5` or click "Governance"
- Open proposals show at the top
- Click "Vote" to cast For/Against/Abstain

---

## Common Issues

### "Cannot connect to gateway"

**Cause**: ICN daemon isn't running or gateway is disabled

**Fix**:
```bash
# Check if daemon is running
curl http://localhost:8080/v1/health

# If nothing, start the daemon
icnd --gateway-enable --gateway-bind 127.0.0.1:8080
```

### "Your session has expired"

**Cause**: JWT token expired (24-hour lifetime)

**Fix**:
```bash
# Get a new token
icnctl auth login --gateway http://localhost:8080 --coop my-coop

# Copy the new token and paste into UI
```

### "You don't have permission to do that"

**Cause**: Your DID is not a member of the cooperative, or you lack required role

**Fix**:
```bash
# Check your membership
icnctl coops show --id my-coop

# Add yourself if missing
icnctl coops member add --coop my-coop --did YOUR_DID --role admin
```

### Web UI shows blank page

**Cause**: Browser cache or CORS issue

**Fix**:
1. Hard refresh: `Ctrl+Shift+R` (Windows/Linux) or `Cmd+Shift+R` (Mac)
2. Open browser console (F12) and check for errors
3. Try incognito/private browsing mode

### Port 3000 already in use

**Fix**:
```bash
# Use a different port
python3 -m http.server 3001

# Or find what's using port 3000
lsof -i :3000

# Kill the process
kill -9 PID
```

---

## Documentation

**User Guides**:
- [Quick Start Guide](QUICK-START.md) - For new members
- [Treasurer's Guide](TREASURER-GUIDE.md) - For financial managers
- [Admin Guide](ADMIN-GUIDE.md) - For system administrators
- [FAQ](FAQ.md) - Common questions

**Deployment**:
- [Production Deployment](PRODUCTION-DEPLOY.md) - TLS, security, monitoring
- [Deployment Checklist](DEPLOYMENT-CHECKLIST.md) - Step-by-step production rollout
- [Summary](SUMMARY.md) - Complete feature overview

**Technical**:
- [README](README.md) - Project overview
- [Phase 1 Improvements](IMPROVEMENTS.md) - Authentication & UX
- [Phase 2 Improvements](PHASE2-IMPROVEMENTS.md) - Mobile & Polish
- [Phase 3 Improvements](PHASE3-IMPROVEMENTS.md) - Advanced Features

---

## Quick Reference Card

**Essential Commands**:
```bash
# Start daemon
icnd --gateway-enable --gateway-bind 127.0.0.1:8080

# Get token
icnctl auth login --gateway http://localhost:8080 --coop COOP_ID

# Serve UI
python3 -m http.server 3000

# Check health
curl http://localhost:8080/v1/health
```

**Essential URLs**:
- Web UI: http://localhost:3000
- Gateway: http://localhost:8080
- Health: http://localhost:8080/v1/health

**Keyboard Shortcuts**:
- `Ctrl+1` → Dashboard
- `Ctrl+2` → Log Hours
- `Ctrl+3` → History
- `Ctrl+4` → Members
- `Ctrl+5` → Governance

---

## Getting Help

**Documentation**:
1. Check the [FAQ](FAQ.md) first
2. Read the relevant guide ([Quick Start](QUICK-START.md), [Admin](ADMIN-GUIDE.md), or [Treasurer](TREASURER-GUIDE.md))
3. Review [Troubleshooting](#common-issues) above

**Still stuck?**
- Open an issue: https://github.com/InterCooperative-Network/icn/issues
- Ask in discussions: https://github.com/InterCooperative-Network/icn/discussions

**Include in your issue**:
- Operating system and version
- How you started the daemon (Docker or binary)
- Error messages from browser console (F12)
- Daemon logs: `journalctl -u icnd` or `docker compose logs icnd`

---

## Next Steps After Testing

Ready to deploy for real cooperative use?

1. **Read the [Admin Guide](ADMIN-GUIDE.md)** - Complete system administration
2. **Review [Production Deployment](PRODUCTION-DEPLOY.md)** - TLS, security, monitoring
3. **Use the [Deployment Checklist](DEPLOYMENT-CHECKLIST.md)** - Step-by-step rollout
4. **Share guides with users**:
   - New members: [Quick Start Guide](QUICK-START.md)
   - Treasurers: [Treasurer's Guide](TREASURER-GUIDE.md)

---

**Welcome to the cooperative internet!** 🌱💚
