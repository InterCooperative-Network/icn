# Workshop 7: Gateway Auth and SDK Usage

## Goal
Authenticate against the ICN gateway API, understand the challenge-response flow,
and use the TypeScript SDK to interact with the ledger.

## Prerequisites
- Completed Module 7 reading
- ICN daemon running (`icnd`)
- Node.js 18+ installed (for SDK exercises)

## Estimated time
2-3 hours

## Part 1: Gateway Architecture

### Steps
1. Open `icn/crates/icn-gateway/src/lib.rs`
2. Find the route definitions
3. Identify the authentication middleware

### Gateway structure
```
┌─────────────────────────────────────────┐
│             ICN Gateway                  │
├─────────────────────────────────────────┤
│  /auth/challenge  │  Get auth challenge  │
│  /auth/verify     │  Submit signed proof │
├─────────────────────────────────────────┤
│  /ledger/*        │  Ledger operations   │
│  /trust/*         │  Trust queries       │
│  /governance/*    │  Voting/proposals    │
│  /compute/*       │  Task submission     │
├─────────────────────────────────────────┤
│  /ws              │  WebSocket events    │
│  /health          │  Health check        │
│  /metrics         │  Prometheus metrics  │
└─────────────────────────────────────────┘
```

### Questions to answer
1. What web framework is used (Actix, Axum, etc.)?
2. How are routes grouped by functionality?
3. Which routes require authentication?

### Checkpoint
- [ ] You understand gateway route structure
- [ ] You can identify protected vs public routes

## Part 2: Challenge-Response Authentication

### Steps
1. Open `icn/crates/icn-gateway/src/auth/` (or search for "challenge")
2. Find the challenge generation code
3. Trace the verification flow

### Auth flow diagram
```
Client                           Gateway
   │                                │
   │──── GET /auth/challenge ──────►│
   │                                │ Generate random nonce
   │◄─── { challenge, expires } ────│
   │                                │
   │ Sign challenge with DID key    │
   │                                │
   │── POST /auth/verify ──────────►│
   │    { did, signature }          │ Verify signature
   │                                │ Generate JWT
   │◄─── { token, expires } ────────│
   │                                │
   │ Include token in future requests
   │                                │
   │── GET /ledger/balance ────────►│
   │    Authorization: Bearer <jwt> │ Validate JWT
   │◄─── { balance: 100 } ──────────│
```

### Questions to answer
1. What is included in the challenge?
2. How long is the challenge valid?
3. What claims are in the JWT?

### Code to find
```rust
pub struct AuthChallenge {
    pub challenge: String,  // Random nonce
    pub expires_at: u64,    // Unix timestamp
}

pub struct VerifyRequest {
    pub did: Did,
    pub signature: Signature,
}
```

### Checkpoint
- [ ] You understand the challenge-response flow
- [ ] You can trace JWT generation

## Part 3: Testing Auth with curl

### Steps
1. Start the daemon if not running:
   ```bash
   export ICN_DATA=$(mktemp -d)
   export ICN_PASSPHRASE="workshop"
   ./target/debug/icnctl --data-dir "$ICN_DATA" id init
   ./target/debug/icnd --data-dir "$ICN_DATA" &
   ```

2. Get a challenge:
   ```bash
   curl -s http://localhost:8080/auth/challenge | jq
   ```

3. Expected response:
   ```json
   {
     "challenge": "abc123...",
     "expires_at": 1700000000
   }
   ```

### Manual signing (conceptual)
To complete auth, you would:
1. Sign the challenge with your DID private key
2. POST to `/auth/verify` with DID and signature
3. Receive JWT token

Note: The SDK handles this automatically.

### Checkpoint
- [ ] You obtained a challenge from the gateway
- [ ] You understand what happens next

## Part 4: TypeScript SDK Setup

### Steps
1. Check for SDK location:
   ```bash
   ls sdk/typescript/ 2>/dev/null || echo "SDK not found"
   ```

2. If SDK exists, install dependencies:
   ```bash
   cd sdk/typescript
   npm install
   ```

3. Review the SDK README for usage examples

### SDK structure (expected)
```
sdk/typescript/
├── src/
│   ├── client.ts      # Main ICN client
│   ├── auth.ts        # Authentication helpers
│   ├── ledger.ts      # Ledger operations
│   ├── trust.ts       # Trust queries
│   └── types.ts       # TypeScript types
├── package.json
└── README.md
```

### Checkpoint
- [ ] You located the TypeScript SDK
- [ ] Dependencies installed successfully

## Part 5: SDK Authentication

### Steps
1. Create a test script `test-auth.ts`:
   ```typescript
   import { IcnClient } from '@icn/sdk';

   async function main() {
     // Create client
     const client = new IcnClient({
       endpoint: 'http://localhost:8080',
     });

     // Load identity (from keystore or generate)
     const identity = await client.loadIdentity({
       keystorePath: process.env.ICN_DATA + '/keystore.age',
       passphrase: process.env.ICN_PASSPHRASE,
     });

     console.log('Loaded DID:', identity.did);

     // Authenticate
     const token = await client.authenticate();
     console.log('Got JWT token:', token.substring(0, 20) + '...');

     // Now client is authenticated for protected endpoints
   }

   main().catch(console.error);
   ```

2. Run the script:
   ```bash
   npx ts-node test-auth.ts
   ```

### Questions to answer
1. How does the SDK sign the challenge?
2. How is the token stored for subsequent requests?
3. What happens when the token expires?

### Checkpoint
- [ ] You authenticated via the SDK
- [ ] You understand token lifecycle

## Part 6: Ledger Operations via SDK

### Steps
1. Create `test-ledger.ts`:
   ```typescript
   import { IcnClient } from '@icn/sdk';

   async function main() {
     const client = new IcnClient({
       endpoint: 'http://localhost:8080',
     });

     await client.loadIdentity({ /* ... */ });
     await client.authenticate();

     // Get balance
     const balance = await client.ledger.getBalance();
     console.log('Current balance:', balance);

     // Transfer (if you have funds)
     // const result = await client.ledger.transfer({
     //   to: 'did:icn:recipient...',
     //   amount: 10,
     //   memo: 'test transfer'
     // });
     // console.log('Transfer result:', result);
   }

   main().catch(console.error);
   ```

2. Run and observe the output

### Expected API calls
```
GET  /ledger/balance?did=<your-did>
POST /ledger/transfer { to, amount, memo }
GET  /ledger/history?did=<your-did>&limit=10
```

### Questions to answer
1. How are ledger entries returned (format)?
2. How does the SDK handle errors?
3. Can you query another DID's balance?

### Checkpoint
- [ ] You queried your balance
- [ ] You understand the ledger API

## Part 7: Trust Queries via SDK

### Steps
1. Extend your script to query trust:
   ```typescript
   // Get trust score between two DIDs
   const trust = await client.trust.getScore({
     from: myDid,
     to: 'did:icn:somepeer...'
   });
   console.log('Trust score:', trust);

   // Add a trust edge (requires auth)
   await client.trust.addEdge({
     to: 'did:icn:somepeer...',
     score: 0.5,
     dimension: 'social'
   });
   ```

### Trust API endpoints
```
GET  /trust/score?from=<did>&to=<did>
GET  /trust/edges?did=<did>
POST /trust/edge { to, score, dimension }
```

### Questions to answer
1. What trust dimensions are available?
2. Can you remove a trust edge?
3. How is transitive trust computed (client vs server)?

### Checkpoint
- [ ] You queried trust scores
- [ ] You understand trust API operations

## Part 8: WebSocket Events

### Steps
1. Create `test-websocket.ts`:
   ```typescript
   import { IcnClient } from '@icn/sdk';

   async function main() {
     const client = new IcnClient({
       endpoint: 'http://localhost:8080',
     });

     await client.loadIdentity({ /* ... */ });
     await client.authenticate();

     // Connect to WebSocket
     const ws = await client.connectWebSocket();

     // Subscribe to events
     ws.on('ledger:entry', (entry) => {
       console.log('New ledger entry:', entry);
     });

     ws.on('trust:update', (update) => {
       console.log('Trust update:', update);
     });

     console.log('Listening for events... (Ctrl+C to exit)');

     // Keep running
     await new Promise(() => {});
   }

   main().catch(console.error);
   ```

2. Run and observe events when activity occurs

### WebSocket protocol
```
Client connects to /ws with JWT in query string
Server sends events as JSON messages:
{
  "type": "ledger:entry",
  "data": { ... entry details ... }
}
```

### Checkpoint
- [ ] You connected to WebSocket
- [ ] You received real-time events

## Part 9: Rate Limiting Exploration

### Steps
1. Open `icn/crates/icn-gateway/src/rate_limit.rs`
2. Find the rate limit configuration
3. Understand per-DID vs per-IP limits

### Rate limit tiers
| Endpoint Category | Limit |
|-------------------|-------|
| Read operations | 200/min |
| Write operations | 60/min |
| Auth endpoints | 120/min per IP |
| Governance | 30/min |
| Compute | 10/min |

### Questions to answer
1. What header indicates remaining rate limit?
2. What happens when rate limited?
3. How does trust level affect limits?

### Test rate limiting
```bash
# Rapid requests to trigger rate limit
for i in {1..100}; do
  curl -s http://localhost:8080/health > /dev/null
done
# Should see 429 responses eventually
```

### Checkpoint
- [ ] You understand rate limiting tiers
- [ ] You know how to handle rate limit errors

## Summary

After completing this workshop you should be able to:
- Authenticate with the ICN gateway using challenge-response
- Use the TypeScript SDK for ledger and trust operations
- Connect to WebSocket for real-time events
- Understand rate limiting and error handling

## Troubleshooting

### "Authentication failed"
Ensure your keystore is properly unlocked and the DID matches.

### "Token expired"
Re-authenticate to get a fresh JWT token.

### "Rate limit exceeded"
Wait for the rate limit window to reset, or reduce request frequency.

### "Connection refused"
Ensure the ICN daemon is running and listening on the expected port.

## Next steps
Proceed to Workshop 8: Web UI
