# ICN Beta Testing Guide

**Status**: Internal Testing Phase
**Environment**: K3s Staging Cluster
**Updated**: 2025-12-12

---

## Quick Access

| Service | URL | Notes |
|---------|-----|-------|
| **Web UI** | http://10.8.10.40:30030 | Timebank pilot interface |
| **Gateway API** | http://10.8.10.40:30080 | REST + WebSocket API |
| **Grafana** | http://10.8.10.40:30300 | Monitoring dashboard |
| **Prometheus** | http://10.8.10.40:30090 | Metrics (internal) |

---

## Getting Started

### 1. Create Your Identity

First, generate your ICN identity (DID) using icnctl:

```bash
# From repo root, navigate to Rust workspace
cd icn
cargo build --release

# Initialize identity
./target/release/icnctl id init

# Show your DID
./target/release/icnctl id show
# Example output: did:icn:zJ6mmCu26hgYegsZJLVQvSF7Ms56UKfo5t7cwUFbpPRiC
```

### 2. Get an Auth Token

**Option A: Use the helper script (Recommended)**

```bash
# From repo root
ICN_PASSPHRASE="your-passphrase" ./scripts/generate-test-token.sh
```

This will output your DID and token ready to paste into the Web UI.

**Option B: Use icnctl directly**

```bash
# From icn/ workspace directory
cd icn

# Get token (handles challenge-response automatically)
ICN_PASSPHRASE="your-passphrase" ./target/release/icnctl auth token \
  --gateway http://YOUR_GATEWAY_URL:30080 \
  --coop-id test-coop
```

### 3. Access the Web UI

1. Open the Pilot UI in your browser (deployment-specific URL)
2. Enter:
   - **Gateway URL**: Your gateway URL
   - **Cooperative ID**: `test-coop`
   - **Your DID**: (from step 1)
   - **Token**: (from step 2)
3. Click "Connect"

---

## Test Scenarios

### Scenario 1: Basic Payment

**Goal**: Send and receive mutual credit hours

1. Log into Web UI with your identity
2. Navigate to "Payments" tab
3. Enter recipient DID and amount (e.g., 5 hours)
4. Add a memo (e.g., "Test payment")
5. Click "Send Payment"
6. Verify balance updated
7. Check transaction in history

**Success Criteria**:
- [ ] Payment shows in sender's history
- [ ] Balance decreased by payment amount
- [ ] No errors displayed

### Scenario 2: Governance Voting

**Goal**: Create and vote on proposals

1. Navigate to "Governance" tab
2. View existing proposals
3. Cast vote on an open proposal
4. Verify vote was recorded

**Success Criteria**:
- [ ] Can view all proposals
- [ ] Vote submission succeeds
- [ ] Vote count updates

### Scenario 3: Real-time Updates

**Goal**: Verify WebSocket events work

1. Open Web UI in two browser windows
2. In Window 1: Send payment to yourself (or another test user)
3. In Window 2: Observe real-time balance update

**Success Criteria**:
- [ ] Balance updates without page refresh
- [ ] No WebSocket connection errors

### Scenario 4: Offline Behavior

**Goal**: Test offline resilience

1. Log into Web UI
2. Disconnect from network (airplane mode or disable interface)
3. Try to send a payment
4. Reconnect to network
5. Verify queued operation completes

**Success Criteria**:
- [ ] Offline indicator appears
- [ ] Operation queued with message
- [ ] Auto-retries on reconnection

---

## Monitoring & Debugging

### Check System Health

```bash
# Gateway health
curl -s http://10.8.10.40:30080/v1/health | jq .

# Node status
ssh ubuntu@10.8.10.40 "sudo kubectl -n icn get pods"

# Node logs
ssh ubuntu@10.8.10.40 "sudo kubectl -n icn logs -f deployment/icn-daemon --tail=50"

# Pilot UI logs
ssh ubuntu@10.8.10.40 "sudo kubectl -n icn logs -f deployment/pilot-ui"
```

### Grafana Dashboard

Open http://10.8.10.40:30300 for metrics:
- **ICN Node Dashboard**: Network, gossip, ledger metrics
- **Byzantine Detection**: Quarantine status, anomalies
- **Performance**: Latency, throughput, errors

---

## Reporting Issues

### Bug Report Template

When you find an issue, please record:

1. **What you were trying to do**
2. **What happened instead**
3. **Steps to reproduce**
4. **Screenshots/logs if available**
5. **Browser/environment info**

### Where to Report

- GitHub Issues: Create issue with `[BETA]` tag
- Shared doc: Add to the Beta Testing Log

---

## Known Limitations

- **Single node**: Currently running 1 ICN node (not full mesh)
- **Test coop**: Using `test-coop` for all testing
- **Internal only**: Not accessible outside local network

---

## Feedback Collection

After each testing session, please note:

1. **Usability**: Was the UI intuitive? Any confusion?
2. **Performance**: Did things feel fast? Any delays?
3. **Features**: What worked well? What's missing?
4. **Bugs**: Any errors or unexpected behavior?

---

## Contact

- **Lead**: Matt Faherty
- **Slack/Discord**: #icn-beta-testing (coming soon)
- **Email**: dev@icn.coop

---

**Thank you for testing!** Your feedback is essential for making ICN production-ready.
