# ICN Gateway Governance API Examples

This directory contains examples demonstrating the ICN Gateway's governance API endpoints.

## Prerequisites

1. **Running ICN Gateway**:
   ```bash
   cd /home/matt/projects/icn/icn
   cargo run --bin icn-gateway -- --bind 127.0.0.1:8080 --jwt-secret test-secret-key
   ```

2. **Required Tools**:
   - `curl` - HTTP client
   - `jq` - JSON processor (install: `sudo apt install jq`)

## Examples

### Full Workflow (`full-workflow.sh`)

Demonstrates a complete governance workflow from start to finish:

1. **Authentication** - Get JWT tokens for 3 test users (Alice, Bob, Carol)
2. **Domain Creation** - Create "Food Cooperative Governance" domain
3. **Proposal Creation** - Create text proposal to approve a new supplier
4. **Open Voting** - Open the proposal for a 24-hour voting period
5. **Cast Votes** - Three members vote (2 FOR, 1 AGAINST)
6. **Close Proposal** - Close voting and calculate outcome (ACCEPTED)
7. **Query Proposals** - List proposals by domain and filter by state

**Usage**:
```bash
chmod +x full-workflow.sh
./full-workflow.sh
```

**Expected Output**:
```
==> Step 1: Authenticating three test users (Alice, Bob, Carol)...
✓ Alice authenticated
✓ Bob authenticated
✓ Carol authenticated

==> Step 2: Creating governance domain 'coop:food'...
✓ Domain created: Food Cooperative Governance

==> Step 3: Creating a proposal to approve a new supplier...
✓ Proposal created: prop-abc123...

==> Step 4: Opening proposal for voting (24 hour period)...
✓ Proposal opened for voting

==> Step 5: Casting votes from three members...
✓ Alice voted FOR
✓ Bob voted FOR
✓ Carol voted AGAINST

==> Step 6: Closing the proposal and calculating outcome...
✓ Proposal closed

==> Step 7: Final Outcome
==================================

Proposal: Approve Local Farms Inc as new supplier
ID: prop-abc123...

✓ OUTCOME: ACCEPTED
The proposal has been approved by the members.
```

## API Endpoints Reference

### Domain Management

| Endpoint | Method | Description | Auth Scope |
|----------|--------|-------------|------------|
| `/v1/gov/domains` | POST | Create new governance domain | `gov:write` |
| `/v1/gov/domains` | GET | List all domains | `gov:read` |
| `/v1/gov/domains/{id}` | GET | Get specific domain | `gov:read` |

### Proposal Management

| Endpoint | Method | Description | Auth Scope |
|----------|--------|-------------|------------|
| `/v1/gov/proposals` | POST | Create new proposal | `gov:write` |
| `/v1/gov/proposals` | GET | List proposals (filter by domain, state) | `gov:read` |
| `/v1/gov/proposals/{id}` | GET | Get specific proposal | `gov:read` |
| `/v1/gov/proposals/{id}/open` | POST | Open proposal for voting | `gov:write` |
| `/v1/gov/proposals/{id}/close` | POST | Close voting and finalize | `gov:write` |
| `/v1/gov/proposals/{id}/vote` | POST | Cast vote on proposal | `gov:write` |

### Query Parameters

**List Proposals** (`GET /v1/gov/proposals`):
- `domain_id` - Filter by governance domain
- `state` - Filter by state (`draft`, `open`, `closed`)

Example:
```bash
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8080/v1/gov/proposals?domain_id=coop:food&state=open"
```

## Proposal Payload Types

### 1. Text Proposal
Simple text-based proposal for general decisions:
```json
{
  "payload": {
    "Text": {
      "body": "Detailed proposal text..."
    }
  }
}
```

### 2. Budget Proposal
Financial allocation proposal:
```json
{
  "payload": {
    "Budget": {
      "amount": 5000,
      "recipient": "did:icn:supplier123",
      "currency": "USD",
      "purpose": "Equipment purchase"
    }
  }
}
```

### 3. Membership Proposal
Add or remove domain members:
```json
{
  "payload": {
    "Membership": {
      "action": "add",
      "did": "did:icn:newmember456"
    }
  }
}
```

### 4. Config Change Proposal
Modify domain configuration:
```json
{
  "payload": {
    "ConfigChange": {
      "key": "quorum_percent",
      "value": "60"
    }
  }
}
```

## WebSocket Real-Time Events

Subscribe to governance events via WebSocket:

```bash
# Connect to WebSocket (use wscat or similar)
wscat -c ws://localhost:8080/v1/ws/coop:food

# Authenticate
> {"type": "Auth", "token": "eyJ0eXAi..."}

# Receive events automatically
< {"type":"Event","GovernanceDomainCreated":{"domain_id":"coop:food","name":"Food Coop","creator":"did:icn:alice"}}
< {"type":"Event","GovernanceProposalCreated":{"proposal_id":"prop-123","domain_id":"coop:food",...}}
< {"type":"Event","GovernanceVoteCast":{"proposal_id":"prop-123","voter":"did:icn:bob","choice":"for"}}
< {"type":"Event","GovernanceProposalClosed":{"proposal_id":"prop-123","outcome":"accepted"}}
```

**Event Types**:
- `GovernanceDomainCreated` - New domain created
- `GovernanceProposalCreated` - New proposal in domain
- `GovernanceProposalOpened` - Voting period started
- `GovernanceProposalClosed` - Voting ended with outcome
- `GovernanceVoteCast` - Member voted on proposal

## Authentication Flow

1. **Get Challenge**:
   ```bash
   curl -X POST http://localhost:8080/v1/auth/challenge \
     -H "Content-Type: application/json" \
     -d '{"did": "did:icn:alice123"}'
   ```

2. **Sign Challenge** (with Ed25519 keypair):
   ```rust
   let signature = keypair.sign(challenge.as_bytes());
   ```

3. **Verify and Get Token**:
   ```bash
   curl -X POST http://localhost:8080/v1/auth/verify \
     -H "Content-Type: application/json" \
     -d '{
       "did": "did:icn:alice123",
       "challenge": "...",
       "signature": "...",
       "coop_id": "food-coop",
       "scopes": ["gov:read", "gov:write"]
     }'
   ```

4. **Use Token** in Authorization header:
   ```bash
   curl -H "Authorization: Bearer eyJ0eXAi..." \
     http://localhost:8080/v1/gov/domains
   ```

## Advanced Examples

### Create Budget Proposal
```bash
curl -X POST http://localhost:8080/v1/gov/proposals \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "domain_id": "coop:food",
    "title": "Allocate funds for solar panels",
    "description": "Install 50kW solar array on warehouse roof",
    "payload": {
      "Budget": {
        "amount": 75000,
        "recipient": "did:icn:solar-installer-co",
        "currency": "USD",
        "purpose": "Renewable energy infrastructure"
      }
    }
  }'
```

### Create Membership Proposal
```bash
curl -X POST http://localhost:8080/v1/gov/proposals \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "domain_id": "coop:food",
    "title": "Add new member: Jane Doe",
    "description": "Jane has been a volunteer for 6 months",
    "payload": {
      "Membership": {
        "action": "add",
        "did": "did:icn:jane-doe"
      }
    }
  }'
```

### Filter Proposals
```bash
# Get all open proposals in a domain
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8080/v1/gov/proposals?domain_id=coop:food&state=open"

# Get all draft proposals
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8080/v1/gov/proposals?state=draft"

# Get all closed proposals
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8080/v1/gov/proposals?state=closed"
```

## Error Handling

The API returns standard HTTP status codes:

- **200 OK** - Success
- **201 Created** - Resource created successfully
- **400 Bad Request** - Invalid input (check error message)
- **401 Unauthorized** - Missing or invalid JWT token
- **403 Forbidden** - Missing required scope (e.g., `gov:write`)
- **404 Not Found** - Resource doesn't exist
- **429 Too Many Requests** - Rate limit exceeded (100 burst, 10/sec refill)
- **500 Internal Server Error** - Server error (check logs)

Example error response:
```json
{
  "error": "Missing required scope: gov:write"
}
```

## Security Notes

- **JWT Expiry**: Tokens expire after 24 hours by default
- **Rate Limiting**: 100 request burst, 10/sec refill per DID
- **Scopes**:
  - `gov:read` - View domains and proposals
  - `gov:write` - Create domains, proposals, and vote
- **Domain IDs**: Max 128 chars, alphanumeric + `-_:`
- **Domain Names**: Max 256 chars

## Troubleshooting

**Gateway not running**:
```bash
# Start the gateway
cd /home/matt/projects/icn/icn
cargo run --bin icn-gateway -- --bind 127.0.0.1:8080 --jwt-secret test-secret-key
```

**Authentication fails**:
- Ensure you're using valid Ed25519 signatures
- For testing, you may need to modify the gateway to accept mock signatures
- Check that JWT_SECRET matches between client and server

**403 Forbidden**:
- Verify token includes required scopes (`gov:read` or `gov:write`)
- Check token hasn't expired

**jq command not found**:
```bash
sudo apt install jq
```

## Related Documentation

- [Governance Primitives Design](../../docs/governance-primitives.md)
- [Gateway API Design](../../docs/platform-layer-design.md)
- [CHANGELOG](../../CHANGELOG.md) - Recent changes and features

## Contributing

Found a bug or have a suggestion? Please open an issue at:
https://github.com/anthropics/icn/issues
