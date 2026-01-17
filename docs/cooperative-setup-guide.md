# Cooperative Setup Guide

This guide provides step-by-step instructions for setting up a new cooperative on ICN.

## Prerequisites

Before setting up a cooperative, ensure you have:

1. **ICN node running** - See [Getting Started](GETTING_STARTED.md) for installation
2. **Admin identity created** - You need a DID to become the founding admin
3. **Basic understanding of cooperative model** - Familiarity with governance, trust, and mutual credit concepts

```bash
# Verify ICN is installed and running
icnctl status

# Verify you have an identity
icnctl id show
```

---

## Step 1: Create the Cooperative

### Via CLI

```bash
# Create a new cooperative
icnctl coop create \
  --id "food-coop" \
  --name "Downtown Food Cooperative"
```

### Via Gateway API

```bash
curl -X POST http://localhost:8080/v1/coops \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "id": "food-coop",
    "name": "Downtown Food Cooperative"
  }'
```

**Naming conventions**:
- `id`: lowercase, hyphens allowed, no spaces (used in API paths)
- `name`: Human-readable display name

---

## Step 2: Configure the Charter

The charter defines your cooperative's governance rules, membership criteria, and economic parameters.

### Charter Template (TOML)

Create a file named `charter.toml`:

```toml
# Cooperative Charter Configuration
# See docs/glossary.md for term definitions

[identity]
id = "food-coop"
name = "Downtown Food Cooperative"
description = "A member-owned food buying cooperative"

[governance]
# Governance model: "consensus", "majority", "delegated"
model = "majority"

# Quorum: minimum participation percentage for valid votes
quorum_percentage = 50

# Voting period in seconds (7 days default)
voting_period_secs = 604800

# Deliberation period before voting opens (3 days default)
deliberation_period_secs = 259200

[membership]
# How new members join: "open", "invitation", "approval"
join_policy = "approval"

# Minimum trust score required for membership
min_trust_score = 0.3

# Number of existing members required to vouch for new member
vouches_required = 2

# Probationary period in days (0 = none)
probation_days = 30

[economics]
# Currency name for ledger display
currency = "hours"

# Credit policy: "conservative", "moderate", "generous"
credit_policy = "moderate"

# Initial credit limit for new members
initial_credit_limit = 50

# Maximum balance allowed (0 = unlimited)
max_balance = 500

# Enable demurrage (negative interest on positive balances)
demurrage_enabled = false
demurrage_rate_annual = 0.0
```

### Apply the Charter

```bash
icnctl coop configure food-coop --charter charter.toml
```

Or via API:

```bash
curl -X PUT http://localhost:8080/v1/coops/food-coop/settings \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "governance_model": "majority",
    "credit_policy": "moderate",
    "currency": "hours"
  }'
```

---

## Step 3: Onboard Founding Members

### Create Invitation Links

Generate invitation links for founding members:

```bash
# Generate invitation (valid for 7 days)
icnctl coop invite food-coop --email alice@example.com

# Output:
# Invitation link: https://icn.coop/join/food-coop?token=abc123...
# Expires: 2026-01-24T00:00:00Z
```

### Member Self-Onboarding

New members follow these steps:

1. **Install ICN** (if not already installed)
   ```bash
   curl -fsSL https://icn.coop/install.sh | bash
   ```

2. **Create identity**
   ```bash
   icnctl id init
   ```

3. **Accept invitation**
   ```bash
   icnctl coop join --invite-token abc123...
   ```

### Admin-Initiated Onboarding

Alternatively, add members directly (requires their DID):

```bash
# Add member with 'member' role
icnctl coop add-member food-coop did:icn:5Xk8Y2r... --role member

# Add member with 'admin' role (can manage other members)
icnctl coop add-member food-coop did:icn:7Zj9K3t... --role admin
```

Via API:

```bash
curl -X POST http://localhost:8080/v1/coops/food-coop/members \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "did": "did:icn:5Xk8Y2r...",
    "role": "member"
  }'
```

---

## Step 4: Establish Initial Trust

New members start with no trust relationships. Existing members must vouch for them.

### Add Trust Attestations

```bash
# Alice vouches for Bob with trust score 0.5 (moderate trust)
icnctl trust add did:icn:bob123... 0.5

# Trust levels guide:
# 0.1 - 0.3: Acquaintance (limited transactions)
# 0.3 - 0.5: Known member (normal participation)
# 0.5 - 0.7: Trusted colleague (elevated privileges)
# 0.7 - 0.9: Close collaborator (governance roles)
```

### Verify Trust Graph

```bash
# Check a member's trust score (computed from all attestations)
icnctl trust score did:icn:bob123...

# View trust relationships
icnctl trust list --from did:icn:alice456...
```

---

## Step 5: Initialize Credit Balances

### Set Initial Credit Limits

Credit limits determine how much a member can go "negative" (receive more than they give):

```bash
# Set credit limit for a member
icnctl ledger set-limit food-coop did:icn:bob123... 100
```

### Initial Allocations (Optional)

For some cooperatives, you may want to seed initial balances:

```bash
# Grant initial balance from system account
icnctl ledger grant food-coop did:icn:bob123... 50 --memo "Founding member allocation"
```

### View Balances

```bash
# Check individual balance
icnctl ledger balance food-coop did:icn:bob123...

# List all balances in the cooperative
icnctl ledger list-balances food-coop
```

---

## Step 6: Verify Setup

Run through this checklist to ensure everything is configured correctly:

### Health Checks

```bash
# Check cooperative exists and is configured
icnctl coop show food-coop

# Verify member list
icnctl coop list-members food-coop

# Check governance is operational
icnctl governance status food-coop
```

### Test Transaction

Have two members perform a test transaction:

```bash
# Alice sends 1 hour to Bob
icnctl ledger transfer food-coop \
  --from did:icn:alice456... \
  --to did:icn:bob123... \
  --amount 1 \
  --memo "Test transaction"

# Verify balances updated
icnctl ledger balance food-coop did:icn:alice456...
icnctl ledger balance food-coop did:icn:bob123...
```

### Test Governance Vote

Create a test proposal and verify voting works:

```bash
# Create a simple proposal
icnctl governance propose food-coop \
  --title "Test Proposal" \
  --description "Verify governance is working correctly" \
  --voting-period 1h

# List proposals
icnctl governance list food-coop

# Cast vote
icnctl governance vote food-coop <proposal-id> yes
```

---

## Templates

### Member List Template (JSON)

Use this template for bulk member imports:

```json
{
  "members": [
    {
      "did": "did:icn:5Xk8Y2r...",
      "name": "Alice Smith",
      "email": "alice@example.com",
      "role": "admin",
      "initial_credit_limit": 100
    },
    {
      "did": "did:icn:7Zj9K3t...",
      "name": "Bob Jones",
      "email": "bob@example.com",
      "role": "member",
      "initial_credit_limit": 50
    }
  ]
}
```

Import with:

```bash
icnctl coop import-members food-coop members.json
```

### Minimal Charter (Quick Start)

For a minimal setup, use these defaults:

```toml
[identity]
id = "my-coop"
name = "My Cooperative"

[governance]
model = "majority"
quorum_percentage = 50

[membership]
join_policy = "approval"
vouches_required = 2

[economics]
currency = "hours"
credit_policy = "moderate"
initial_credit_limit = 50
```

### Worker Cooperative Charter

Optimized for worker-owned businesses:

```toml
[identity]
id = "tech-workers-coop"
name = "Tech Workers Cooperative"
description = "A worker-owned software development cooperative"

[governance]
model = "consensus"
quorum_percentage = 75
voting_period_secs = 604800      # 7 days
deliberation_period_secs = 259200 # 3 days

[membership]
join_policy = "approval"
min_trust_score = 0.4
vouches_required = 3
probation_days = 90

[economics]
currency = "hours"
credit_policy = "conservative"
initial_credit_limit = 40
max_balance = 200
```

---

## Troubleshooting

### "Member not found" Error

Ensure the DID exists and the member has completed identity setup:

```bash
icnctl id verify did:icn:xyz789...
```

### "Insufficient trust" Error

The member needs trust attestations from existing members:

```bash
# Check current trust score
icnctl trust score did:icn:xyz789...

# Have trusted members add attestations
icnctl trust add did:icn:xyz789... 0.5
```

### "Credit limit exceeded" Error

The member has reached their credit limit. Options:

1. Increase their limit: `icnctl ledger set-limit food-coop did:icn:xyz789... 100`
2. Have them receive payments to reduce negative balance
3. Review and adjust cooperative credit policy

### "Quorum not reached" Error

Not enough members voted on a proposal:

1. Remind members to vote before the deadline
2. Consider reducing quorum percentage in charter
3. Extend the voting period if allowed

---

## Next Steps

After completing setup:

1. **Document your charter** - Share governance rules with all members
2. **Set up backup procedures** - See [Backup and Recovery](backup-and-recovery.md)
3. **Configure monitoring** - See [Observability Guide](ARCHITECTURE.md#observability)
4. **Plan governance cadence** - Regular meetings, proposal review cycles

---

## Related Documentation

- [Getting Started](GETTING_STARTED.md) - Installation and first steps
- [Governance Primitives](governance-primitives.md) - Detailed governance documentation
- [Economic Safety](economic-safety.md) - Credit limits and financial safeguards
- [Trust Threshold Configuration](trust-threshold-configuration.md) - Trust system details
- [API Reference](API_REFERENCE.md) - Complete API documentation
- [Glossary](glossary.md) - ICN terminology definitions
