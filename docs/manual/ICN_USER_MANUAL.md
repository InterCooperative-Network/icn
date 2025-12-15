# ICN User Manual

**Intercooperative Network**
*Infrastructure for the Cooperative Economy*

---

**Manual Version:** 1.1.1
**Target Software:** ICN v0.2.x
**Last Updated:** December 2025
**Status:** Pilot Ready

---

## Role Labels

| Icon | Role | Description |
|------|------|-------------|
| **Member** | Cooperative member | Day-to-day participation |
| **Admin** | Cooperative administrator | Organizational management |
| **Operator** | Node operator | Infrastructure management |

---

## Quick Start: 15 Minutes

Get running with ICN in under 15 minutes:

### 1. Install (2 min)

```bash
# Docker (recommended)
git clone https://github.com/InterCooperative-Network/icn.git
cd icn
cp .env.example .env
docker compose up -d

# Or from source
cd icn/icn && cargo build --release
sudo cp target/release/{icnd,icnctl} /usr/local/bin/
```

### 2. Create Identity (1 min)

```bash
icnctl id init
# Enter passphrase when prompted
icnctl id show
```

### 3. Configure & Start (2 min)

```bash
# Edit config
nano ~/.icn/icn.toml

# Add bootstrap peer
# bootstrap_peers = ["icn://did:icn:BOOTSTRAP_DID@IP:7777"]

# Start daemon
icnd
```

### 4. Join Network (5 min)

```bash
# Check network status
icnctl network status

# Request trust from existing member
# They run: icnctl trust add did:icn:YOUR_DID 0.5
```

### 5. First Transaction (2 min)

```bash
# Check balance
icnctl ledger balance

# Send credits
icnctl ledger pay did:icn:RECIPIENT_DID 2.0 hours "Helped with garden"

# View history
icnctl ledger history
```

You're now participating in the cooperative economy.

---

## Conceptual Model

```
┌─────────────────────────────────────────────────────────────┐
│                        ICN Node                             │
├─────────────────────────────────────────────────────────────┤
│  User Interfaces                                            │
│  ┌──────────┐  ┌──────────────┐  ┌─────────────────────┐   │
│  │  icnctl  │  │  Gateway API │  │  Web UI (Pilot)     │   │
│  │  (CLI)   │  │  (REST/WS)   │  │  (Browser)          │   │
│  └────┬─────┘  └──────┬───────┘  └──────────┬──────────┘   │
│       │               │                      │              │
├───────┴───────────────┴──────────────────────┴──────────────┤
│  icnd (Daemon)                                              │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐   │
│  │ Identity │  │  Trust   │  │  Ledger  │  │Governance│   │
│  │  Layer   │  │  Graph   │  │  (Mutual │  │  Engine  │   │
│  │          │  │          │  │  Credit) │  │          │   │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘   │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐  ┌──────────┐   │
│  │  Gossip  │  │ Contract │  │  Store   │  │ Compute  │   │
│  │  Sync    │  │   (CCL)  │  │  (Sled)  │  │  Layer   │   │
│  └──────────┘  └──────────┘  └──────────┘  └──────────┘   │
├─────────────────────────────────────────────────────────────┤
│  Network: QUIC/TLS + mDNS Discovery                         │
└─────────────────────────────────────────────────────────────┘
```

---

# Part I: Introduction

## Chapter 1: What is ICN?

### Vision and Purpose

ICN (Intercooperative Network) is a **peer-to-peer coordination substrate** for cooperatives and community organizations. It provides the infrastructure for:

- **Decentralized Identity**: Self-sovereign identifiers (DIDs) with Ed25519 cryptography
- **Trust Graph**: Web-of-participation based reputation
- **Mutual Credit**: Community currency without external backing
- **Gossip Protocol**: Resilient P2P communication
- **Cooperative Contracts**: Programmable agreements (CCL)
- **Governance Primitives**: Democratic decision-making tools

> *"The goal isn't finance. The goal is cooperation you can audit."*

### How ICN Differs from Blockchain

| Aspect | ICN | Blockchain |
|--------|-----|------------|
| **Consensus** | Local autonomy, eventual consistency | Global consensus |
| **Trust Model** | Assumes relationship | Assumes hostility |
| **Tokens** | Mutual credit (internal accounting) | Tradeable assets |
| **Speculation** | Impossible by design | Common |
| **Energy** | Minimal (no mining) | Often significant |
| **Governance** | Democratic, cooperative | Token-weighted |

ICN is infrastructure for people who already trust each other to coordinate better—not for strangers to transact without trust.

### Who ICN is For

**Primary audiences:**
- **Timebanks**: Hour-based mutual credit communities
- **Worker cooperatives**: Democratic workplaces
- **Food cooperatives**: Member-owned grocery/buying clubs
- **Housing cooperatives**: Resident-owned housing
- **Mutual aid networks**: Community support systems
- **Credit unions**: Member-owned financial cooperatives

**ICN is not for:**
- Speculative trading
- Anonymous transactions
- Replacing government currencies
- "Disrupting" existing cooperatives

---

## Chapter 2: Core Concepts

### Decentralized Identity (DIDs)

Every participant has a **DID** (Decentralized Identifier):

```
did:icn:5Xk8Y2rQm7...
```

- **Self-sovereign**: You control your keys, not a central authority
- **Cryptographic**: Based on Ed25519 public key
- **Portable**: Works across all cooperatives
- **Recoverable**: Multi-device and social recovery options

### Trust Graph

Trust is the foundation of ICN. The trust graph tracks:

- **Direct trust**: "I trust Alice with score 0.8"
- **Transitive trust**: Trust flows through connections
- **Trust decay**: Indirect trust weakens with distance

**Trust Classes:**

| Class | Score Range | Access Level |
|-------|-------------|--------------|
| Isolated | < 0.1 | Very limited |
| Known | 0.1 - 0.4 | Basic |
| Partner | 0.4 - 0.7 | Standard |
| Federated | 0.7+ | Full |

**Trust decay**: By default, trust decays by factor `d = 0.7` per hop. A 1-hop path contributes `d`, 2-hop contributes `d²`, etc. The default decay factor reflects empirical social distance assumptions and is configurable per cooperative.

### Mutual Credit

ICN uses **mutual credit** instead of tokens:

```
Alice helps Bob for 2 hours:
  Alice: +2 hours
  Bob:   -2 hours
  Total:  0 hours (always zero-sum)
```

**Key properties:**
- Credits are internal accounting, not tradeable assets
- Balances can be negative (you owe) or positive (you're owed)
- No external backing required
- Value comes from reciprocity, not scarcity

**Currency scopes:**
- `hours` - Local default
- `coop:food-portland:hours` - Coop-scoped
- `federation:pacific-nw:hours` - Federation-scoped

### Fuel System

**Fuel** is permission to perform network operations:

- **Regenerative**: Refills over time (24-hour full regen)
- **Non-transferable**: Cannot be bought or sold
- **Contribution-based**: More contribution = higher capacity

**Fuel costs:**
| Operation | Cost |
|-----------|------|
| Publish message | 1 |
| Ledger transaction | 10 |
| Create proposal | 50 |
| Submit compute job | Variable |

**Guaranteed minimum**: 50 fuel units ensure everyone can participate in governance regardless of contribution level.

### Governance

ICN provides democratic decision-making primitives:

- **Domains**: Governance contexts (e.g., `coop:food-collective`)
- **Proposals**: Motions to be voted on
- **Voting**: For/Against/Abstain with configurable thresholds
- **Profiles**: Decision rules (consensus, majority, consent)

---

## Chapter 3: Glossary

See [docs/glossary.md](../glossary.md) for the complete, authoritative terminology reference.

**Quick reference:**

| Term | Definition |
|------|------------|
| **DID** | Decentralized Identifier (`did:icn:<pubkey>`) |
| **Trust Score** | Computed reputation (0.0 to 1.0) |
| **Credit** | Internal accounting unit (not a token) |
| **Fuel** | Rate-limiting permission (regenerates) |
| **Cooperative** | Economic organization with ledger |
| **Community** | Civic organization (mutual aid, advocacy) |
| **Federation** | Group of cooperatives with shared agreements |
| **Gateway API** | REST/WebSocket interface for applications |

---

# Part II: Getting Started

## Chapter 4: Installation

**Operator**

### System Requirements

| Component | Minimum | Recommended |
|-----------|---------|-------------|
| CPU | 2 cores | 4+ cores |
| RAM | 2 GB | 4+ GB |
| Storage | 10 GB SSD | 50+ GB SSD |
| Network | 10 Mbps | 100+ Mbps |
| OS | Linux (kernel 5.4+) | Ubuntu 22.04 LTS |

### Installing from Source

```bash
# Prerequisites
sudo apt install build-essential pkg-config libssl-dev

# Clone and build
git clone https://github.com/InterCooperative-Network/icn.git
cd icn/icn
cargo build --release

# Install binaries
sudo cp target/release/icnd /usr/local/bin/
sudo cp target/release/icnctl /usr/local/bin/

# Verify
icnctl --version
icnd --version
```

### Docker Installation

```bash
# Clone repository
git clone https://github.com/InterCooperative-Network/icn.git
cd icn

# Configure environment
cp .env.example .env
nano .env  # Edit as needed

# Build and run
docker compose up -d

# Verify
docker compose ps
docker compose logs icnd
```

### Verifying Installation

```bash
# Check versions
icnctl --version
icnd --version

# Validate configuration
icnd --validate --config ~/.icn/icn.toml

# Test daemon startup
icnd --dry-run
```

---

## Chapter 5: Creating Your Identity

**Member**

### Generating Keys

```bash
# Initialize identity
icnctl id init

# You'll be prompted for:
# 1. Passphrase (encrypts your private key)
# 2. Passphrase confirmation

# View your identity
icnctl id show
```

Output:
```
DID: did:icn:5Xk8Y2rQm7nPqKjL...
Public Key: 5Xk8Y2rQm7nPqKjL...
Keystore: ~/.icn/keystore.age
Created: 2025-12-14T10:30:00Z
```

### Understanding Your DID

Your DID is derived from your public key:
- Format: `did:icn:<base58-encoded-pubkey>`
- Permanent: Changing keys requires rotation (see below)
- Portable: Works across all ICN networks

### Backing Up Your Identity

**Critical**: Your keystore is your identity. Back it up immediately.

```bash
# Create encrypted backup
icnctl backup ~/icn-backup-$(date +%Y%m%d).tar.gz.age

# Store backups:
# - Off-site (cloud storage, different physical location)
# - Multiple copies (at least 2)
# - Test restoration periodically
```

### Multi-Device Setup

Use your identity on multiple devices:

```bash
# On primary device: add new device
icnctl device add --name "laptop" --capabilities sign

# On new device: import device key
icnctl device import --from ~/device-key.age

# List devices
icnctl device list
```

---

## Chapter 6: Joining a Cooperative

**Member**

### Finding a Cooperative

Cooperatives share bootstrap information through:
- Invitation links from existing members
- Community registries (if available)
- Direct peer addresses

### The Trust Onboarding Process

1. **Connect to bootstrap node**:
   ```bash
   # Add to config or run directly
   icnctl network add-peer 192.168.1.100:7777 did:icn:BOOTSTRAP_DID
   ```

2. **Request trust attestation**:
   Contact an existing member. They run:
   ```bash
   icnctl trust add did:icn:YOUR_DID 0.5 "Knows from community garden"
   ```

3. **Verify trust receipt**:
   ```bash
   icnctl trust score did:icn:YOUR_DID
   ```

4. **Join governance domain** (if required):
   ```bash
   icnctl gov domain join --domain-id coop:food-collective
   ```

### Your First Transaction

Once trusted, you can participate:

```bash
# Check your balance
icnctl ledger balance

# Check your credit limit
icnctl ledger limit

# Send credits
icnctl ledger pay did:icn:RECIPIENT_DID 2.0 hours "Helped with inventory"

# View transaction history
icnctl ledger history --limit 10
```

---

## Chapter 7: Using the Web Interface

**Member**

### Dashboard Overview

Access the Web UI at `http://localhost:8080` (or your gateway address).

**Dashboard shows:**
- Current balance across currencies
- Recent transactions
- Active proposals
- Trust score and network status

### Logging Time/Credits

1. Navigate to **Log Entry**
2. Select recipient (or use DID)
3. Enter amount and currency
4. Add description
5. Submit

### Viewing Balances

The **Balances** page shows:
- Current balance per currency
- Credit limit and available credit
- Transaction history with filters
- Export options (CSV)

### Governance Participation

1. Navigate to **Governance**
2. View active proposals
3. Click proposal to see details
4. Cast vote (For / Against / Abstain)
5. View results after voting period

---

# Part III: Daily Usage

## Chapter 8: Transactions

**Member**

### Sending Credits

```bash
# Basic transfer
icnctl ledger pay did:icn:RECIPIENT_DID 5.0 hours "Weekly tutoring session"

# With specific currency
icnctl ledger pay did:icn:RECIPIENT_DID 100.0 coop:food-coop:credits "Produce order"
```

### Receiving Credits

Credits appear automatically when someone pays you:

```bash
# Check for new transactions
icnctl ledger history --since "1 hour ago"

# View current balance
icnctl ledger balance
```

### Transaction History

```bash
# Recent transactions
icnctl ledger history --limit 20

# Filter by date range
icnctl ledger history --from 2025-01-01 --to 2025-12-31

# Export to CSV
icnctl ledger history --format csv > transactions.csv
```

### Credit Limits and How They Work

Your credit limit determines how negative your balance can go:

```
Base Limit + Trust Bonus + History Bonus = Your Limit
```

- **Base Limit**: Everyone starts with a baseline (e.g., 10 hours)
- **Trust Bonus**: Higher trust score = higher limit
- **History Bonus**: Cleared obligations increase limit over time

**New member throttling**: New members have reduced limits during probation period. These are example parameters; each cooperative configures credit policy per currency.

---

## Chapter 9: Trust Management

**Member**

### Adding Trust

```bash
# Add trust to someone you know
icnctl trust add did:icn:THEIR_DID 0.7 "Known from housing coop"

# Trust scores range from 0.0 to 1.0
# 0.3-0.5: Casual acquaintance
# 0.5-0.7: Regular collaborator
# 0.7-0.9: Trusted partner
# 0.9-1.0: Deep trust (use sparingly)
```

### Understanding Trust Scores

Trust is computed from the trust graph:

```bash
# View your trust score
icnctl trust score did:icn:YOUR_DID

# View trust graph around a DID
icnctl trust graph did:icn:SOME_DID --depth 2
```

**Trust decay**: Indirect trust decays by hop distance. The default decay factor reflects empirical social distance assumptions and is configurable per cooperative.

### Trust Classes

Your trust class determines rate limits and access:

| Class | Score | Rate Limit |
|-------|-------|------------|
| Isolated | < 0.1 | 10 msg/sec |
| Known | 0.1-0.4 | 50 msg/sec |
| Partner | 0.4-0.7 | 100 msg/sec |
| Federated | 0.7+ | 200 msg/sec |

---

## Chapter 10: Governance

**Member**

### Creating Proposals

```bash
# Create a text proposal
icnctl gov proposal create \
  --domain-id coop:food-collective \
  --title "Approve new produce supplier" \
  --body "Proposal to add Sunny Farms as approved supplier..." \
  --kind text

# Create a parameter change proposal
icnctl gov proposal create \
  --domain-id coop:food-collective \
  --title "Increase credit limit baseline" \
  --kind parameter \
  --parameter credit_baseline \
  --value 20
```

### Voting

```bash
# List active proposals
icnctl gov proposal list --domain-id coop:food-collective --status active

# View proposal details
icnctl gov proposal show PROPOSAL_ID

# Cast vote
icnctl gov vote cast --proposal-id PROPOSAL_ID --choice for
# Choices: for, against, abstain
```

### Governance Templates

ICN includes pre-built governance profiles:

| Template | Description |
|----------|-------------|
| `consensus-with-fallback` | Seek consensus, fall back to 2/3 vote |
| `sociocracy-consent` | Consent-based (no paramount objections) |
| `council-delegation` | Elected council makes decisions |
| `direct-democracy` | All members vote on everything |

### Understanding Decision Rules

Each domain has configured thresholds:

- **Quorum**: Minimum participation (e.g., 50% of members)
- **Threshold**: Approval percentage (e.g., 66% for major decisions)
- **Voting period**: Time window for votes (e.g., 7 days)

---

## Chapter 11: Disputes

**Member**

### When to File a Dispute

File a dispute when:
- Transaction was recorded incorrectly
- Service wasn't delivered as agreed
- You suspect fraudulent activity

Do not file disputes for:
- Disagreements about value (negotiate first)
- Technical errors (contact support)

### The Dispute Process

```bash
# File a dispute
icnctl ledger dispute \
  --entry-id ENTRY_ID \
  --reason "Service not delivered as described"

# View dispute status
icnctl ledger dispute show DISPUTE_ID
```

**Process:**
1. **Filing**: Dispute recorded, entry marked as contested
2. **Response**: Other party has time to respond
3. **Mediation**: If unresolved, mediator assigned
4. **Resolution**: Mediator decides outcome

### Resolution Outcomes

ICN never deletes history. Resolutions create new entries:

- **Reversed**: A corrective entry is appended that nets out the original
- **Settlement**: A follow-up entry adjusts the obligation per mediator decision
- **Write-off**: Governance records the obligation as forgiven; balances may be adjusted by a compensating entry depending on policy
- **Upheld**: Original entry stands, dispute closed

---

# Part IV: Administration

## Chapter 12: Starting a New Cooperative

**Admin**

### Planning Your Cooperative

Before creating a cooperative, decide:

1. **Type**: Economic (coop) or civic (community)?
2. **Governance**: Which decision-making model?
3. **Currency**: What units? (hours, credits, custom)
4. **Membership**: Open or invite-only?
5. **Founding members**: Minimum 3 recommended

### Initial Setup

```bash
# Create governance domain
icnctl gov domain create \
  --domain-id coop:my-food-coop \
  --name "My Food Cooperative" \
  --profile consensus-with-fallback

# Initialize ledger currency
icnctl ledger currency create \
  --domain-id coop:my-food-coop \
  --currency hours \
  --description "Labor hours"
```

### Configuring Credit Policies

```bash
# Set credit policy (example parameters - configure per your needs)
icnctl ledger policy set \
  --domain-id coop:my-food-coop \
  --baseline 10 \
  --trust-multiplier 2.0 \
  --new-member-limit 5 \
  --probation-days 30
```

### Inviting Members

```bash
# Generate invite link
icnctl invite create \
  --domain-id coop:my-food-coop \
  --expires "7 days" \
  --max-uses 10

# Or add member directly (if already trusted)
icnctl gov member add \
  --domain-id coop:my-food-coop \
  --did did:icn:NEW_MEMBER_DID
```

---

## Chapter 13: Cooperative Treasurer Guide

**Admin**

### Financial Overview

```bash
# View cooperative treasury balance
icnctl ledger balance --domain-id coop:my-food-coop --treasury

# View all member balances
icnctl ledger balances --domain-id coop:my-food-coop

# View circulation metrics
icnctl ledger stats --domain-id coop:my-food-coop
```

### Reports and Exports

```bash
# Export transactions for period
icnctl ledger history \
  --domain-id coop:my-food-coop \
  --from 2025-01-01 \
  --to 2025-12-31 \
  --format csv > annual.csv

# Export member balances
icnctl ledger balances \
  --domain-id coop:my-food-coop \
  --format csv > balances.csv
```

### Handling Defaults

When a member cannot repay their negative balance:

```bash
# View member's history
icnctl ledger history --did did:icn:MEMBER_DID

# Options:
# 1. Payment plan (governance decision)
# 2. Write-off (governance decision)
# 3. Membership suspension

# Initiate write-off proposal
icnctl gov proposal create \
  --domain-id coop:my-food-coop \
  --title "Write off debt for Member X" \
  --kind economic \
  --amount 50.0
```

### Audit Procedures

1. **Monthly reconciliation**: Compare ledger totals (should net to zero)
2. **Quarterly review**: Check unusual patterns
3. **Annual audit**: Full transaction review

```bash
# Verify ledger integrity
icnctl ledger verify --domain-id coop:my-food-coop

# Check total should be zero
icnctl ledger stats --domain-id coop:my-food-coop | grep "Net Balance"
```

---

## Chapter 14: Governance Administration

**Admin**

### Setting Up Domains

```bash
# Create main governance domain
icnctl gov domain create \
  --domain-id coop:my-coop \
  --name "My Cooperative" \
  --profile consensus-with-fallback

# Create sub-domain for specific decisions
icnctl gov domain create \
  --domain-id coop:my-coop:membership \
  --name "Membership Committee" \
  --parent coop:my-coop \
  --profile council-delegation
```

### Configuring Voting Rules

```bash
# Set quorum and threshold
icnctl gov config set \
  --domain-id coop:my-coop \
  --quorum 0.5 \
  --threshold 0.66 \
  --voting-period "7 days"

# Set different rules for different proposal types
icnctl gov config set \
  --domain-id coop:my-coop \
  --proposal-type constitutional \
  --threshold 0.75 \
  --voting-period "14 days"
```

### Managing Facilitators

```bash
# Assign facilitator role
icnctl gov role assign \
  --domain-id coop:my-coop \
  --did did:icn:FACILITATOR_DID \
  --role facilitator

# List current roles
icnctl gov role list --domain-id coop:my-coop
```

---

# Part V: Operations

## Chapter 15: Running a Node

**Operator**

### Node Setup

```bash
# Create configuration directory
mkdir -p ~/.icn

# Generate default config
icnd --generate-config > ~/.icn/icn.toml

# Edit configuration
nano ~/.icn/icn.toml
```

### Configuration Options

Key configuration sections in `~/.icn/icn.toml`:

```toml
[node]
data_dir = "~/.icn/data"
log_level = "info"

[network]
listen_addr = "0.0.0.0:7777"
bootstrap_peers = [
  "icn://did:icn:PEER1_DID@192.168.1.100:7777",
  "icn://did:icn:PEER2_DID@192.168.1.101:7777"
]

[gateway]
enabled = true
listen_addr = "127.0.0.1:8080"
jwt_secret = "your-secret-here"

[metrics]
enabled = true
listen_addr = "127.0.0.1:9100"
```

### Port Reference

| Service | Port | Protocol | Description |
|---------|------|----------|-------------|
| P2P Network | 7777 | UDP (QUIC) | Node-to-node communication |
| Gateway API | 8080 | HTTP/WS | REST API and WebSocket events |
| Metrics | 9100 | HTTP | Prometheus metrics endpoint |

Note: The daemon exposes no public TCP RPC by default; `icnctl` communicates via local IPC or the gateway when enabled.

### Monitoring

```bash
# Check daemon status
icnctl status

# View connected peers
icnctl network peers

# Check gossip sync status
icnctl network gossip status
```

**Prometheus metrics** available at `http://localhost:9100/metrics`:

```bash
# Key metrics to monitor
icn_network_connections_active
icn_gossip_entries_total
icn_ledger_quarantine_size
icn_fuel_consumed_total
```

### Health Checks

```bash
# API health endpoint
curl http://localhost:8080/health

# Detailed health
curl http://localhost:8080/health/detailed
```

---

## Chapter 16: Backup and Recovery

**Operator**

### Backup Procedures

```bash
# Full backup (recommended)
icnctl backup ~/backups/icn-$(date +%Y%m%d).tar.gz.age

# What's included:
# - Keystore (encrypted)
# - Ledger data
# - Trust graph
# - Gossip store
# - Configuration
```

### Restore Procedures

```bash
# Stop daemon first
sudo systemctl stop icnd

# Restore from backup
icnctl restore ~/backups/icn-20251214.tar.gz.age --force

# Restart daemon
sudo systemctl start icnd

# Verify
icnctl status
icnctl ledger balance
```

### Disaster Recovery

If keystore is lost without backup:

1. **Your identity is permanently lost**
2. Create new identity: `icnctl id init`
3. Request trust re-attestation from cooperative members
4. Previous balances are not recoverable (coordinate with cooperative)

**Prevention**: Always maintain multiple backup copies in different locations.

---

## Chapter 17: Upgrades

**Operator**

### Version Checking

```bash
# Check current version
icnctl --version
icnd --version

# Check for updates (if configured)
icnctl update check
```

### Upgrade Procedures

```bash
# 1. Backup first (always!)
icnctl backup ~/backups/pre-upgrade-$(date +%Y%m%d).tar.gz.age

# 2. Stop daemon
sudo systemctl stop icnd

# 3. Update binaries
cd ~/icn/icn
git fetch origin
git checkout v0.2.1
cargo build --release
sudo cp target/release/{icnd,icnctl} /usr/local/bin/

# 4. Run migrations (if needed)
icnctl migrate --from v0.2.0 --to v0.2.1

# 5. Start daemon
sudo systemctl start icnd

# 6. Verify
icnctl status
```

### Rollback

If upgrade fails:

```bash
# Stop daemon
sudo systemctl stop icnd

# Restore backup
icnctl restore ~/backups/pre-upgrade-20251214.tar.gz.age --force

# Reinstall previous version
git checkout v0.2.0
cargo build --release
sudo cp target/release/{icnd,icnctl} /usr/local/bin/

# Start daemon
sudo systemctl start icnd
```

---

## Chapter 18: Troubleshooting

**Operator**

### Common Issues

#### "Identity not initialized"
```bash
icnctl id init
```

#### "Failed to unlock keystore"
Wrong passphrase. Try again or restore from backup.

#### "No peers connected"
```bash
# Check firewall
sudo ufw allow 7777/udp

# Verify bootstrap peers in config
grep bootstrap ~/.icn/icn.toml

# Manually add peer
icnctl network add-peer 192.168.1.100:7777 did:icn:PEER_DID
```

#### "Transaction rejected: insufficient credit"
You've hit your credit limit. Options:
- Receive credits from others
- Build more trust (increases limit)
- Wait for probation period to end

#### "Rate limited"
You've exhausted fuel. Wait for regeneration (up to 24 hours for full refill).

### Network Problems

```bash
# Check connectivity
icnctl network status

# View peer connections
icnctl network peers

# Test specific peer
icnctl network ping did:icn:PEER_DID

# Check logs for errors
journalctl -u icnd -f
```

### Identity Issues

```bash
# View keystore status
icnctl id show

# Rotate keys (if compromised)
icnctl id rotate

# Export identity for backup
icnctl id export --output ~/identity-backup.age
```

### Getting Help

1. **Logs**: `journalctl -u icnd -f` or `RUST_LOG=debug icnd`
2. **Documentation**: All docs in `docs/` directory
3. **GitHub Issues**: https://github.com/InterCooperative-Network/icn/issues

### Common First-Week Questions

**Q: Why is my trust score 0?**
A: You need attestations from existing members. Ask someone to run `icnctl trust add YOUR_DID 0.5`.

**Q: Why can't I send more credits?**
A: New members have throttled limits during probation. Build trust and history.

**Q: How do I see who trusts me?**
A: `icnctl trust graph did:icn:YOUR_DID --incoming`

**Q: My transaction is stuck?**
A: Gossip propagation can take 10-30 seconds. Check `icnctl ledger history`.

---

# Part VI: Advanced Topics

## Chapter 19: CCL Contracts

**Admin**

### Contract Language Overview

CCL (Cooperative Contract Language) is a domain-specific language for agreements:

- **Deterministic**: Same inputs always produce same outputs
- **Fuel-metered**: Execution has bounded cost
- **Capability-based**: Contracts request specific permissions
- **Not Turing-complete**: No infinite loops

Determinism applies to **state transitions** (ledger/governance/contract execution). Network delivery is eventually consistent; nodes converge on the same result once they receive the same signed inputs.

### Governance Templates

Pre-built contracts in `contracts/governance/`:

```
contracts/governance/
├── consensus-with-fallback.ccl
├── sociocracy-consent.ccl
├── council-delegation.ccl
├── direct-democracy.ccl
└── emergency-lock.ccl
```

### Writing Custom Contracts

```
contract MembershipApproval {
    capability ReadLedger;
    capability ReadTrust;

    rule approve_member(applicant: DID, sponsors: List<DID>) {
        require sponsors.len() >= 2;
        require all(sponsors, |s| trust_score(s) > 0.5);
        require all(sponsors, |s| member_age(s) > 90 days);

        create_membership(applicant);
    }
}
```

Deploy:
```bash
icnctl contract deploy \
  --domain-id coop:my-coop \
  --file ./membership-approval.ccl
```

---

## Chapter 20: Federation

**Admin**

### Multi-Coop Networks

Federations enable cooperation between cooperatives:

```bash
# Create federation
icnctl federation create \
  --federation-id federation:pacific-nw \
  --name "Pacific Northwest Federation"

# Join federation
icnctl federation join \
  --federation-id federation:pacific-nw \
  --coop-id coop:my-coop
```

### Trust Bridging

Trust flows across federation boundaries with decay. These are default federation policies; federations may override them:

| Scope | Trust Multiplier |
|-------|------------------|
| Within coop | 100% |
| Across federation | 70% |
| External | 50% |

### Credit Exchange

Federation members can exchange credits:

```bash
# View exchange rates
icnctl federation exchange rates

# Swap credits
icnctl federation exchange \
  --from coop:my-coop:hours \
  --to federation:pacific-nw:credits \
  --amount 10
```

---

## Chapter 21: Security

**Operator**

### Threat Model Overview

ICN's security model addresses:

| Threat | Mitigation |
|--------|------------|
| Key compromise | Multi-device, rotation, social recovery |
| Sybil attacks | Trust graph, attestation requirements |
| Network attacks | QUIC/TLS, signed messages |
| Economic attacks | Credit limits, dispute resolution |

### Security Features Status

| Feature | Status | Notes |
|---------|--------|-------|
| Signed envelopes (Ed25519) | Implemented | Integrity + authenticity |
| QUIC + TLS transport | Implemented | Link encryption |
| DID-TLS binding | Implemented | Identity verification |
| Replay protection | Implemented | Sequence-based |
| Optional payload encryption | Experimental | Depends on channel/topic |
| Onion routing / obfuscation | Experimental | Privacy enhancement |

Note: Onion routing and traffic obfuscation protect against **casual and passive observers**, not nation-state global adversaries.

### Key Management Best Practices

1. **Use strong passphrases**: 20+ characters recommended
2. **Backup immediately**: Multiple copies, different locations
3. **Rotate periodically**: Annual rotation recommended
4. **Enable multi-device**: Reduces single point of failure

### Incident Response

If you suspect key compromise:

```bash
# 1. Rotate keys immediately
icnctl id rotate

# 2. Notify cooperative
# (Use out-of-band communication)

# 3. Revoke old devices
icnctl device revoke --device-id OLD_DEVICE

# 4. Request trust re-attestation
# (Old attestations may be suspect)
```

See `docs/incident-response.md` for complete playbook.

---

## Chapter 22: Building on ICN

**Operator**

### Gateway API

The Gateway provides REST and WebSocket APIs:

**Base URL**: `http://localhost:8080`

**Authentication**:
```bash
# Get challenge
curl -X POST http://localhost:8080/v1/auth/challenge \
  -d '{"did": "did:icn:YOUR_DID"}'

# Sign challenge and get token
curl -X POST http://localhost:8080/v1/auth/token \
  -d '{"did": "did:icn:YOUR_DID", "signature": "...", "challenge": "..."}'

# Use token
curl -H "Authorization: Bearer TOKEN" \
  http://localhost:8080/v1/ledger/balance
```

### TypeScript SDK

```typescript
import { ICNClient } from '@icn/client';

const client = new ICNClient({ baseUrl: 'http://localhost:8080' });

// Authenticate
await client.authenticate(did, signer, 'my-coop', ['ledger:read']);

// Make payment
await client.pay('my-coop', {
  from: 'did:icn:alice',
  to: 'did:icn:bob',
  amount: 5,
  currency: 'hours',
});

// Submit compute task
const task = await client.submitTask({
  code: JSON.stringify(cclContract),
  fuel_limit: 10000,
});
```

### WebSocket Events

```javascript
const ws = new WebSocket('ws://localhost:8080/ws/coop:my-coop');

ws.onopen = () => {
  ws.send(JSON.stringify({ type: 'Auth', token: 'JWT_TOKEN' }));
};

ws.onmessage = (event) => {
  const data = JSON.parse(event.data);
  switch (data.type) {
    case 'LedgerEntry':
      console.log('New transaction:', data.entry);
      break;
    case 'Proposal':
      console.log('New proposal:', data.proposal);
      break;
    case 'Vote':
      console.log('Vote cast:', data.vote);
      break;
  }
};
```

---

# Part VII: Reference

## Appendix A: CLI Reference

### icnctl

```bash
icnctl <COMMAND> [OPTIONS]

COMMANDS:
  id          Identity management
  trust       Trust graph operations
  ledger      Ledger transactions
  gov         Governance operations
  network     Network management
  backup      Create backup
  restore     Restore from backup
  status      Daemon status
  device      Multi-device management
  contract    Contract deployment
  federation  Federation operations

GLOBAL OPTIONS:
  -v, --verbose    Verbose output
  -q, --quiet      Minimal output
  --config PATH    Config file path
  --help           Show help
  --version        Show version
```

### Key Commands

```bash
# Identity
icnctl id init                    # Create identity
icnctl id show                    # Show identity
icnctl id rotate                  # Rotate keys
icnctl id export                  # Export identity

# Trust
icnctl trust add <DID> <SCORE>    # Add trust
icnctl trust remove <DID>         # Remove trust
icnctl trust score <DID>          # Query score
icnctl trust graph <DID>          # View graph

# Ledger
icnctl ledger balance             # Check balance
icnctl ledger pay <DID> <AMT> <CUR> <MEMO>
icnctl ledger history             # Transaction history
icnctl ledger dispute             # File dispute

# Governance
icnctl gov domain create          # Create domain
icnctl gov proposal create        # Create proposal
icnctl gov vote cast              # Cast vote
icnctl gov proposal list          # List proposals

# Network
icnctl network status             # Network status
icnctl network peers              # List peers
icnctl network add-peer           # Add peer

# Operations
icnctl backup <PATH>              # Create backup
icnctl restore <PATH>             # Restore backup
icnctl status                     # Daemon status
```

---

## Appendix B: Configuration Reference

### Complete icn.toml

```toml
[node]
# Data directory for all persistent state
data_dir = "~/.icn/data"

# Logging level: trace, debug, info, warn, error
log_level = "info"

# Node identity (defaults to keystore identity)
# identity = "did:icn:..."

[network]
# Listen address for P2P connections (QUIC/UDP)
listen_addr = "0.0.0.0:7777"

# Bootstrap peers for initial network discovery
bootstrap_peers = [
  "icn://did:icn:PEER1@192.168.1.100:7777",
  "icn://did:icn:PEER2@192.168.1.101:7777"
]

# Enable mDNS for local network discovery
mdns_enabled = true

# Maximum number of peer connections
max_peers = 50

[gateway]
# Enable Gateway API
enabled = true

# Listen address for HTTP/WebSocket
listen_addr = "127.0.0.1:8080"

# JWT secret for authentication (generate securely!)
jwt_secret = "your-256-bit-secret-here"

# JWT token expiry
token_expiry = "24h"

# Rate limiting per DID
rate_limit_per_did = 100

[metrics]
# Enable Prometheus metrics
enabled = true

# Listen address for metrics endpoint
listen_addr = "127.0.0.1:9100"

[gossip]
# Maximum entries in gossip store
max_entries = 100000

# Anti-entropy interval
sync_interval = "30s"

# Message TTL
message_ttl = "24h"

[ledger]
# Enable quarantine for conflicting entries
quarantine_enabled = true

# Maximum quarantine size
max_quarantine = 1000

[compute]
# Enable distributed compute
enabled = true

# Maximum fuel per job
max_fuel_per_job = 100000

# Concurrent job limit
max_concurrent_jobs = 10
```

### Environment Variables

| Variable | Description | Default |
|----------|-------------|---------|
| `ICN_CONFIG` | Config file path | `~/.icn/icn.toml` |
| `ICN_DATA_DIR` | Data directory | `~/.icn/data` |
| `ICN_LOG_LEVEL` | Log level | `info` |
| `ICN_KEYSTORE_PASSPHRASE` | Keystore passphrase | (prompt) |

---

## Appendix C: API Reference

### REST Endpoints

| Endpoint | Method | Auth | Description |
|----------|--------|------|-------------|
| `/v1/auth/challenge` | POST | No | Get auth challenge |
| `/v1/auth/token` | POST | No | Exchange signature for token |
| `/v1/identity` | GET | Yes | Current identity |
| `/v1/ledger/balance` | GET | Yes | Account balances |
| `/v1/ledger/transfer` | POST | Yes | Create transfer |
| `/v1/ledger/history` | GET | Yes | Transaction history |
| `/v1/governance/proposals` | GET | Yes | List proposals |
| `/v1/governance/proposals` | POST | Yes | Create proposal |
| `/v1/governance/vote` | POST | Yes | Cast vote |
| `/v1/trust/score` | GET | Yes | Query trust score |
| `/health` | GET | No | Health check |
| `/health/detailed` | GET | No | Detailed health |

---

## Appendix D: Metrics Reference

| Metric | Type | Description |
|--------|------|-------------|
| `icn_network_connections_active` | Gauge | Current peer connections |
| `icn_network_bytes_sent_total` | Counter | Total bytes sent |
| `icn_network_bytes_received_total` | Counter | Total bytes received |
| `icn_gossip_entries_total` | Gauge | Gossip store size |
| `icn_gossip_messages_sent_total` | Counter | Gossip messages sent |
| `icn_ledger_entries_total` | Gauge | Total ledger entries |
| `icn_ledger_quarantine_size` | Gauge | Quarantined entries |
| `icn_trust_cache_hits_total` | Counter | Trust cache hits |
| `icn_trust_cache_misses_total` | Counter | Trust cache misses |
| `icn_fuel_consumed_total` | Counter | Total fuel consumed |
| `icn_fuel_regenerated_total` | Counter | Total fuel regenerated |
| `icn_compute_jobs_total` | Counter | Compute jobs executed |
| `icn_compute_fuel_used_total` | Counter | Compute fuel used |

---

## Appendix E: Error Codes

| Code | Message | Solution |
|------|---------|----------|
| `E001` | Identity not initialized | Run `icnctl id init` |
| `E002` | Failed to unlock keystore | Check passphrase |
| `E003` | No peers connected | Check config, firewall |
| `E004` | Transaction rejected | Check credit limit |
| `E005` | Proposal not found | Verify proposal ID |
| `E006` | Not authorized | Check permissions |
| `E007` | Rate limited | Wait for fuel regeneration |
| `E008` | Insufficient fuel | Wait or reduce operations |
| `E009` | Invalid signature | Key mismatch or corruption |
| `E010` | Gossip sync failed | Check network connectivity |
| `E011` | Quarantine full | Resolve conflicts |
| `E012` | Contract execution failed | Check CCL code |

---

## Appendix F: Migration Guides

### v0.1.x to v0.2.x

**Breaking Changes:**
1. Config key renames in TOML
2. Metrics port moved from 9090 to 9100
3. CLI command restructuring: `icnctl pay` → `icnctl ledger pay`

**Procedure:**
```bash
# Backup
icnctl backup ~/pre-migration.tar.gz.age

# Stop daemon
sudo systemctl stop icnd

# Build new version
cd icn/icn
git checkout v0.2.0
cargo build --release
sudo cp target/release/{icnd,icnctl} /usr/local/bin/

# Run migration
icnctl migrate --from v0.1.x --to v0.2.x

# Start daemon
sudo systemctl start icnd
```

---

## Appendix G: Legal and Compliance

### GDPR

ICN provides tools for GDPR compliance:

| Right | Support |
|-------|---------|
| Access | `icnctl backup` exports all data |
| Portability | `icnctl ledger history --format csv` |
| Rectification | File dispute for corrections |

**Erasure Note:** ICN minimizes personal data. For erasure requests:
- Delete local mappings (names, labels)
- Stop re-publishing optional metadata
- Ledger entries are audit records and may be retained for accounting integrity

### Tax Reporting

Cooperatives should:
1. Export transaction history regularly
2. Consult local tax regulations for mutual credit treatment
3. Maintain records per jurisdiction requirements

---

## Appendix H: Contributing

### Ways to Contribute

- Report bugs via GitHub Issues
- Improve documentation
- Submit code (see CONTRIBUTING.md)
- Join pilot deployments
- Translate documentation

### Development Setup

```bash
git clone https://github.com/InterCooperative-Network/icn.git
cd icn/icn
cargo build
cargo test
```

### Code Standards

- Rust 2021 edition
- `cargo fmt` before commits
- `cargo clippy` warnings addressed
- Tests for new functionality

---

# About This Manual

**ICN User Manual v1.1.1**
December 2025

**License:** CC BY-SA 4.0

**Repository:** https://github.com/InterCooperative-Network/icn

**Feedback:** File issues at the repository or contribute improvements.

---

*Welcome to the cooperative internet!*
