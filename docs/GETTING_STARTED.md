# Getting Started with ICN

Welcome to ICN (Intercooperative Network)! This guide will help you get up and running with ICN in minutes.

## What is ICN?

ICN is a **peer-to-peer coordination substrate** for cooperatives and community organizations. It provides:

- **Decentralized Identity** (DIDs) with Ed25519 cryptography
- **Trust Graph** for reputation and access control
- **Mutual Credit Ledger** for community currency
- **Gossip Protocol** for P2P communication
- **Cooperative Contracts** (CCL) for programmable agreements
- **Governance Primitives** for democratic decision-making

Think of it as infrastructure for the cooperative economy - like Git for collaboration, but for economic coordination.

---

## Quick Start (5 minutes)

### 1. Install ICN

**One-line install** (Linux/macOS):
```bash
curl -fsSL https://raw.githubusercontent.com/InterCooperative-Network/icn/main/scripts/install.sh | bash
```

**Manual install** (build from source):
```bash
git clone https://github.com/InterCooperative-Network/icn.git
cd icn/icn
cargo build --release
sudo cp target/release/{icnd,icnctl} /usr/local/bin/
```

**Verify installation:**
```bash
icnctl --version
icnd --version
```

### 2. Create Your Identity

ICN uses decentralized identifiers (DIDs) instead of usernames. Let's create yours:

```bash
# Use an explicit data dir so icnctl and icnd read the same identity state
icnctl --data-dir ~/.icn id init
```

You'll be prompted to:
1. **Choose a passphrase** (encrypts your private key)
2. **Confirm the passphrase**

Your DID will look like: `did:icn:5Xk8Y2r...` (based on your public key)

**View your identity:**
```bash
icnctl --data-dir ~/.icn id show
```

### 3. Start the Daemon

ICN runs as a background daemon (`icnd`):

```bash
# Start manually (foreground, for testing)
icnd --data-dir ~/.icn

# Or install as a system service
sudo systemctl enable --now icnd  # Linux
sudo launchctl load /Library/LaunchDaemons/com.icn.icnd.plist  # macOS
```

**Check daemon status:**
```bash
icnctl --data-dir ~/.icn status
```

### 4. Join a Cooperative

If you're joining an existing cooperative, you need:
1. The **cooperative's invite link** or **bootstrap node address**
2. **Trust attestation** from an existing member

```bash
# Connect to a bootstrap node
icnctl network add-peer 192.168.1.100:7777 did:icn:abc123...

# Request trust from a member
# (They run: icnctl trust add did:icn:YOUR_DID 0.5)
```

### 5. Your First Transaction

Once you're trusted by the cooperative, you can participate in the mutual credit ledger:

```bash
# Check your balance
icnctl ledger balance

# Record a transaction (e.g., "I gave Alice 2 hours of help")
icnctl ledger pay did:icn:alice... 2 hours "Gardening help"

# View transaction history
icnctl ledger history
```

---

## Core Concepts

### Identity & Trust

- **DID (Decentralized Identifier)**: Your cryptographic identity (e.g., `did:icn:5Xk8Y...`)
- **Keystore**: Encrypted file storing your private key (`{data_dir}/identity.age`, for example `~/.icn/identity.age` when using `--data-dir ~/.icn`)
- **Trust Score**: How much the network trusts you (0.0 to 1.0)
- **Trust Graph**: Web of trust relationships between members

Trust is **transitive**: If Alice trusts Bob (0.8) and Bob trusts Carol (0.6), Alice implicitly trusts Carol (~0.48).

### Mutual Credit Ledger

- **Mutual Credit**: Community currency where balances net to zero
- **Double-Entry Accounting**: Every transaction has a debit and credit
- **Credit Limits**: Maximum negative balance (based on trust + history)
- **Currencies**: Hours, USD, credits, kWh, etc. (configurable per cooperative)

**Example:**
```
Alice helps Bob for 2 hours
  Alice: +2 hours
  Bob: -2 hours
  Network total: 0 hours
```

### Gossip Protocol

ICN uses **gossip** (epidemic broadcast) for P2P communication:

- **Topics**: Channels like `ledger:sync`, `governance:proposals`, `compute:tasks`
- **Access Control**: Public, private, or trust-gated topics
- **Anti-Entropy**: Automatic conflict resolution and missing data repair
- **Vector Clocks**: Track causal ordering of events

You don't need to understand gossip to use ICN - it's automatic!

### Governance

ICN provides **primitives** for democratic decision-making:

- **Domains**: Governance contexts (e.g., "food-coop:membership")
- **Proposals**: Motions to be voted on
- **Voting**: For/Against/Abstain with configurable thresholds
- **Profiles**: Decision rules (consensus, majority, consent, etc.)

```bash
# Create a governance domain
icnctl gov domain create --domain-id my-coop --name "My Cooperative"

# Create a proposal
icnctl gov proposal create --domain-id my-coop --title "Approve new supplier" --kind text

# Vote
icnctl gov vote cast --proposal-id abc123 --choice for
```

---

## Common Workflows

### Set Up a New Cooperative

1. **Choose a member** to initialize:
   ```bash
   icnctl init-coop --name "My Food Coop" --members "did:icn:alice,did:icn:bob,did:icn:carol"
   ```

2. **Set initial trust**:
   ```bash
   # Members trust each other
   icnctl trust add did:icn:alice 0.8
   icnctl trust add did:icn:bob 0.8
   ```

3. **Create governance domain**:
   ```bash
   icnctl gov domain create --domain-id food-coop --name "Food Coop Governance"
   ```

4. **Set credit policies** (optional):
   ```bash
   # Via CCL contract or manual configuration
   # See docs/economic-safety.md for details
   ```

### Backup Your Identity

**IMPORTANT**: Your keystore is your identity. Back it up!

```bash
# Backup entire data directory (keystore + ledger + trust graph)
icnctl backup ~/icn-backup-$(date +%Y%m%d).tar.gz.age

# Restore from backup
icnctl restore ~/icn-backup-20251121.tar.gz.age --force
```

Store backups:
- **Off-site** (cloud storage, different physical location)
- **Encrypted** (backups are Age-encrypted with your passphrase)
- **Regularly** (weekly for active cooperatives)

### Monitor Your Node

1. **Web Dashboard**:
   ```
   Open http://localhost:8080 in your browser
   ```

2. **Prometheus Metrics**:
   ```
   curl http://localhost:9100/metrics
   ```

3. **Health Check**:
   ```bash
   curl http://localhost:8080/health
   icnctl network status
   ```

### Multi-Device Setup

Use your identity on multiple devices (laptop + phone):

```bash
# On Device 1 (primary)
icnctl device add --name "My Phone" --capabilities sign

# On Device 2 (new device)
# 1. Copy the device keypair from Device 1
# 2. Import and approve the device
icnctl device list
```

See [Multi-Device Identity Design](design/multi-device-identity-design.md) for details.

---

## Authentication for Web/Mobile Apps

ICN provides a **Gateway API** for building user-facing applications:

1. **Get a JWT token**:
   ```bash
   icnctl auth token --gateway http://localhost:8080 --coop-id my-coop
   ```

2. **Use the token** in HTTP requests:
   ```bash
   curl -H "Authorization: Bearer eyJ0eXAi..." \
        http://localhost:8080/v1/coops/my-coop
   ```

3. **WebSocket for real-time updates**:
   ```javascript
   const ws = new WebSocket('ws://localhost:8080/ws/my-coop');
   ws.send(JSON.stringify({type: 'Auth', token: 'eyJ0eXAi...'}));
   ```

See [Platform Layer Design](design/platform-layer-design.md) and the TypeScript SDK ([sdk/typescript/](../sdk/typescript/)).

---

## Troubleshooting

### "Identity already exists"
You've already run `icnctl id init`. View your existing identity:
```bash
icnctl id show
```

### "Failed to unlock keystore"
Wrong passphrase. Try again or restore from backup.

### "No peers connected"
Your node hasn't discovered peers yet. Either:
1. Wait for mDNS discovery (~30 seconds on LAN)
2. Manually add a peer: `icnctl network add-peer <addr> <did>`
3. Check firewall settings (port 7777/UDP for QUIC)

### "Transaction rejected: insufficient credit"
You've hit your credit limit. Either:
1. Receive payments from others (increase balance)
2. Build more trust (increases limit)
3. Wait for credit limit ramp (new members have throttled limits)

### "Proposal not found"
The proposal hasn't gossiped to your node yet. Wait 10-30 seconds and try again.

### Logs and Debugging
```bash
# View logs (systemd)
journalctl -u icnd -f

# View logs (manual run)
RUST_LOG=debug icnd

# Check network connectivity
icnctl network peers
icnctl network status
```

---

## Next Steps

### Learn More
- [Architecture Overview](ARCHITECTURE.md) - How ICN works under the hood
- [Operations Guide](guides/operations/operations-guide.md) - Day-to-day management
- [Economic Safety](design/economics/economic-safety.md) - Credit limits, dispute resolution
- [Governance Primitives](design/governance/governance-primitives.md) - Democratic decision-making
- [Deployment Guide](operations/deployment/deployment-guide.md) - Production deployment

### Build on ICN

**Quick TypeScript Example**:
```typescript
import { ICNClient } from '@icn/client';

const client = new ICNClient({ baseUrl: 'http://localhost:8080' });

// Authenticate (you provide signing logic)
await client.authenticate('did:icn:alice', signer, 'my-coop', ['ledger:read']);

// Make a payment
await client.pay('my-coop', {
  from: 'did:icn:alice',
  to: 'did:icn:bob',
  amount: 5,
  currency: 'hours',
});

// Submit a compute task
const task = await client.submitTask({
  code: JSON.stringify(cclContract),
  fuel_limit: 10000,
});
const result = await client.waitForTask(task.task_hash);
```

**More Resources**:
- [Gateway API Reference](reference/api/API_REFERENCE.md) - REST + WebSocket API
- [TypeScript SDK](../sdk/typescript/) - `@icn/client` npm package
- [Platform Layer Design](design/platform-layer-design.md) - App architecture

### Get Help
- **GitHub Issues**: https://github.com/InterCooperative-Network/icn/issues
- **Documentation**: All docs in `docs/` directory
- **Examples**: See `examples/` directory

### Contribute
- [Contributing Guide](../CONTRIBUTING.md) - How to contribute
- [Code of Conduct](../CODE_OF_CONDUCT.md) - Community standards
- [Phase History](PHASE_HISTORY.md) - Completed development phases
- [Current TODO](TODO.md) - What's being built next

---

## Quick Reference Card

```bash
# Identity
icnctl id init              # Create new identity
icnctl id show              # View your DID
icnctl id rotate            # Rotate to new keypair

# Trust
icnctl trust add <did> <score>     # Add trust edge
icnctl trust list                  # View trust graph
icnctl trust score <did>           # Query trust score

# Ledger
icnctl ledger balance              # Check balance
icnctl ledger pay <did> <amount> <currency> <memo>
icnctl ledger history              # Transaction history

# Governance
icnctl gov domain create --domain-id <id> --name <name>
icnctl gov proposal create --domain-id <id> --title <title>
icnctl gov vote cast --proposal-id <id> --choice for|against|abstain

# Network
icnctl network status              # Show peers, gossip state
icnctl network add-peer <addr> <did>  # Manual peering

# Backup
icnctl backup <path>               # Create encrypted backup
icnctl restore <path>              # Restore from backup

# Daemon
icnd                               # Start daemon (foreground)
icnctl status                      # Check daemon status
```

---

**Welcome to the cooperative internet!** 🎉

If you have questions, check the [docs/](.) directory or open a [GitHub issue](https://github.com/InterCooperative-Network/icn/issues).
