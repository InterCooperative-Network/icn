# ICN Demo Wiring - Session 1 Status

> **Archived Document Notice (2026-02-12):** This file is retained for historical context and may not reflect current code, APIs, runtime defaults, CI status, or deployment posture.
> Use active documentation under `docs/` as authoritative.

**Date:** 2025-12-18  
**Duration:** ~1.5 hours  
**Focus:** Getting daemon running with gateway

---

## ✅ What Worked

### Identity Creation
- Successfully created test identity in `<demo-data-dir>/data`
- DID: `did:icn:zBFnhJhgvRjgukhQmkq9ddBz5wiEt32ptkQkBDjWx6uPh`
- Passphrase: `demo123`
- Keystore format: v4 (SDIS support)

### Configuration
- Created working config at `<demo-data-dir>/demo.toml`
- JWT secret configured
- Gateway bind address: 127.0.0.1:8080
- Network port: 19777
- RPC port: 15602

### Daemon Startup (Partial)
- Daemon initializes successfully
- All actors spawn correctly:
  - Trust graph ✓
  - Gossip actor ✓
  - Ledger ✓
  - Contract actor ✓
  - Cooperative actor ✓
  - Identity actor ✓
  - Network actor ✓
- QUIC endpoint binds successfully to 0.0.0.0:19777

---

## ❌ Current Blocker

### Session Manager Fails to Start
**Error:** `Failed to start session manager: Address already in use (os error 98)`

**Symptoms:**
1. QUIC endpoint successfully binds: "QUIC endpoint listening on 0.0.0.0:19777"
2. Immediately after, discovery service shuts down
3. Actors stop
4. Runtime exits with "Address already in use" error

**What's Strange:**
- Port 19777 is NOT in use before or after daemon exits (verified with `ss` and `lsof`)
- Tried multiple ports (7778, 8888, 19777) - all show same behavior
- Even with `mdns_enabled = false`, mDNS service still registers

**Hypothesis:**
This might not be a port conflict at all. The error message is misleading. Possible causes:
1. Some other resource (not port) is "already in use"
2. Multiple bind attempts happening internally
3. Actor crash/panic triggering shutdown
4. Race condition during initialization

---

## 🔍 Next Steps to Debug

### Option 1: Check for Existing Daemons
There are several icnd processes running from previous sessions:
```
matt        2242  - icnd --config /config/node3.toml
root        2308  - icnd --config /root/.icn/icn.toml
matt        2330  - icnd --config /config/node1.toml
matt        2347  - icnd --config /config/node2.toml
```

These might be holding resources (trust graph store, ledger store, etc.)

**Action:** Kill all existing daemons and try again

### Option 2: Check Store/Database Locks
The daemon initializes several stores:
- Trust graph: `<demo-data-dir>/data/store/trust`
- Gossip: `<demo-data-dir>/data/store/gossip`
- Ledger: `<demo-data-dir>/data/store/ledger`
- Cooperative: `<demo-data-dir>/data/store/cooperative`

If Sled (the database) has locks, that could cause "address already in use"

**Action:** Check if database files are locked

### Option 3: Increase Log Level
Run with `--log-level debug` or `--log-level trace` to see more detail

**Action:** Try with verbose logging

### Option 4: Review Recent Code Changes
Check if there were recent changes to network/session initialization that might cause this

**Action:** Look at recent commits to icn-net

### Option 5: Test Gateway Separately
Since the daemon gets far enough to initialize the gateway, we might be able to test gateway endpoints separately

**Action:** Try a minimal daemon without network actor

---

## 🎯 Recommended Immediate Action

**Kill all running daemons and retry:**

```bash
# Kill all icnd processes
ps aux | grep icnd | grep -v grep | awk '{print $2}' | xargs kill 2>/dev/null

# Wait for cleanup
sleep 5

# Clean test directory
rm -rf <demo-data-dir>/data/store/*

# Retry daemon start
cd <repo-root>/icn
./target/release/icnd --config <demo-data-dir>/demo.toml \
    --gateway-enable \
    --gateway-bind "127.0.0.1:8080" \
    --gateway-jwt-secret "demo-secret-key-change-in-production" \
    --log-level debug
```

---

## 📊 Progress Assessment

**Overall Progress: 60%**

### What's Working (80%)
- ✅ Build system
- ✅ Identity creation
- ✅ Configuration
- ✅ Actor initialization
- ✅ Most of daemon startup

### What's Blocked (20%)
- ❌ Network actor / session manager
- ❓ Gateway startup (might be working but daemon exits before we can test)

**Time Spent:** 1.5 hours  
**Time to Solution (Estimate):** 30 minutes to 2 hours

**Confidence this is solvable:** HIGH
- Error is environmental, not fundamental
- System initializes correctly up to network layer
- Likely resource conflict with existing daemons

---

## 🔄 Alternative Approaches if This Persists

### Plan B: Use Existing Running Daemon
Use one of the already-running daemons (ports 7777, etc.) for testing

**Pros:**
- Already running
- Might have gateway enabled

**Cons:**
- Unknown configuration
- Might not have test identity

### Plan C: Test Gateway as Standalone
Build minimal binary that starts just the gateway without network layer

**Pros:**
- Can test API endpoints
- Bypass network initialization

**Cons:**
- Requires code changes
- Won't be full integration test

### Plan D: Use Docker
Run daemon in Docker container with isolated resources

**Pros:**
- Clean environment
- No conflicts

**Cons:**
- Takes time to set up
- Might have networking complexity

---

##🎯 Decision Point

**If next attempt fails:**
1. Check existing daemon at port 7777 - can we use it?
2. Test gateway standalone
3. Come back to this after checking gateway API implementation

**Don't spend more than 30 minutes on this blocker**  
Gateway API testing is more critical path

---

## 📝 Notes for Documentation

When this is resolved, add to setup docs:
1. "Kill all existing icnd processes before starting test daemon"
2. "Clean store directories if switching data directories"
3. "Passphrase prompt requires interactive terminal"
4. Consider adding `--passphrase-file` option to icnd

---

## ⏰ Time Box

**Maximum time on this issue:** 30 more minutes  
**Then:** Move to testing gateway with existing daemon or standalone

**Rationale:** We need to verify gateway API works - that's the critical path for demo. Network layer can be debugged separately if needed.
