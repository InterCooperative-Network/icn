# ICN Pilot Deployment Playbook

A step-by-step guide for deploying ICN to a pilot cooperative community.

## Overview

This playbook covers the complete lifecycle of an ICN pilot deployment:
1. Pre-deployment preparation
2. Infrastructure setup
3. Initial node deployment
4. Member onboarding
5. Operational validation
6. Day-2 operations
7. Success metrics

## Phase 1: Pre-Deployment Preparation

### 1.1 Community Assessment

Before deploying ICN, validate the pilot community is ready:

**Technical Requirements:**
- [ ] At least 3-5 members with technical comfort (can follow CLI instructions)
- [ ] Stable internet connectivity for all members
- [ ] Mix of devices (Linux, macOS, or WSL on Windows)
- [ ] Backup communication channel (Signal, Matrix, etc.)

**Organizational Requirements:**
- [ ] Designated coordinator/champion
- [ ] Clear governance expectations (voting rules, quorum)
- [ ] Initial use case defined (timebank, mutual aid, etc.)
- [ ] Members briefed on mutual credit concepts

**Checklist Questions:**
```
1. What is your primary use case? (timebanking, resource sharing, etc.)
2. How many members will participate initially?
3. What devices/platforms will members use?
4. Who will serve as the technical coordinator?
5. What existing governance rules should ICN mirror?
6. What's your backup communication method if ICN has issues?
```

### 1.2 Network Planning

Plan your network topology:

**Small Pilot (3-10 members):**
- All nodes peer-to-peer via mDNS on local network
- Or use a central bootstrap node for geographically distributed members

**Bootstrap Node Selection:**
- Choose most reliable connection (uptime, bandwidth)
- Static IP or dynamic DNS recommended
- Will be listed in all member configs

**Example topology:**
```
                    ┌─────────────┐
                    │  Bootstrap  │
                    │   (Alice)   │
                    └──────┬──────┘
                           │
           ┌───────────────┼───────────────┐
           │               │               │
    ┌──────┴──────┐ ┌──────┴──────┐ ┌──────┴──────┐
    │    Bob      │ │   Carol     │ │   David     │
    └─────────────┘ └─────────────┘ └─────────────┘
```

### 1.3 Prepare Bootstrap Node

On the bootstrap node (most reliable member):

```bash
# Install ICN
curl -sSL https://icn.coop/install.sh | bash

# Initialize identity
icnctl id init

# Get DID for other members to connect
icnctl id show
# Output: did:icn:abc123...

# Get IP address
hostname -I | awk '{print $1}'
# Or for public IP: curl -s ifconfig.me
```

Record this information:
- Bootstrap DID: `did:icn:...`
- Bootstrap address: `<IP>:7777`
- Bootstrap URI: `icn://did:icn:abc123@<IP>:7777`

## Phase 2: Infrastructure Setup

### 2.1 Installation (All Members)

Each member runs:

```bash
# One-line install
curl -sSL https://icn.coop/install.sh | bash

# Or build from source (if binaries unavailable)
curl -sSL https://icn.coop/install.sh | bash -s -- --build

# Initialize identity
icnctl id init
# Enter a strong passphrase (save it securely!)

# Verify installation
icnctl id show
```

### 2.2 Configuration

Edit `~/.icn/icn.toml` for each member:

**Bootstrap Node:**
```toml
data_dir = "~/.icn"

[network]
mdns_enabled = true
listen_addr = "0.0.0.0:7777"
rpc_port = 5601

[observability]
metrics_port = 9090
health_port = 8080
log_level = "info"

[gateway]
enabled = true
bind_addr = "127.0.0.1:8080"
jwt_secret = "<generate-32-char-random-string>"
token_expiry_hours = 24
```

**Member Nodes:**
```toml
data_dir = "~/.icn"

[network]
mdns_enabled = true
listen_addr = "0.0.0.0:7777"
rpc_port = 5601

# Connect to bootstrap node
bootstrap_peers = ["icn://did:icn:<BOOTSTRAP_DID>@<BOOTSTRAP_IP>:7777"]

[observability]
metrics_port = 9090
health_port = 8080
log_level = "info"

[gateway]
enabled = false
```

### 2.3 Start Nodes

**Using systemd (Linux):**
```bash
# Start daemon
systemctl --user start icnd

# Enable on boot
systemctl --user enable icnd

# Check status
systemctl --user status icnd

# View logs
journalctl --user -u icnd -f
```

**Using launchd (macOS):**
```bash
# Load agent
launchctl load ~/Library/LaunchAgents/coop.icn.icnd.plist

# Start service
launchctl start coop.icn.icnd

# View logs
tail -f ~/.icn/logs/icnd.log
```

**Manual start (testing):**
```bash
icnd --config ~/.icn/icn.toml
```

### 2.4 Verify Connectivity

On each node:

```bash
# Check node status
icnctl status

# Expected output:
# Node Status
# -----------
# DID: did:icn:...
# Network: 3 peers connected
# Gossip: 5 topics subscribed
# Ledger: 0 entries

# List connected peers
icnctl peers list

# Test connectivity to specific peer
icnctl peers ping did:icn:<peer-did>
```

## Phase 3: Initialize Cooperative

### 3.1 Create the Cooperative

On the bootstrap node (as coordinator):

```bash
# Interactive wizard
icnctl init-coop

# Or with flags
icnctl init-coop \
  --name "Our Food Coop" \
  --members "did:icn:alice,did:icn:bob,did:icn:carol"
```

The wizard will:
1. Create an identity (if none exists)
2. Initialize local storage
3. Set up the trust graph with founding members
4. Create governance domain
5. Optionally start the daemon

### 3.2 Establish Initial Trust

Each member vouches for members they know:

```bash
# Alice vouches for Bob
icnctl trust set \
  --target did:icn:bob \
  --score 0.8 \
  --labels "founding member"

# Bob vouches for Alice
icnctl trust set \
  --target did:icn:alice \
  --score 0.8 \
  --labels "founding member"

# View trust graph
icnctl trust list
```

**Trust Score Guidelines:**
- 0.8-1.0: Close collaborators, founding members
- 0.5-0.7: Regular members you know well
- 0.3-0.4: New members or casual acquaintances
- < 0.3: Unknown or minimal interaction

### 3.3 Create Governance Domain

```bash
# Create domain for the cooperative
icnctl gov domain create \
  --domain-id "coop:foodcoop" \
  --name "Food Coop Governance" \
  --members "did:icn:alice,did:icn:bob,did:icn:carol"

# Verify
icnctl gov domain show --domain-id "coop:foodcoop"
```

## Phase 4: Member Onboarding

### 4.1 Onboarding Workflow

For each new member:

1. **Install & Initialize:**
   ```bash
   curl -sSL https://icn.coop/install.sh | bash
   icnctl id init
   icnctl id show  # Share DID with coordinator
   ```

2. **Configure & Connect:**
   - Edit `~/.icn/icn.toml` with bootstrap peer
   - Start icnd service
   - Verify connectivity with `icnctl status`

3. **Coordinator Actions:**
   ```bash
   # Add to governance domain
   icnctl gov domain add-member \
     --domain-id "coop:foodcoop" \
     --member did:icn:newmember

   # Set initial trust
   icnctl trust set \
     --target did:icn:newmember \
     --score 0.3 \
     --labels "new member"
   ```

4. **Community Trust Building:**
   - Existing members vouch for new member
   - Trust scores increase with positive interactions

### 4.2 First Transaction

Test mutual credit with a simple transaction:

```bash
# Alice pays Bob 1 hour for helping with setup
icnctl ledger pay \
  --to did:icn:bob \
  --amount 1.0 \
  --currency hours \
  --memo "Help with initial setup"

# Check balances
icnctl ledger balance
# Alice: -1.0 hours
# Bob: +1.0 hours

# View transaction history
icnctl ledger history
```

### 4.3 First Governance Vote

Create and vote on a proposal:

```bash
# Create proposal
icnctl gov proposal create \
  --domain-id "coop:foodcoop" \
  --title "Approve monthly meeting schedule" \
  --kind text

# Open for voting
icnctl gov proposal open --proposal-id <id>

# Each member votes
icnctl gov vote cast --proposal-id <id> --choice for

# Check results
icnctl gov vote show --proposal-id <id>

# Close when ready
icnctl gov proposal close --proposal-id <id>
```

## Phase 5: Operational Validation

### 5.1 Health Checks

Run validation checklist:

```bash
# On each node
icnctl status       # Should show connected peers
icnctl peers list   # Should list all members
icnctl trust list   # Should show trust graph

# Test gossip propagation
# On one node:
icnctl ledger pay --to did:icn:peer --amount 0.1 --currency test --memo "propagation test"

# On other nodes (within 30 seconds):
icnctl ledger history  # Should show the transaction
```

### 5.2 Monitoring Setup

Set up Prometheus monitoring:

```yaml
# prometheus.yml
scrape_configs:
  - job_name: 'icn-nodes'
    static_configs:
      - targets:
        - 'alice-node:9090'
        - 'bob-node:9090'
        - 'carol-node:9090'
```

Key metrics to monitor:
- `icn_network_connections_active` - Peer connectivity
- `icn_gossip_announces_sent_total` - Message propagation
- `icn_ledger_entries_total` - Transaction volume
- `icn_ledger_entries_quarantined` - Conflict detection

### 5.3 Backup Verification

Test backup/restore on bootstrap node:

```bash
# Create backup
icnctl backup create --output ~/icn-backup.tar.enc

# Simulate restore (on test machine or after reset)
icnctl backup restore --input ~/icn-backup.tar.enc

# Verify restoration
icnctl id show
icnctl trust list
icnctl ledger balance
```

## Phase 6: Day-2 Operations

### 6.1 Regular Maintenance

**Daily:**
- Monitor health endpoint: `curl http://localhost:8080/health`
- Check for warnings in logs

**Weekly:**
- Review metrics dashboard
- Check disk usage
- Verify peer connectivity

**Monthly:**
- Create backup: `icnctl backup create -o backup-$(date +%Y%m).tar.enc`
- Review trust scores
- Archive old proposals

### 6.2 Common Operations

**Rotate Identity (if compromised):**
```bash
icnctl id rotate
# Automatically announces new key to network
```

**Remove Unresponsive Member:**
```bash
# Create governance proposal first
icnctl gov proposal create \
  --domain-id "coop:foodcoop" \
  --title "Remove inactive member" \
  --kind membership

# After approval
icnctl gov domain remove-member \
  --domain-id "coop:foodcoop" \
  --member did:icn:inactive
```

**Handle Disputes:**
```bash
# File dispute
icnctl dispute file \
  --counterparty did:icn:other \
  --amount 5.0 \
  --currency hours \
  --description "Work not completed"

# Mediator resolves
icnctl dispute resolve \
  --dispute-id <id> \
  --resolution "partial_payment" \
  --amount 2.5
```

### 6.3 Incident Response

See [docs/incident-response.md](incident-response.md) for detailed procedures.

**Quick Reference:**
- Node down: Check systemd status, logs, disk space
- Sync issues: Restart node, check peer connectivity
- Trust issues: Review trust graph, check for expired vouches
- Ledger conflicts: Check quarantine, review transaction history

## Phase 7: Success Metrics

### 7.1 Technical Metrics

Track weekly:
- Network uptime (% of time all nodes connected)
- Message propagation time (gossip latency)
- Transaction success rate
- Conflict/quarantine rate

Targets:
- Network uptime: >95%
- Propagation time: <5 seconds
- Transaction success: >99%
- Conflict rate: <1%

### 7.2 Community Metrics

Track monthly:
- Active members (made transaction in last 30 days)
- Transaction volume (total and per member)
- Proposal participation rate
- Trust graph density

Targets:
- Active members: >80% of total
- Proposals: >50% participation
- Trust density: >0.3 (avg edges per node)

### 7.3 Feedback Collection

Collect structured feedback after:
- 1 week: Initial impressions, setup difficulties
- 1 month: Usability, missing features
- 3 months: Impact on community, recommendations

## Appendix

### A. Quick Reference Commands

```bash
# Identity
icnctl id init          # Create identity
icnctl id show          # Display DID
icnctl id rotate        # Rotate keys

# Network
icnctl status           # Node status
icnctl peers list       # Connected peers
icnctl peers dial       # Connect to peer

# Trust
icnctl trust set        # Add trust edge
icnctl trust list       # View trust graph
icnctl trust score      # Get trust score

# Ledger
icnctl ledger pay       # Create payment
icnctl ledger balance   # Check balance
icnctl ledger history   # Transaction history

# Governance
icnctl gov domain create      # Create domain
icnctl gov proposal create    # Create proposal
icnctl gov vote cast          # Cast vote
icnctl gov proposal close     # Close voting

# Operations
icnctl backup create    # Create backup
icnctl backup restore   # Restore backup
icnctl migrate          # Run migrations
```

### B. Troubleshooting

**"Connection refused" on dial:**
- Check firewall allows port 7777
- Verify peer is running: `icnctl status` on peer
- Check bootstrap URI is correct

**"No peers found":**
- If on same network: Check mDNS (port 5353 UDP)
- If remote: Verify bootstrap_peers in config
- Restart icnd to re-discover

**"Ledger entry quarantined":**
- Check for double-spending
- Review `icnctl ledger quarantine list`
- May need mediator resolution

**"Proposal not passing":**
- Check quorum requirements
- Verify all members have voted
- Review `icnctl gov vote show --proposal-id <id>`

### C. Security Checklist

Pre-deployment:
- [ ] Strong passphrase for keystore (16+ chars)
- [ ] JWT secret is random and secure
- [ ] Gateway bound to localhost unless needed

Operational:
- [ ] Regular backups (weekly minimum)
- [ ] Monitor for unauthorized DIDs
- [ ] Review trust graph periodically
- [ ] Rotate keys if compromise suspected

Network:
- [ ] Firewall configured (7777 TCP/UDP only)
- [ ] TLS certificates valid
- [ ] No exposed RPC ports to internet

### D. Support Resources

- Documentation: https://icn.coop/docs
- Community: https://icn.coop/community
- Issues: https://github.com/anthropics/icn/issues
- Security: security@icn.coop

---

## Changelog

- 2025-01-17: Initial playbook created
