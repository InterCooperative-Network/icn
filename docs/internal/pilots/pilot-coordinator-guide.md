# ICN Pilot Coordinator Guide

A practical guide for cooperative coordinators deploying ICN (Intercooperative Network) for their community timebank or mutual credit system.

## Overview

This guide walks you through:
1. Setting up ICN for your cooperative
2. Onboarding members
3. Using the web interface for daily operations
4. Managing governance and proposals
5. Troubleshooting common issues

**Time required**: ~30 minutes for initial setup

## Prerequisites

Before starting, ensure you have:
- A Linux server (Ubuntu 20.04+ or similar) with:
  - 2GB RAM minimum
  - 10GB disk space
  - Public IP address (for remote members)
- SSH access to your server
- Basic comfort with terminal commands

## Quick Start

For the fastest setup, use our quickstart script:

```bash
# Download and run the quickstart script
curl -sSL https://raw.githubusercontent.com/InterCooperative-Network/icn/main/deploy/quickstart.sh | bash

# Follow the prompts to configure your cooperative
```

The script will:
1. Install ICN binaries
2. Create your node identity
3. Start the ICN daemon
4. Launch the web UI

## Manual Setup

If you prefer manual control, follow these steps:

### Step 1: Install ICN

```bash
# Download the latest release
curl -LO https://github.com/InterCooperative-Network/icn/releases/latest/download/icn-linux-amd64.tar.gz

# Extract binaries
tar -xzf icn-linux-amd64.tar.gz
sudo mv icnd icnctl /usr/local/bin/

# Verify installation
icnctl --version
```

### Step 2: Initialize Your Identity

```bash
# Create your node's identity
icnctl id init

# You'll be prompted for a passphrase - choose something secure!
# This protects your node's cryptographic keys
```

Save your DID (Decentralized Identifier) - it looks like `did:icn:abc123...`. You'll need this for administration.

### Step 3: Start the ICN Daemon

```bash
# Set your JWT secret (use a strong random value)
export ICN_GATEWAY_JWT_SECRET="$(openssl rand -hex 32)"

# Save this secret for later use
echo "JWT_SECRET=$ICN_GATEWAY_JWT_SECRET" >> ~/.icn/env

# Start the daemon with the web gateway enabled
icnd --gateway-enable --gateway-bind 0.0.0.0:8080
```

For production deployments, create a systemd service:

```bash
# Create service file
sudo tee /etc/systemd/system/icnd.service << 'EOF'
[Unit]
Description=ICN Daemon
After=network.target

[Service]
Type=simple
User=icn
EnvironmentFile=/home/icn/.icn/env
ExecStart=/usr/local/bin/icnd --gateway-enable --gateway-bind 0.0.0.0:8080
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
EOF

# Enable and start
sudo systemctl enable icnd
sudo systemctl start icnd
```

### Step 4: Initialize Your Cooperative

```bash
# Run the cooperative initialization wizard
icnctl init-coop

# This will prompt you for:
# - Cooperative name (e.g., "Sunrise Timebank")
# - Cooperative ID (e.g., "sunrise-timebank")
# - Initial member DIDs
```

## Onboarding Members

### For Each New Member

1. **Generate their identity** (they run this on their device):
   ```bash
   icnctl id init
   icnctl id show
   # Outputs: did:icn:xyz789...
   ```

2. **Add them to your cooperative**:
   ```bash
   # As the coordinator, run:
   icnctl gov domain add-member \
     --domain-id "your-coop-id" \
     --member "did:icn:xyz789..."
   ```

3. **Share connection info**:
   - Gateway URL: `http://your-server-ip:8080`
   - Cooperative ID: `your-coop-id`
   - They'll use their DID and generate a token

### Getting Authentication Tokens

Members need a JWT token to access the web UI:

```bash
# Member runs this to get their token
icnctl auth token \
  --gateway http://your-server-ip:8080 \
  --coop-id your-coop-id

# Enter passphrase when prompted
# Copy the displayed token
```

## Using the Web Interface

Access the web UI at `http://your-server-ip:8080`

### Login Screen

1. Enter the **Gateway URL**: `http://your-server-ip:8080`
2. Enter your **Cooperative ID**: `your-coop-id`
3. Enter your **DID**: `did:icn:abc123...`
4. Paste your **JWT Token**
5. Click **Connect**

### Dashboard Tab

The dashboard shows:
- **Your Balance**: Current hour balance (positive = hours owed to you)
- **Members**: Total cooperative members
- **Monthly Hours**: Hours exchanged this month
- **Recent Activity**: Latest transactions

### Log Hours Tab

Record service exchanges:

1. **Service Provided To**: Select the member who received your service
2. **Hours**: Enter time spent (minimum 0.25 hours / 15 minutes)
3. **Description**: Describe the service (e.g., "Garden consultation")
4. Click **Log Hours**

The recipient's balance decreases, yours increases.

### History Tab

View your complete transaction history:
- Date and time of each exchange
- Who you exchanged with
- Amount and description
- Running balance

### Members Tab

See all cooperative members:
- Member DIDs (identifiers)
- Roles (Owner, Admin, Member)

### Governance Tab

Participate in cooperative decision-making:

**Active Proposals**: Vote on current proposals
- Click **For**, **Against**, or **Abstain**
- See current vote counts

**Recent Decisions**: View closed proposals and their outcomes

## Managing Governance

### Creating Proposals

As a coordinator, create proposals for community decisions:

```bash
# Create a text proposal
icnctl gov proposal create \
  --domain-id "your-coop-id" \
  --title "Adopt new member onboarding process" \
  --description "Proposal to implement a buddy system for new members..." \
  --kind text

# Create a budget proposal
icnctl gov proposal create \
  --domain-id "your-coop-id" \
  --title "Fund community garden project" \
  --description "Allocate 100 hours to establish community garden..." \
  --kind budget
```

### Opening Proposals for Voting

```bash
# Get the proposal ID from creation output, then:
icnctl gov proposal open --proposal-id <proposal-id>
```

Members can now vote via the web UI.

### Closing Proposals

After sufficient time for voting:

```bash
icnctl gov proposal close --proposal-id <proposal-id>
```

The system calculates the outcome based on:
- **Quorum**: Minimum 51% of members must vote
- **Approval**: Majority (>50%) must vote For

## Day-to-Day Operations

### Daily Tasks

1. **Check system health**:
   ```bash
   curl http://localhost:8080/v1/health
   # Should return: {"status": "ok", ...}
   ```

2. **Monitor activity** through the web UI dashboard

### Weekly Tasks

1. **Review transaction volume** for unusual patterns
2. **Check pending proposals** and send reminders
3. **Backup your data**:
   ```bash
   icnctl backup create --output ~/backups/icn-$(date +%Y%m%d).backup
   ```

### Monthly Tasks

1. **Review member engagement** - who's active, who's not
2. **Archive old proposals**
3. **Update any system software**:
   ```bash
   sudo systemctl stop icnd
   # Download new version
   sudo systemctl start icnd
   ```

## Troubleshooting

### "Connection refused" when accessing web UI

1. Check if icnd is running:
   ```bash
   systemctl status icnd
   ```

2. Check if port 8080 is open:
   ```bash
   sudo ufw allow 8080/tcp
   ```

3. Verify gateway is enabled in startup

### "Invalid token" errors

Tokens expire after 24 hours. Generate a new one:
```bash
icnctl auth token --gateway http://localhost:8080 --coop-id your-coop-id
```

### "Unauthorized" when accessing API

Check that your token has the right scopes. Default scopes are:
- `ledger:read` - View balances and history
- `ledger:write` - Create transactions
- `coop:read` - View cooperative info
- `gov:read` - View proposals
- `gov:write` - Vote on proposals

### Members can't connect from outside

1. Ensure your server has a public IP
2. Configure firewall to allow port 8080
3. Share the public IP with members, not localhost

### Lost passphrase

**There is no recovery for a lost passphrase.** Your node's identity is protected by this passphrase. If lost:

1. Initialize a new identity: `icnctl id init`
2. Update your cooperative with the new DID
3. Members must update their records

**Recommendation**: Store your passphrase in a secure password manager.

### Transaction shows wrong balance

Mutual credit is double-entry:
- When you **provide** service: your balance **increases** (credit)
- When you **receive** service: your balance **decreases** (debit)

If Alice provides 2 hours to Bob:
- Alice: +2 hours
- Bob: -2 hours

### Proposal didn't pass despite majority

Check quorum requirements. With cooperative defaults:
- At least 51% of members must vote
- Of those who voted, majority must be For

Example: 10 members, 4 vote (3 For, 1 Against)
- Quorum: 4/10 = 40% < 51% required
- Result: **Rejected** (quorum not met)

## Security Best Practices

1. **Use strong passphrases** for node identity
2. **Keep JWT secret secure** - never commit to version control
3. **Regular backups** stored off-server
4. **Update software** when new versions release
5. **Use HTTPS** in production with a reverse proxy (nginx/caddy)

### Setting Up HTTPS (Recommended for Production)

```bash
# Install Caddy
sudo apt install caddy

# Configure reverse proxy
sudo tee /etc/caddy/Caddyfile << EOF
your-domain.org {
    reverse_proxy localhost:8080
}
EOF

# Restart Caddy (auto-provisions HTTPS cert)
sudo systemctl restart caddy
```

Members then connect via `https://your-domain.org`

## Getting Help

- **Documentation**: https://github.com/InterCooperative-Network/icn/docs
- **Issues**: https://github.com/InterCooperative-Network/icn/issues
- **Community**: Join our cooperative coordination channels

## Glossary

- **DID**: Decentralized Identifier - your unique identity (like a username)
- **JWT**: JSON Web Token - temporary access pass for the API
- **Cooperative/Coop**: Your community organization using ICN
- **Governance Domain**: The decision-making space for your cooperative
- **Proposal**: A formal request for community decision
- **Quorum**: Minimum participation required for valid vote
- **Mutual Credit**: Accounting system where credits and debits always balance

## Next Steps

Once your cooperative is running:

1. **Grow membership** - onboard 5-10 active members
2. **Establish norms** - create proposals for community agreements
3. **Connect with others** - federate with other ICN cooperatives
4. **Provide feedback** - help improve ICN for future pilots

---

*Thank you for piloting ICN! Your feedback shapes the cooperative internet.*
