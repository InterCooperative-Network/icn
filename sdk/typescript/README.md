# @icn/client

TypeScript client SDK for the ICN Gateway API.

## Installation

```bash
npm install @icn/client
```

## Quick Start

```typescript
import { ICNClient } from '@icn/client';

// Create client
const client = new ICNClient({
  baseUrl: 'http://localhost:8080',
});

// Check health
const health = await client.health();
console.log('Gateway status:', health.status);

// Authenticate (you need to provide your own signing logic)
const challenge = await client.getChallenge('did:icn:alice');
const signature = await signWithYourKey(challenge.challenge);
const auth = await client.verify('did:icn:alice', signature, 'my-coop', ['ledger:read', 'ledger:write']);
client.setToken(auth.token);

// Use authenticated APIs
const balance = await client.getBalance('my-coop', 'did:icn:alice');
console.log('Balance:', balance.balance);
```

## Authentication

ICN uses DID-based authentication with JWT tokens.

### Challenge-Response Flow

1. Request a challenge for your DID
2. Sign the challenge with your Ed25519 private key
3. Verify the signature to get a JWT token
4. Use the token for authenticated requests

```typescript
// 1. Get challenge
const { challenge, expires_at } = await client.getChallenge('did:icn:alice');

// 2. Sign challenge (implement your own signing logic)
const signature = await ed25519Sign(challenge, privateKey);

// 3. Verify and get token
const { token } = await client.verify(
  'did:icn:alice',
  signature,
  'my-coop',           // cooperative ID
  ['ledger:read', 'ledger:write', 'coop:read']  // requested scopes
);

// 4. Set token for future requests
client.setToken(token);
```

### Using a Signature Provider

You can implement `SignatureProvider` for cleaner auth:

```typescript
const signer: SignatureProvider = {
  async sign(challenge: string): Promise<string> {
    // Your Ed25519 signing logic here
    return hexSignature;
  }
};

const auth = await client.authenticate('did:icn:alice', signer, 'my-coop');
// Token is automatically set
```

### Automatic Token Refresh

Enable `autoRefresh` to automatically re-authenticate when the token expires:

```typescript
const client = new ICNClient({
  baseUrl: 'http://localhost:8080',
  autoRefresh: true,
  refreshBeforeExpiry: 60,  // Refresh 60 seconds before expiry
});

// Authenticate once - credentials are stored for auto-refresh
await client.authenticate('did:icn:alice', signer, 'my-coop');

// Token will be automatically refreshed before expiring
// No need to handle 401 errors manually
```

## Retry Logic

The SDK includes built-in retry logic with exponential backoff:

```typescript
const client = new ICNClient({
  baseUrl: 'http://localhost:8080',
  retry: {
    maxRetries: 3,           // Default: 3
    initialDelayMs: 1000,    // Default: 1000ms
    maxDelayMs: 10000,       // Default: 10000ms
    backoffMultiplier: 2,    // Default: 2
    jitterFactor: 0.1,       // Default: 0.1 (10%)
    retryableStatuses: [408, 429, 500, 502, 503, 504],  // Default
  }
});
```

Retries are automatic for transient errors (5xx, 429 rate limiting).

## API Reference

### Cooperatives

```typescript
// Create cooperative
const coop = await client.createCoop({
  id: 'food-coop',
  name: 'Food Cooperative',
  settings: { currency: 'hours' }
});

// Get cooperative
const coop = await client.getCoop('food-coop');

// Update cooperative
const updated = await client.updateCoop('food-coop', {
  settings: { credit_limit: 100 }
});

// Delete cooperative
await client.deleteCoop('food-coop');

// Manage members
const members = await client.listMembers('food-coop');
await client.addMember('food-coop', { did: 'did:icn:bob', role: 'member' });
await client.updateMember('food-coop', 'did:icn:bob', { role: 'admin' });
await client.removeMember('food-coop', 'did:icn:bob');
```

### Ledger

```typescript
// Get balance
const balance = await client.getBalance('food-coop', 'did:icn:alice');
console.log(`${balance.balance} ${balance.currency}`);

// Make payment
const payment = await client.pay('food-coop', {
  from: 'did:icn:alice',
  to: 'did:icn:bob',
  amount: 2.5,
  currency: 'hours',
  memo: 'Garden help'
});

// Get transaction history
const history = await client.getHistory('food-coop', {
  offset: 0,
  limit: 50
});
console.log(`Total transactions: ${history.total}`);
```

### Governance

```typescript
// Create domain
const domain = await client.createDomain({
  domain_id: 'coop:food',
  name: 'Food Coop Governance',
  members: ['did:icn:alice', 'did:icn:bob', 'did:icn:carol']
});

// Create proposal
const proposal = await client.createProposal({
  domain_id: 'coop:food',
  title: 'Approve new member',
  kind: 'membership'
});

// Open for voting
await client.openProposal(proposal.id);

// Cast vote
await client.vote(proposal.id, { choice: 'for' });

// Get current tally
const tally = await client.getVotes(proposal.id);
console.log(`For: ${tally.votes_for}, Against: ${tally.votes_against}`);

// Close and get outcome
const outcome = await client.closeProposal(proposal.id);
console.log(`Accepted: ${outcome.accepted}`);
```

### Compute

```typescript
// Submit CCL task
const cclContract = {
  name: 'calculator',
  rules: [{ name: 'add', params: ['a', 'b'], body: [] }]
};
const task = await client.submitTask({
  code: JSON.stringify(cclContract),
  fuel_limit: 10000,
  priority: 'normal',
  payment_rate: 100  // credits per 1000 fuel
});
console.log('Task hash:', task.task_hash);

// Submit WASM task (helper method handles base64 encoding)
const wasmBytes = await fetch('/module.wasm').then(r => r.arrayBuffer());
const wasmTask = await client.submitWasmTask(new Uint8Array(wasmBytes), {
  fuel_limit: 10000,
  inputs: { x: 42 }
});

// Check task status
const status = await client.getTaskStatus(task.task_hash);
console.log('Status:', status.status);  // pending, claimed, completed, failed, cancelled

// Wait for task completion (polls until done)
const result = await client.waitForTask(task.task_hash, 1000, 60000);  // 1s interval, 60s timeout
if (result.status === 'completed') {
  console.log('Output:', result.result?.output);
  console.log('Fuel used:', result.result?.fuel_used);
}

// Cancel a task
await client.cancelTask(task.task_hash, { reason: 'No longer needed' });
```

### WebSocket Events

#### Basic Connection

```typescript
const ws = client.connectWebSocket('food-coop', {
  onOpen: () => {
    console.log('Connected to WebSocket');
  },
  onMessage: (message) => {
    if (message.type === 'Event') {
      console.log('Event:', message.event_type, message.payload);
    }
  },
  onError: (error) => {
    console.error('WebSocket error:', error);
  },
  onClose: () => {
    console.log('WebSocket closed');
  }
});

// Close when done
ws.close();
```

#### Managed Subscription with Auto-Reconnect

Use `ICNSubscription` for production apps that need resilient connections:

```typescript
import { ICNClient, ICNSubscription } from '@icn/client';

const subscription = client.subscribe('food-coop', {
  onEvent: (event) => {
    console.log('Event received:', event);
  },
  onAuthOk: (did) => {
    console.log('Authenticated as:', did);
  },
  onReconnect: (attempt) => {
    console.log(`Reconnecting... attempt ${attempt}`);
  },
  onError: (error) => {
    console.error('Error:', error);
  },
  onDisconnect: () => {
    console.log('Disconnected');
  }
}, {
  autoReconnect: true,           // Default: true
  maxReconnectAttempts: 10,      // Default: 10
  reconnectDelayMs: 1000,        // Default: 1000ms
  maxReconnectDelayMs: 30000,    // Default: 30000ms
});

// Check connection status
console.log('Connected:', subscription.isConnected());

// Send a message
subscription.send({ type: 'Ping' });

// Close the subscription (stops auto-reconnect)
subscription.close();
```

## Error Handling

All API methods throw `ICNError` on failure:

```typescript
import { ICNError } from '@icn/client';

try {
  await client.pay('food-coop', { ... });
} catch (error) {
  if (error instanceof ICNError) {
    console.error(`Error ${error.statusCode}: ${error.message}`);

    if (error.statusCode === 401) {
      // Re-authenticate
    } else if (error.statusCode === 429) {
      // Rate limited, back off
    }
  }
}
```

## Scopes

Request specific scopes during authentication:

| Scope | Description |
|-------|-------------|
| `ledger:read` | Read balances and history |
| `ledger:write` | Create payments |
| `coop:read` | Read cooperative info |
| `coop:write` | Modify cooperative settings |
| `coop:admin` | Manage members |
| `gov:read` | Read governance domains/proposals |
| `gov:write` | Create proposals, cast votes |
| `compute:read` | Check task status |
| `compute:write` | Submit and cancel tasks |

## Browser Usage

The SDK works in browsers with a fetch polyfill:

```typescript
import { ICNClient } from '@icn/client';

const client = new ICNClient({
  baseUrl: 'http://localhost:8080',
  fetch: window.fetch.bind(window)  // Optional, auto-detected
});
```

Note: WebSocket in browsers uses the native `WebSocket` API.

## Development

```bash
# Install dependencies
npm install

# Build
npm run build

# Watch mode
npm run dev

# Run tests
npm test

# Regenerate types from OpenAPI spec
npm run generate-types
```

### Type Generation

The SDK includes auto-generated types from the OpenAPI specification (`docs/api/openapi.yaml`).

- **`src/api-types.ts`** - Generated types matching the API spec exactly
- **`src/types.ts`** - Hand-written types for the client SDK

To regenerate types after API changes:

```bash
npm run generate-types
```

This uses [openapi-typescript](https://github.com/openapi-ts/openapi-typescript) to ensure type safety with the API.

## License

MIT OR Apache-2.0
