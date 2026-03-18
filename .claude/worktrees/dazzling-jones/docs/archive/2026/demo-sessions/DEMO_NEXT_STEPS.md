# ICN Demo - Immediate Next Steps

> **Archived Document Notice (2026-02-12):** This file is retained for historical context and may not reflect current code, APIs, runtime defaults, CI status, or deployment posture.
> Use active documentation under `docs/` as authoritative.

**Status:** Foundation is solid, need to connect the pieces  
**Confidence:** 7/10 → 10/10 in 7-9 hours  
**Goal:** Working end-to-end transaction demo by end of week

---

## ✅ What We Confirmed Works

### Backend
- ✅ Builds successfully (47s)
- ✅ 99.6% tests passing (1130+/1134)
- ✅ Daemon starts and runs
- ✅ Gateway API exists with all needed endpoints:
  - `/v1/members/{coop_id}/{did}` - Member profiles
  - `/ledger/{coop_id}/balance/{did}` - Get balance
  - `/ledger/{coop_id}/payment` - Create payment
  - `/ledger/{coop_id}/history` - Transaction history
- ✅ Multi-node configs ready (alpha/beta)

### Frontend
- ✅ Complete pilot UI (4500+ lines)
- ✅ Mobile responsive
- ✅ Offline mode with service worker
- ✅ Authentication flow
- ✅ Transaction forms
- ✅ Member list
- ✅ Governance UI

**Bottom Line:** We have all the pieces, just need to wire them together.

---

## 🎯 Today's Mission (Day 1 - 4 hours)

### Goal: One Working Transaction

**Success Criteria:**
1. Daemon running with gateway enabled
2. UI connects to gateway
3. Can log in
4. Can create a transaction
5. Balance updates correctly

---

## Step-by-Step Plan

### Phase 1: Setup Working Environment (30 minutes)

```bash
# 1. Create dedicated demo directory
mkdir -p ~/icn-demo-test
cd ~/icn-demo-test

# 2. Initialize identity
<repo-root>/icn/target/release/icnctl id init

# 3. Copy and customize config
cp <repo-root>/config/icn-alpha.toml ./demo-node.toml

# Edit demo-node.toml:
#   - data_dir = "~/icn-demo-test/data"
#   - Enable gateway (if not already)

# 4. Start daemon with gateway
cd <repo-root>/icn
./target/release/icnd --config ~/icn-demo-test/demo-node.toml --gateway-enable --gateway-bind "127.0.0.1:8080"

# 5. Verify it's running
curl http://localhost:8080/api/health  # or whatever health endpoint exists
curl http://localhost:5601/health      # RPC health
curl http://localhost:9100/metrics | head -20  # Metrics
```

**Expected Output:**
- Daemon starts without errors
- Gateway listening on 8080
- Health checks return 200 OK

---

### Phase 2: Initialize Cooperative (45 minutes)

The UI expects a cooperative to exist. We need to create one.

```bash
# Check what the init-coop command does
./target/release/icnctl init-coop --help

# Initialize a test cooperative
./target/release/icnctl init-coop

# Follow the wizard to create:
# - Coop name: "Rochester Tool Library"
# - Type: Timebank
# - Credit limit: 10.0 hours
# - Initial members: Just your DID for now

# Verify cooperative exists
./target/release/icnctl ledger --help
# Look for commands to query state

# Alternative: Use RPC directly
# Check icn-rpc crate for proto definitions
```

**Critical Questions to Answer:**
1. Does `init-coop` exist and work?
2. How do we add members to a coop?
3. How do we get a JWT token for gateway auth?

**If init-coop doesn't work:**
- Check `icn-gateway/src/api/coops.rs` for endpoints
- May need to call API directly to create coop
- Document what needs to be built

---

### Phase 3: Test Gateway Auth (30 minutes)

The UI needs JWT tokens to call the gateway.

```bash
# Check auth command
./target/release/icnctl auth --help

# Try to get a token
./target/release/icnctl auth login --coop-id "rochester-tool-library"

# Test token
TOKEN="<token from above>"
curl -H "Authorization: Bearer $TOKEN" http://localhost:8080/v1/members/rochester-tool-library/<your-did>

# If auth command doesn't exist:
# Check icn-gateway/src/api/auth.rs for how to get tokens
# May need to call API directly: POST /auth with DID + signature
```

**Expected:**
- Can get JWT token
- Token works for authenticated requests
- Token includes correct scopes (ledger:read, ledger:write)

---

### Phase 4: Test UI Connection (45 minutes)

```bash
# 1. Start simple HTTP server for UI
cd <repo-root>/web/pilot-ui
python3 -m http.server 3000

# Open browser: http://localhost:3000

# 2. Try logging in
# Gateway URL: http://localhost:8080
# Cooperative ID: rochester-tool-library
# DID: <your DID from icnctl id show>
# Token: <token from icnctl auth login>

# 3. Watch browser console for errors
# Open DevTools (F12) → Console

# 4. Watch network requests
# DevTools → Network → XHR

# 5. Check what API calls are made
# Look for:
#   - GET /v1/members/<coop-id>  (member list)
#   - GET /ledger/<coop-id>/balance/<did>
#   - etc.
```

**Common Issues to Watch For:**
- CORS errors (gateway needs to allow localhost:3000)
- 404 errors (API paths don't match what UI expects)
- 401 errors (token invalid or missing scopes)
- 403 errors (authorization issues)

**Fix CORS if needed:**
```rust
// In icn-gateway/src/server.rs
// Look for CORS configuration
// Add localhost:3000 to allowed origins
```

---

### Phase 5: Debug and Fix (1 hour)

Based on what we find in Phase 4, fix issues:

#### If API paths don't match:
```bash
# Compare UI expectations vs gateway routes
cd <repo-root>/web/pilot-ui
grep -n "fetch.*api/" app.js | head -20

# Compare with gateway routes
cd <repo-root>/icn/crates/icn-gateway/src
grep -rn "\.service\|\.route" . | grep -E "members|ledger|balance"

# Document discrepancies
# Either:
#   A) Fix UI to match gateway
#   B) Fix gateway to match UI expectations
```

#### If no cooperative exists:
```bash
# May need to create cooperative via code
# Check if there's a way to bootstrap a test coop
# Or build a quick script to call gateway APIs directly
```

#### If members don't exist:
```bash
# Create additional test identities
mkdir -p /tmp/test-member-{1,2,3}
for i in 1 2 3; do
    cd /tmp/test-member-$i
    icnctl id init  # Create identity
    # Extract DID and add to cooperative
done
```

---

### Phase 6: Test Transaction (30 minutes)

Once logged in successfully:

```bash
# In the UI:
# 1. Go to "Log Hours" tab
# 2. Fill in form:
#    - Hours: 2.5
#    - Recipient: <another DID - might need to create>
#    - Memo: "Woodworking instruction - table saw safety"
# 3. Submit

# Watch what happens:
# - Check browser console for errors
# - Check network tab for POST request
# - Check daemon logs for activity

# Verify:
# 1. Transaction appears in history
# 2. Balance updates correctly
# 3. No errors in console

# From CLI:
./target/release/icnctl ledger history --coop-id rochester-tool-library
./target/release/icnctl ledger balance --coop-id rochester-tool-library --did <your-did>
```

---

## Quick Reference

### Useful Commands

```bash
# Identity
icnctl id show                          # Show your DID
icnctl id init                          # Create new identity

# Daemon
icnd --config <path> --gateway-enable   # Start with gateway
icnd --help                             # See all options

# Status checks
curl http://localhost:8080/health       # Gateway health
curl http://localhost:5601/health       # RPC health  
curl http://localhost:9100/metrics      # Prometheus metrics

# Logs
# Daemon logs go to stdout
# Watch with: tail -f or in same terminal

# Find processes
ps aux | grep icnd
lsof -i :8080    # What's using port 8080
lsof -i :5601    # What's using port 5601
```

### Key Files

**Backend:**
- `icn/bins/icnd/src/main.rs` - Daemon entry point
- `icn/crates/icn-gateway/src/server.rs` - Gateway routes
- `icn/crates/icn-gateway/src/api/` - API endpoints
- `config/icn-alpha.toml` - Example config

**Frontend:**
- `web/pilot-ui/index.html` - Main HTML
- `web/pilot-ui/app.js` - Application logic
- `web/pilot-ui/style.css` - Styles

**Config:**
- `config/icn-alpha.toml` - Node 1 config
- `config/icn-beta.toml` - Node 2 config

---

## Expected Timeline

### Today (4 hours)
- [x] Audit complete
- [ ] Environment setup (30 min)
- [ ] Cooperative initialization (45 min)
- [ ] Gateway auth working (30 min)
- [ ] UI connects (45 min)
- [ ] Debug and fix (1 hour)
- [ ] One transaction works (30 min)

**End of Day Goal:** Can create one transaction via UI

### Tomorrow (3 hours)
- [ ] Add 2-3 more test members
- [ ] Test transactions between members
- [ ] Create sample data generator script
- [ ] Document exact setup workflow

**End of Day Goal:** Multi-member demo working

### Day 3 (2 hours)
- [ ] Polish UI feedback (loading, errors)
- [ ] Test on mobile browser
- [ ] Create demo reset script
- [ ] Write demo narration

**End of Day Goal:** Demo is smooth and polished

### Day 4 (2 hours)
- [ ] Practice demo 10 times
- [ ] Time the demo (target: 18-22 minutes)
- [ ] Fix any remaining rough edges
- [ ] Create troubleshooting guide

**End of Day Goal:** Can demo confidently

---

## Blocking Questions (Answer ASAP)

### 1. How do we create a cooperative?
- [ ] `icnctl init-coop` exists and works?
- [ ] Or via gateway API?
- [ ] Or needs to be built?

### 2. How do we add members?
- [ ] Member invitation flow?
- [ ] Admin adds members directly?
- [ ] Self-service signup?

### 3. How does gateway auth work?
- [ ] `icnctl auth` command?
- [ ] Sign DID challenge?
- [ ] Something else?

### 4. Do API paths match UI expectations?
- [ ] Need to audit both sides
- [ ] May need to update UI or gateway

**Action:** Answer these in Phase 2-3 of today's work

---

## Risk Mitigation

### If cooperative init is complex:
**Backup Plan:** Manually create minimal coop data structure
- Create JSON files with coop + member data
- Import directly to data store
- Bypass initialization flow for demo

### If gateway auth is complex:
**Backup Plan:** Disable auth for demo
- Add flag to gateway: `--no-auth` mode
- Demo runs without JWT tokens
- Document that production needs auth

### If UI doesn't connect:
**Backup Plan:** Use curl to demo backend
- Show transactions via CLI
- Use `jq` to format JSON nicely
- Terminal-based demo as backup

### If nothing works by Wednesday:
**Backup Plan:** Record demo video
- Get one transaction working
- Record screen capture
- Use video + slides for presentation

---

## Success Metrics

### Minimum (by EOD Friday)
- [ ] Daemon starts reliably
- [ ] Can create one transaction
- [ ] Balance updates correctly
- [ ] Have recorded backup video

### Target (by EOD Friday)
- [ ] UI works end-to-end
- [ ] 3+ members in demo
- [ ] Can demo 5-10 transactions
- [ ] Mobile experience works
- [ ] Demo runs 20/20 times

### Stretch (by EOD Friday)
- [ ] Two-node federation working
- [ ] Governance voting demo
- [ ] Beautiful sample data
- [ ] Professional screenshots

---

## Decision Points

### After Phase 4 (UI Connection Test)

**If everything works:**
→ Continue to transaction test  
→ Start building sample data tomorrow

**If missing features found:**
→ Assess: Can fix in 2 hours? Or need backup plan?  
→ Document what needs to be built  
→ Prioritize: Must-have vs nice-to-have

**If fundamental issues:**
→ Escalate: Does demo need to be delayed?  
→ Or: Switch to CLI-based demo?  
→ Or: Recorded video only?

### Make This Decision Today
By end of Phase 4, we'll know if the full demo is feasible or if we need plan B.

---

## Communication Plan

### End of Today
**Send update:**
- What's working
- What's blocked
- Confidence level
- Need any help?

### End of Tomorrow
**Send update:**
- Demo readiness %
- Remaining tasks
- Risk assessment
- Backup plan status

### Thursday
**Send demo invitation:**
- Schedule demo time
- Location (in-person or remote)
- What to expect
- How to join

---

## Key Insight

**We're 80% there** - the foundation is solid:
- ✅ Core backend works
- ✅ Gateway API exists
- ✅ UI is complete
- ✅ Tests pass

**The 20% is integration:**
- Wire UI to gateway
- Create sample data
- Polish and practice

This is **achievable in 3-4 focused days**.

The biggest risk is discovering a missing feature (like cooperative initialization). But even then, we have backup plans.

**Let's start with Phase 1** and see what we find. 🚀
