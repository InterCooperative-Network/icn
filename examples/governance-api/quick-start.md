# Governance API Quick Start

Copy-paste these curl commands to try out the governance API.

## 1. Start the Gateway

```bash
cd /home/matt/projects/icn/icn
cargo run --bin icn-gateway -- --bind 127.0.0.1:8080 --jwt-secret test-secret-key
```

## 2. Authenticate (Get JWT Token)

```bash
# Get challenge
curl -X POST http://localhost:8080/v1/auth/challenge \
  -H "Content-Type: application/json" \
  -d '{"did": "did:icn:alice"}' | jq

# Verify (returns JWT token)
# Note: In production, sign the challenge with Ed25519 key
curl -X POST http://localhost:8080/v1/auth/verify \
  -H "Content-Type: application/json" \
  -d '{
    "did": "did:icn:alice",
    "challenge": "CHALLENGE_FROM_ABOVE",
    "signature": "SIGN_WITH_PRIVATE_KEY",
    "coop_id": "test-coop",
    "scopes": ["gov:read", "gov:write"]
  }' | jq

# Save token for next commands
export TOKEN="eyJ0eXAi..."
```

## 3. Create a Governance Domain

```bash
curl -X POST http://localhost:8080/v1/gov/domains \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "id": "coop:food",
    "name": "Food Cooperative",
    "profile": "cooperative",
    "quorum_percent": 50,
    "approval_percent": 66,
    "voting_period_days": 7,
    "members": ["did:icn:alice", "did:icn:bob", "did:icn:carol"]
  }' | jq
```

## 4. List Domains

```bash
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/v1/gov/domains | jq
```

## 5. Create a Proposal

```bash
curl -X POST http://localhost:8080/v1/gov/proposals \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "domain_id": "coop:food",
    "title": "Approve new supplier",
    "description": "Partner with Local Farms Inc for organic produce",
    "payload": {
      "Text": {
        "body": "2-year contract, 15% discount, weekly delivery"
      }
    }
  }' | jq

# Save proposal ID for next commands
export PROPOSAL_ID="prop-abc123..."
```

## 6. List Proposals

```bash
# All proposals
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/v1/gov/proposals | jq

# Filter by domain
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8080/v1/gov/proposals?domain_id=coop:food" | jq

# Filter by state
curl -H "Authorization: Bearer $TOKEN" \
  "http://localhost:8080/v1/gov/proposals?state=draft" | jq
```

## 7. Open Proposal for Voting

```bash
curl -X POST http://localhost:8080/v1/gov/proposals/$PROPOSAL_ID/open \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "voting_period_seconds": 86400
  }' | jq
```

## 8. Cast a Vote

```bash
curl -X POST http://localhost:8080/v1/gov/proposals/$PROPOSAL_ID/vote \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "choice": "for",
    "comment": "Great deal, I support this!"
  }' | jq
```

## 9. Close Proposal

```bash
curl -X POST http://localhost:8080/v1/gov/proposals/$PROPOSAL_ID/close \
  -H "Authorization: Bearer $TOKEN" | jq
```

## 10. Get Proposal Details

```bash
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/v1/gov/proposals/$PROPOSAL_ID | jq
```

## Example: Budget Proposal

```bash
curl -X POST http://localhost:8080/v1/gov/proposals \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "domain_id": "coop:food",
    "title": "Purchase new delivery van",
    "description": "Our current van needs replacement",
    "payload": {
      "Budget": {
        "amount": 35000,
        "recipient": "did:icn:vehicle-dealer",
        "currency": "USD",
        "purpose": "Delivery vehicle replacement"
      }
    }
  }' | jq
```

## Example: Membership Proposal

```bash
curl -X POST http://localhost:8080/v1/gov/proposals \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "domain_id": "coop:food",
    "title": "Add new member",
    "description": "Invite Jane Doe to join as full member",
    "payload": {
      "Membership": {
        "action": "add",
        "did": "did:icn:jane-doe"
      }
    }
  }' | jq
```

## WebSocket: Real-Time Events

```bash
# Install wscat
npm install -g wscat

# Connect to WebSocket
wscat -c ws://localhost:8080/v1/ws/coop:food

# Authenticate
> {"type": "Auth", "token": "YOUR_JWT_TOKEN"}

# Receive events automatically
< {"type":"Event","GovernanceProposalCreated":{...}}
< {"type":"Event","GovernanceVoteCast":{...}}
< {"type":"Event","GovernanceProposalClosed":{...}}
```

## Troubleshooting

**401 Unauthorized**: Token missing or invalid
```bash
# Get a fresh token
curl -X POST http://localhost:8080/v1/auth/verify ...
```

**403 Forbidden**: Missing required scope
```bash
# Ensure scopes include "gov:read" or "gov:write"
"scopes": ["gov:read", "gov:write"]
```

**404 Not Found**: Resource doesn't exist
```bash
# Check domain/proposal ID is correct
curl -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/v1/gov/domains | jq
```

**429 Too Many Requests**: Rate limit exceeded
```bash
# Wait a few seconds or check rate limit status
# Limit: 100 burst, 10/sec refill per DID
```

## Next Steps

- Run the [full workflow script](./full-workflow.sh) for a complete example
- Read the [API documentation](./README.md) for detailed endpoint reference
- Explore [governance primitives design](../../docs/governance-primitives.md)
