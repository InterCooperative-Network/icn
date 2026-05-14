# ICN Institution Setup Guide

How to create a real institution on ICN — from first entity to operational governance.

This guide covers the full lifecycle: deciding what kind of institution you're building, creating entities, issuing identities, ratifying a charter, and bringing the institution to life. It uses the NYCN federation as the reference implementation, but the patterns generalize to any cooperative, community, or federation.

---

## Step 0: Decide Your Institutional Shape

Before creating anything, answer three questions:

**1. What kind of institution is this?**

| If you are... | Top-level entity type | Example |
|--------------|----------------------|---------|
| A single cooperative | `Cooperative` | A worker co-op, food co-op, housing co-op |
| A community or civic organization | `Community` | A neighborhood council, mutual aid network |
| A network of cooperatives | `Federation` | A regional co-op network, sector alliance |
| A federation of federations | `Federation` (nested) | A national cooperative association |

**2. Does it have internal structure?**

| If you need... | Create child entities as... |
|---------------|---------------------------|
| Committees or working groups | `Community` entities with `parent_id` pointing to the parent |
| Departments with their own budgets | `Cooperative` entities with `parent_id` (they get their own treasury) |
| Member organizations | Independent entities joined via `FederationProfile.member_entities` |
| No internal structure | Nothing — a single entity is fine |

**3. What needs to be governed?**

Every entity with a `governance_domain_id` gets its own proposal/voting space. Only create governance domains where actual decisions happen. A committee that just coordinates doesn't need one. A committee that approves sessions or spending does.

---

## Step 1: Create the Root Entity

### Prerequisites

- A running ICN node (`icnd`)
- `icnctl` CLI installed
- At least one identity created (`icnctl id init`)

### For a Single Cooperative

```bash
# Create the cooperative
curl -X POST http://localhost:8080/coops \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "id": "my-coop",
    "name": "My Worker Cooperative",
    "entity_type": "Cooperative",
    "governance_domain_id": "my-coop-gov",
    "treasury_account": "my-coop-treasury"
  }'
```

### For a Federation

```bash
# Create the federation
curl -X POST http://localhost:8080/coops \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "id": "my-federation",
    "name": "Regional Cooperative Federation",
    "entity_type": "Federation",
    "governance_domain_id": "my-fed-gov",
    "treasury_account": "my-fed-treasury"
  }'
```

### For a Community

```bash
curl -X POST http://localhost:8080/coops \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "id": "my-community",
    "name": "Neighborhood Mutual Aid",
    "entity_type": "Community",
    "governance_domain_id": "my-community-gov"
  }'
```

Note: Communities have governance but no treasury by default. If the community manages money, use `Cooperative` instead.

---

## Step 2: Create Internal Structure

Skip this step if your institution is flat (no committees, departments, or working groups).

### Committees as Child Communities

Each committee gets its own governance domain (its own proposal/voting space):

```bash
# Create a committee
curl -X POST http://localhost:8080/coops \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "id": "my-coop-finance",
    "name": "Finance Committee",
    "entity_type": "Community",
    "parent_id": "my-coop",
    "governance_domain_id": "my-coop-finance-gov"
  }'
```

Repeat for each committee. The `parent_id` creates the hierarchical relationship.

### Operational Body Under a Federation

If the federation has a distinct operational arm (like NYCN's organizing co-op):

```bash
curl -X POST http://localhost:8080/coops \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "id": "my-fed-ops",
    "name": "Federation Operations",
    "entity_type": "Cooperative",
    "parent_id": "my-federation",
    "governance_domain_id": "my-fed-ops-gov",
    "treasury_account": "my-fed-ops-treasury"
  }'
```

Use `Cooperative` (not `Community`) because it holds a treasury and executes operations.

---

## Step 3: Issue Identities

Every person who participates needs a DID.

### For the Founding Team

Each founder runs locally:

```bash
icnctl id init
# Output: Created identity did:icn:z6MkrT...
# Keystore saved to ~/.icn/keystore.age
```

### Authentication

To get a JWT for API access:

```bash
# 1. Request challenge
CHALLENGE=$(curl -s -X POST http://localhost:8080/v1/auth/challenge \
  -H "Content-Type: application/json" \
  -d '{"did": "did:icn:z6MkrT..."}' | jq -r '.nonce')

# 2. Sign challenge with your key
SIGNATURE=$(icnctl auth sign "$CHALLENGE")

# 3. Verify and get token
TOKEN=$(curl -s -X POST http://localhost:8080/v1/auth/verify \
  -H "Content-Type: application/json" \
  -d '{
    "did": "did:icn:z6MkrT...",
    "signature": "'$SIGNATURE'",
    "coop_id": "my-coop",
    "scopes": ["propose", "vote", "treasury_access"]
  }' | jq -r '.token')
```

The `coop_id` and `scopes` determine which hat you're wearing. Same DID, different institutional context.

---

## Step 4: Add Founding Members

```bash
# Add a founder to the root entity
curl -X POST http://localhost:8080/coops/my-coop/members \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "did": "did:icn:z6MkrT...",
    "role": "founder"
  }'
```

### Role Selection

| Role | Use When | Default Capabilities |
|------|----------|---------------------|
| `founder` | Establishing the institution | All capabilities |
| `member` | Regular participant | Vote, Propose |
| `board_member` | Elected/appointed leadership | Vote, Propose, TreasuryAccess, Invite |
| `officer` (with title) | Named role (treasurer, secretary) | Vote, Propose, TreasuryAccess, Invite, Configure |
| `federated_member` | Organization joining a federation | Vote, Propose |
| `associate_member` | Organization with limited participation | Limited Vote |
| `observer_member` | Read-only access | None (observe only) |
| `custom` (with name) | Anything else | Define per-institution |

### Add Founders to Committees

Same person, different entity, different role:

```bash
# Matt as finance committee coordinator
curl -X POST http://localhost:8080/coops/my-coop-finance/members \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "did": "did:icn:z6MkrT...",
    "role": "officer",
    "title": "Coordinator"
  }'
```

---

## Step 5: Ratify a Charter

The charter is the constitutional document. It defines governance rules, thresholds, authority boundaries, and trust requirements. See [CCL Charter Templates](../reference/ccl-charter-templates.md) for starter templates.

### Submit the Charter

```bash
curl -X POST http://localhost:8080/governance/my-coop-gov/proposals \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "title": "Ratify founding charter",
    "description": "Constitutional document establishing governance rules",
    "payload": {
      "type": "Charter",
      "charter_id": "my-coop-charter-v1",
      "yaml": "... (CCL charter content) ..."
    }
  }'
```

### Vote on the Charter

All founders vote:

```bash
curl -X POST http://localhost:8080/governance/my-coop-gov/proposals/PROPOSAL_ID/vote \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{"vote": "yes"}'
```

When the charter passes, the `charter_accepted_hook` deploys it to the `CharterPolicyOracle`. From that point, governance rules are enforced by the system.

---

## Step 6: Set Up Communication

Create gossip topics for institutional communication:

### General Channel

```
Topic: "my-coop:general"
ACL: Participants([all member DIDs])
```

### Committee Channels

```
Topic: "my-coop:finance:coordination"
ACL: Participants([finance committee member DIDs])
```

### Public Channel

```
Topic: "my-coop:public"
ACL: Public
```

---

## Step 7: Set Up Budget (If Applicable)

### Submit Budget Proposal

```bash
curl -X POST http://localhost:8080/governance/my-coop-gov/proposals \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "title": "2026 Operating Budget",
    "payload": {
      "type": "Allocation",
      "pool_amount": 5000,
      "unit": "USD",
      "options": [
        {"name": "operations", "amount": 2000, "description": "General operations"},
        {"name": "programs", "amount": 2000, "description": "Program activities"},
        {"name": "reserve", "amount": 1000, "description": "Contingency"}
      ],
      "purpose": "Annual operating budget"
    }
  }'
```

When approved, allocations become ledger entries — treasury credits per category.

---

## Step 8: Invite Additional Members

Founders use the `Invite` capability to bring in more people:

```bash
# Create an invite
curl -X POST http://localhost:8080/v1/invites \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "coop_id": "my-coop",
    "role": "member",
    "max_uses": 10,
    "expires_in_hours": 168
  }'
# Returns an invite code that new members use to join
```

New members:
1. Create their identity (`icnctl id init`)
2. Join via invite code
3. Get assigned to committees as needed

---

## Step 9: Begin Operating

The institution is now live. Members can:

- **Submit proposals** in their governance domain
- **Vote** on open proposals
- **Track budget** through the ledger
- **Communicate** via gossip topics
- **Invite** new members (if they have the capability)

Every decision, vote, and financial event is recorded with full provenance. The institution has durable memory from day one.

---

## What's Next

- [CCL Charter Templates](../reference/ccl-charter-templates.md) — Starter charters for common institutional types
- [Governance Playbook](../reference/governance-playbook.md) — Common decision patterns as reusable templates
- [Entity Topology Patterns](../reference/entity-topology-patterns.md) — Organizational structures for different institution types
- [Onboarding Runbook](../guides/onboarding-runbook.md) — How to bring real humans into an ICN institution

---

## Reference: NYCN as Worked Example

The NYCN federation is the reference implementation of this guide. See:

- NYCN Institutional Strategy — Why NYCN lives on ICN
- NYCN Institutional Design — Full entity tree, governance model, and ICN primitive mapping
