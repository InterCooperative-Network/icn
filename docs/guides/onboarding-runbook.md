# Onboarding Runbook

How to bring real humans into an ICN institution. This covers the full path from "I got an invite" to "I'm a voting member of a committee."

The core challenge: ICN uses DID-based identity (no email/password), which is stronger than traditional auth but unfamiliar. This runbook defines the onboarding flow so that organizers can join without needing to understand cryptography.

---

## Audience Tiers

Different people need different onboarding depths:

| Tier | Who | What They Need | Onboarding Effort |
|------|-----|---------------|-------------------|
| **Core organizer** | Committee leads, steering members | Full identity, multiple committee memberships, governance participation | 15 minutes guided setup |
| **Active volunteer** | Regular contributors, working group members | Identity, one or two committee memberships | 10 minutes guided setup |
| **Event participant** | Summit attendees, one-time contributors | Optional identity, event access | 5 minutes or skip |
| **Member organization** | Cooperatives joining the federation | Organizational entity, delegate assignment | 30 minutes with admin support |

Start with core organizers. Expand outward as the process proves reliable.

---

## Flow 1: Individual Onboarding (Organizer / Volunteer)

### What the person experiences

1. **Receive invite link** — an organizer sends them a URL or code
2. **Open the pilot UI** — browser-based, no install required
3. **Create identity** — the UI generates a DID keypair in-browser
4. **Save recovery phrase** — 12-word BIP39 seed phrase, written down or saved securely
5. **Join the institution** — invite code links them to the cooperative/committee
6. **Get assigned a role** — admin confirms their role and capabilities
7. **Start participating** — they can now view proposals, vote, communicate

### What happens technically

```
Person clicks invite link
  → Pilot UI loads at /join?invite=INVITE_CODE
  → UI calls POST /v1/auth/create-identity (generates Ed25519 keypair in browser)
  → UI displays DID and recovery phrase
  → Person confirms they've saved the recovery phrase
  → UI calls POST /v1/invites/join with invite code + new DID
  → Server validates invite, adds person to entity with specified role
  → UI calls POST /v1/auth/challenge + POST /v1/auth/verify to get JWT
  → Person is now authenticated and can use the system
```

### Admin steps (before the person arrives)

```bash
# Create an invite for the organizing co-op
curl -X POST http://localhost:8080/v1/invites \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "coop_id": "nycn-organizers",
    "role": "member",
    "max_uses": 1,
    "expires_in_hours": 168
  }'
# Returns: { "invite_code": "abc123...", "invite_url": "https://..." }
```

### Post-join admin steps

After the person joins, assign them to committees:

```bash
# Add to marketing committee
curl -X POST http://localhost:8080/coops/nycn-marketing/members \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "did": "did:icn:z6Mk...",
    "role": "member"
  }'
```

---

## Flow 2: Organization Onboarding (Member Co-op Joining Federation)

### What the organization does

1. **Admin creates the org entity** — or the org creates it themselves if they're running their own node
2. **Org designates a delegate** — a person who represents the org in the federation
3. **Delegate gets their own identity** — same as individual onboarding
4. **Submit membership application** — delegate submits request to federation
5. **Federation reviews and votes** — governance proposal, per charter thresholds
6. **On approval** — org entity added to federation's `member_entities`

### Admin steps

```bash
# Create the organization entity (if they don't have their own node)
curl -X POST http://localhost:8080/coops \
  -H "Authorization: Bearer $ADMIN_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "id": "greenstar",
    "name": "GreenStar Food Co-op",
    "entity_type": "Cooperative"
  }'

# The org's delegate goes through individual onboarding (Flow 1)
# Then submit membership proposal to federation governance
curl -X POST http://localhost:8080/governance/nycn-federation-gov/proposals \
  -H "Authorization: Bearer $DELEGATE_TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "title": "Admit GreenStar Food Co-op to NYCN federation",
    "description": "GreenStar is a consumer food co-op in Ithaca, NY with 12,000 members. Silver-level summit sponsor since 2024.",
    "payload": {
      "type": "Membership",
      "action": "Add",
      "member": "did:icn:z6Mk..."
    }
  }'

# Federation members vote on the proposal
```

---

## Flow 3: Bulk Onboarding (Migrating Existing Members)

When an institution like NYCN migrates from a traditional database (PostgreSQL, Google Sheets) to ICN, there's a bulk onboarding challenge: 702 people and 236 organizations already exist in the database.

### Strategy: Tiered Migration

**Tier 1 (immediate)**: Core organizers (~15 people)
- Individual onboarding (Flow 1) with guided support
- Assign to committees, set up governance domains

**Tier 2 (first month)**: Active volunteers (~50 people)
- Batch invite links per committee
- Self-service onboarding via pilot UI

**Tier 3 (ongoing)**: Member organizations (~236 orgs)
- Create entity records for organizations that confirm participation
- Delegate assignment per org
- Federation membership via governance proposal

**Tier 4 (optional)**: Event participants (~700 people)
- Optional identity creation at registration
- Can participate without DID (external attendee records)

### Data Migration

Existing records (people, organizations, sponsorships, sessions) can be imported as ICN entities with metadata linking back to the source database:

```bash
# For each existing organization:
# 1. Create ICN entity
# 2. Store legacy ID in metadata: { "legacy_db_id": "236", "source": "nycn-postgres" }
# 3. If they have a contact person, that person becomes the initial member
```

**Important**: Don't create DIDs for people who haven't consented. Create entities for organizations (public records) and let individuals opt in to identity creation.

---

## Recovery

### Lost Device / Forgotten Password

1. Person uses their 12-word recovery phrase
2. Recovery phrase regenerates the keypair
3. Person authenticates with restored identity

### Lost Recovery Phrase

1. If multi-device is configured: use another device to recover
2. If social recovery is configured: guardians approve recovery request
3. Last resort: admin creates a new identity, migrates memberships manually

### Key Rotation

When a person suspects their key is compromised:

```bash
icnctl id rotate
# Generates new keypair, updates DID document
# Old key is revoked, new key takes effect
# All existing memberships transfer automatically
```

---

## Common Problems

| Problem | Solution |
|---------|----------|
| "I can't log in" | Check if they saved their recovery phrase. If using browser, check if local storage was cleared. |
| "I can't see my committees" | Verify they're authenticated with the right `coop_id`. They may need to re-auth with a different scope. |
| "I can't vote" | Check their role — `member` has `Vote` capability, but `observer_member` does not. Admin may need to upgrade their role. |
| "The invite link expired" | Create a new invite. Default expiry is 7 days. |
| "I want to join a second committee" | Admin adds their DID to the second committee. Same person, new membership record. |
| "Our organization wants to join" | Follow Flow 2 (Organization Onboarding). Requires federation governance vote. |

---

## Onboarding Checklist

### For the Admin

- [ ] Institution entities created (federation, co-op, committees)
- [ ] Charter ratified
- [ ] Invite codes generated for each onboarding tier
- [ ] Gossip topics created for committee communication
- [ ] At least 3 founding members onboarded (to reach quorum for governance)

### For Each New Member

- [ ] Identity created (DID generated)
- [ ] Recovery phrase saved securely
- [ ] Joined institution via invite code
- [ ] Role assigned by admin
- [ ] Added to relevant committees
- [ ] Can authenticate and access governance dashboard
- [ ] Has verified they can view proposals and cast votes

---

## See Also

- [Institution Setup Guide](institution-setup.md) — Creating the institution before onboarding anyone
- [CCL Charter Templates](../reference/ccl-charter-templates.md) — Governance rules that affect who can do what
- [Entity Topology Patterns](../reference/entity-topology-patterns.md) — Understanding the structure people are joining
