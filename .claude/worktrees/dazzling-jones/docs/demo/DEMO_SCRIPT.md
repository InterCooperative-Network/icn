# ICN Demo Script

**Duration**: 20 minutes
**Audience**: All (cooperative members, developers, funders, other coops)
**Last Updated**: 2026-02-13

---

## Before You Begin

### Pre-Demo Checklist (2 minutes)

```bash
# From repository root
cd /home/ubuntu/projects/icn

# 1. Verify everything is ready
./demo/scripts/quick-test.sh

# 2. If issues, reset and rebuild
./demo/scripts/reset-demo.sh
cd icn && cargo build --release
```

### What You'll Show

| Feature | What It Proves |
|---------|----------------|
| Identity | "Each member has cryptographic identity they control" |
| Governance | "Cooperatives make democratic decisions" |
| Ledger | "Value exchange without banks or middlemen" |
| Trust | "Accountability through transparent relationships" |

---

## Part 1: Opening (2 minutes)

### The Pitch (30 seconds)

> "ICN is infrastructure for the cooperative economy. It lets cooperatives manage identity, make democratic decisions, and exchange value - without depending on corporations or banks."

### The Problem (30 seconds)

> "Today, cooperatives use Slack for communication, QuickBooks for accounting, Google Docs for governance. Each tool is a dependency on a corporation that doesn't share their values."

### The Solution (1 minute)

> "ICN replaces these dependencies with cooperative-owned infrastructure. Members control their own identity. Decisions are recorded transparently. Value flows directly between people."

---

## Part 2: Start the Demo (3 minutes)

### Launch Everything

```bash
# From repository root
./demo/scripts/run-tool-library-demo.sh
```

**What happens:**
1. Daemon starts with gateway API
2. Authentication token generated
3. Pilot UI server starts
4. Credentials displayed

### Point Out the Credentials

```
┌──────────────────────────────────────────────────────────────────┐
│                        DEMO CREDENTIALS                          │
├──────────────────────────────────────────────────────────────────┤
│ Gateway URL:    http://localhost:8080                            │
│ UI URL:         http://localhost:3000                            │
│ Cooperative:    rochester-tool-library                           │
│ DID:            did:icn:z...                                     │
│ JWT Token:      eyJ...                                           │
└──────────────────────────────────────────────────────────────────┘
```

### Open the UI

1. Open browser to http://localhost:3000
2. Click "Sign In"
3. Enter:
   - Gateway URL: `http://localhost:8080`
   - Cooperative ID: `rochester-tool-library`
   - DID: (copy from terminal)
   - Token: (copy from terminal)
4. Click "Sign In"

**After login**, the dashboard shows your display name (e.g. "Demo User") instead of a raw DID.

**Say:** "This is the Rochester Tool Library - a real cooperative that lends tools to community members. Notice you see your name, not a cryptographic identifier."

---

## Part 3: Identity (3 minutes)

### Show the Dashboard

**Point out:**
- Your display name (e.g. "Demo User") — not a raw cryptographic ID
- Balance (hours in the timebank)
- Cooperative context

**Say:** "Every member has a decentralized identifier - a DID - but you don't have to look at raw crypto strings. The system resolves names so it feels like any normal app."

### Edit Your Display Name

1. Click the pencil icon next to your name (top-right)
2. Enter a new name (e.g. your real name)
3. Notice it updates everywhere immediately

**Say:** "Members control their own profile. Names are stored locally and resolved on demand - no central directory."

### Show the Members Tab

1. Click "Members" in sidebar
2. Point out the 4 pre-seeded members with real names:
   - Sarah Chen, Marcus Rivera, Priya Patel, James Okafor
3. Note: names appear throughout - transactions, governance, member list

**Say:** "This information is stored locally and shared only with your cooperative. Not with advertisers or data brokers."

### Show Trust (if time)

**Say:** "Trust is explicit. You can see who vouches for whom. This creates accountability without surveillance."

---

## Part 4: Governance (5 minutes)

### Create a Proposal

1. Click "Governance" in sidebar
2. Click "Create Proposal"
3. Fill in:
   - Title: "Extend tool lending period to 2 weeks"
   - Type: Text Proposal
   - Description: "Currently tools are due in 1 week. Proposing we extend to 2 weeks for member convenience."
4. Submit

**Say:** "Any member can propose changes. Notice the 'Proposed by' line shows your name - not a cryptographic hash. This creates a formal record that everyone can see."

### Vote on the Proposal

1. Click on the new proposal
2. Click "Vote"
3. Select "Yes"
4. Confirm

**Say:** "Voting is transparent and recorded. You can see who voted which way - or configure anonymous voting if preferred."

### Show Delegation (if time)

**Say:** "You can also delegate your vote to someone you trust. This lets experts handle technical decisions while you retain ultimate control."

---

## Part 5: Ledger & Economic Flows (4 minutes)

### Log Hours

1. Click "Transactions" in sidebar
2. Click "Log Hours" or "New Transaction"
3. Fill in:
   - Recipient: Select from dropdown — names shown, not DIDs
   - Activity: "Tool maintenance"
   - Hours: 2.0
   - Description: "Cleaned and oiled power tools"
4. Submit

**Say:** "This is mutual credit - value exchange without money. The cooperative tracks contributions and everyone can see the ledger."

### Show Transaction History

1. Scroll through transactions
2. Point out:
   - Sender/recipient shown as **names** (Sarah Chen, Marcus Rivera, etc.)
   - Date/time and activity type
   - Hours credited
   - The pre-seeded transactions between named members

**Say:** "Every transaction is cryptographically signed and linked to the previous one. It's tamper-evident - any change would be visible. And notice - you see people's names, not cryptographic hashes."

### Show Receipts Tab

1. Click "Receipts" in sidebar
2. Point out verification badges

**Say:** "These are economic receipts with deterministic hashes. Any node can verify the entire history independently."

---

## Part 6: Architecture Overview (3 minutes)

### Show the Terminal

Switch to the terminal running the demo.

**Say:** "What you're seeing is a single node. In production, each cooperative runs their own node. They connect peer-to-peer - no central server."

### Draw the Mental Model

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│  Coop A     │◄───►│  Coop B     │◄───►│  Coop C     │
│  (node)     │     │  (node)     │     │  (node)     │
└─────────────┘     └─────────────┘     └─────────────┘
      │                   │                   │
      ▼                   ▼                   ▼
   Members             Members             Members
```

**Say:** "Each cooperative controls their data. They can federate with others for inter-coop transactions. But no one can unilaterally access or shut down another cooperative."

### The Key Insight

**Say:** "This is what makes ICN different from both blockchains and traditional SaaS:

- Unlike blockchain: No global consensus tax. Cooperatives are sovereign.
- Unlike SaaS: No corporate dependency. You own your infrastructure.

It's the cooperative principle - user ownership - applied to the infrastructure itself."

---

## Part 7: Closing & Q&A (Remaining time)

### Summary

> "You've seen identity, governance, and economic exchange - all running on cooperative-owned infrastructure. This is a working pilot. The Rochester Tool Library is one of several cooperatives testing it."

### Common Questions

**"How is this different from blockchain?"**
> "Blockchains force global consensus - every transaction validated by everyone. ICN is local-first. Cooperatives validate their own transactions. They only coordinate with others when needed."

**"What if a node goes down?"**
> "Data syncs across nodes via gossip. If your node goes down, you restore from backup or sync from peers. No single point of failure."

**"How do cooperatives control policy?"**
> "Governance. The proposals and voting you saw aren't just for bylaws - they control technical parameters too. Credit limits, membership rules, federation agreements."

**"Is this ready for production?"**
> "We're in pilot phase with real cooperatives. Core infrastructure is solid - 2,000+ tests passing. We're focused on usability and documentation now."

### Call to Action

For **cooperative members**: "Try the demo. Tell us what's missing for your workflow."

For **developers**: "The code is open. Check out the architecture docs. We welcome contributions."

For **funders**: "This is patient infrastructure. It compounds over decades as more cooperatives adopt it."

For **other cooperatives**: "Talk to us about running a pilot. We're looking for diverse use cases."

---

## Stopping the Demo

```bash
# In the terminal running the demo
Ctrl+C
```

Or to fully reset:

```bash
./demo/scripts/reset-demo.sh
```

---

## Troubleshooting During Demo

### Gateway not responding

```bash
curl http://localhost:8080/v1/health
# If no response, restart the demo
```

### Token expired

The demo script displays fresh credentials. If needed:

```bash
cd icn
ICN_PASSPHRASE=demo123 ./target/release/icnctl \
  -d ../.demo-data/tool-library \
  -e 127.0.0.1:15602 \
  auth token \
  --coop-id rochester-tool-library \
  --scopes "coop:write,coop:read,coop:admin,ledger:read,ledger:write,gov:read,gov:write"
```

### UI shows errors

Check browser console (F12). Most common issues:
- CORS: Gateway should allow localhost:3000
- Auth: Token may have expired (regenerate)
- API: Gateway may have stopped (restart demo)

---

## Demo Variants

### 5-Minute Lightning Demo

Skip to:
1. Start demo (1 min)
2. Login + Dashboard (1 min)
3. Create proposal + vote (2 min)
4. "Questions?" (1 min)

### 45-Minute Deep Dive

Add:
- Multi-node setup (show 2 nodes syncing)
- CLI commands (icnctl walkthrough)
- Architecture diagrams (from docs)
- Code walkthrough (crate structure)

### Hands-On Workshop

Provide:
- Pre-built VM or Docker image
- Step-by-step exercises
- Breakout for Q&A

---

## Resources

- **Full Documentation**: `docs/ARCHITECTURE.md`
- **Quick Start**: `docs/GETTING_STARTED.md`
- **API Reference**: `docs/api/`
- **Demo Infrastructure**: `demo/README.md`
