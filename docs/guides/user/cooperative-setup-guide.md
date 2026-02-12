# Cooperative Setup Guide

This guide provides step-by-step instructions for setting up a new cooperative on ICN.

## Prerequisites

Before setting up a cooperative, ensure you have:

1. **ICN node running** - See [Getting Started](GETTING_STARTED.md) for installation
2. **Admin identity created** - You need a DID to become the founding admin
3. **Gateway running** - For REST API access (member onboarding, payments)

```bash
# Verify ICN is installed and running
icnctl status

# Verify you have an identity
icnctl id show
```

---

## Step 1: Create the Cooperative

### Option A: Guided Setup Wizard (Recommended)

Use the interactive wizard for a streamlined experience:

```bash
# Start the guided setup wizard
icnctl init-coop --name "Downtown Food Cooperative"

# With pre-specified founding members
icnctl init-coop --name "Downtown Food Cooperative" \
  --members "did:icn:alice123,did:icn:bob456"

# Non-interactive mode (skip confirmation prompts)
icnctl init-coop --name "Downtown Food Cooperative" --yes
```

The wizard will:
1. Generate a cooperative ID from the name
2. Create initial governance domain
3. Set up the mutual credit ledger
4. Configure founding members

### Option B: Gateway API

For programmatic setup, use the Gateway REST API:

```bash
# Get JWT token for API access
export TOKEN=$(icnctl auth token --gateway http://localhost:8080 --coop-id food-coop)

# Create cooperative via API
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

## Step 2: Create and Ratify the Charter

The charter is your cooperative's founding document, defining governance rules and membership criteria.

### Create a Charter

```bash
# Create a charter for the cooperative
icnctl charter create \
  --name "Downtown Food Cooperative Charter" \
  --domain "coop:food-coop" \
  --org-type cooperative \
  --mission "Provide affordable local food to community members" \
  --founders "did:icn:alice123,did:icn:bob456" \
  --output food-coop-charter.json
```

> **Note**: Governance rules (quorum, voting periods) are configured in the charter JSON template, not via CLI flags.

### Sign and Ratify

Founding members must sign the charter before it becomes active:

```bash
# Each founder signs the charter
icnctl charter sign food-coop-charter --coop-id food-coop

# Once enough signatures are collected, ratify the charter
icnctl charter ratify food-coop-charter --coop-id food-coop
```

### View Charter Status

```bash
# Show charter details and signature status
icnctl charter show food-coop-charter

# List all charters
icnctl charter list
```

---

## Step 3: Set Up Governance Domain

Create a governance domain for proposals and voting:

```bash
# Create the governance domain
icnctl gov domain create \
  --domain-id "coop:food-coop" \
  --name "Food Coop Governance"
```

---

## Step 4: Onboard Members

### Add Members via Gateway API

```bash
# Get JWT token for admin operations
export TOKEN=$(icnctl auth token --gateway http://localhost:8080 --coop-id food-coop)

# Add a member via REST API
curl -X POST http://localhost:8080/v1/coops/food-coop/members \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "did": "did:icn:5Xk8Y2r...",
    "role": "member"
  }'
```

### Member Roles

| Role | Permissions |
|------|------------|
| `member` | Vote, transact, create proposals |
| `admin` | Manage members, configure settings |
| `owner` | Full control, including deletion |

---

## Step 5: Establish Initial Trust

New members start with no trust relationships. Existing members must vouch for them.

### Add Trust Attestations

```bash
# Alice vouches for Bob with trust score 0.5 (moderate trust)
icnctl trust add did:icn:bob123 0.5

# Trust levels guide:
# 0.1 - 0.3: Acquaintance (limited transactions)
# 0.3 - 0.5: Known member (normal participation)
# 0.5 - 0.7: Trusted colleague (elevated privileges)
# 0.7 - 0.9: Close collaborator (governance roles)
```

### Verify Trust

```bash
# Check a member's computed trust score
icnctl trust show did:icn:bob123

# View your trust attestations
icnctl trust list
```

---

## Step 6: Verify Setup

Run through this checklist to ensure everything is configured correctly:

### Health Checks

```bash
# Check daemon status
icnctl status

# Verify charter is ratified
icnctl charter show food-coop-charter

# View ledger state
icnctl ledger head
```

### Test Ledger Operations

```bash
# Check a specific account balance (use your DID)
icnctl ledger balance did:icn:alice123

# With currency filter
icnctl ledger balance did:icn:alice123 --currency hours

# View recent transactions
icnctl ledger history --limit 10
```

### Make a Payment (via Gateway API)

```bash
curl -X POST http://localhost:8080/v1/ledger/food-coop/payment \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "from": "did:icn:alice456...",
    "to": "did:icn:bob123...",
    "amount": 10,
    "currency": "hours",
    "memo": "Test payment"
  }'
```

### Test Governance Vote

Create a test proposal and verify voting works:

```bash
# Create a test proposal
icnctl gov proposal create \
  --domain "coop:food-coop" \
  --title "Test Proposal" \
  --description "Verify governance is working correctly" \
  --voting-period 1h

# List proposals
icnctl gov proposal list --domain "coop:food-coop"

# Cast a vote
icnctl gov vote cast <proposal-id> yes
```

---

## Charter Templates

### Worker Cooperative Charter

Optimized for worker-owned businesses:

```json
{
  "name": "Tech Workers Cooperative Charter",
  "organization_id": "tech-workers",
  "governance": {
    "model": "consensus",
    "quorum_percentage": 75,
    "voting_period_days": 7,
    "deliberation_period_days": 3
  },
  "membership": {
    "join_policy": "approval",
    "vouches_required": 3,
    "probation_days": 90
  },
  "economics": {
    "currency": "hours",
    "initial_credit_limit": 40
  }
}
```

### Consumer Cooperative Charter

For buying cooperatives:

```json
{
  "name": "Food Buying Cooperative Charter",
  "organization_id": "food-coop",
  "governance": {
    "model": "majority",
    "quorum_percentage": 50,
    "voting_period_days": 5
  },
  "membership": {
    "join_policy": "approval",
    "vouches_required": 2,
    "probation_days": 30
  },
  "economics": {
    "currency": "credits",
    "initial_credit_limit": 100
  }
}
```

---

## Troubleshooting

### "Insufficient trust" Error

The member needs trust attestations from existing members:

```bash
# Check current trust score
icnctl trust show did:icn:xyz789

# Have trusted members add attestations
icnctl trust add did:icn:xyz789 0.5
```

### "Charter not ratified" Error

Ensure enough founding members have signed:

```bash
# Check signature status
icnctl charter show food-coop-charter

# Have missing founders sign
icnctl charter sign food-coop-charter --coop-id food-coop
```

### "Domain not found" Error

Create the governance domain first:

```bash
icnctl gov domain create --domain-id "coop:food-coop"
```

### "Unauthorized" API Error

Ensure you have a valid JWT token:

```bash
# Get a fresh token
export TOKEN=$(icnctl auth token --gateway http://localhost:8080 --coop-id food-coop)

# Verify token is valid by making an API call
curl -H "Authorization: Bearer $TOKEN" http://localhost:8080/v1/health
```

---

## Next Steps

After completing setup:

1. **Invite members** - Share the cooperative ID and onboarding instructions
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
