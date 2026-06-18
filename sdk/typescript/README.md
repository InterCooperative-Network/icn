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

// Authenticate — challenge/verify proves control of your DID key.
// NOTE: passing a coop_id here is dev/demo-only self-serve issuance and is
// fail-closed in production. DID key control is not cooperative authority.
// See "Authentication" below for the production-shaped pattern.
const challenge = await client.getChallenge('did:icn:alice');
const signature = await signWithYourKey(challenge.challenge);
const auth = await client.verify('did:icn:alice', signature, 'my-coop', ['ledger:read', 'ledger:write']);
client.setToken(auth.token);

// Use authenticated APIs
const position = await client.getPosition('my-coop', 'did:icn:alice');
console.log('Position:', position.position, position.unit);
```

## Authentication

**Two different things, often confused. Keep them separate.**

- **DID key authentication** — the challenge/verify flow proves you *control a DID's
  key*. That is all it proves. It does not prove membership, standing, or any authority
  to act for a cooperative.
- **Institutional authority issuance** — a cooperative-scoped token (one whose `coop_id`
  the gateway will *trust*) must come from a trusted institutional path: membership,
  standing, role, capability, delegation, invite, session, or SDIS-backed proof. It is
  never minted from a `coop_id` the caller picked.

> **DID key control is not cooperative authority. A capability token is not a mandate.**
> See [`ABUSE_CASE_HARDENING_STRATEGY.md`](../../docs/architecture/ABUSE_CASE_HARDENING_STRATEGY.md),
> [RFC-0018](../../docs/rfcs/RFC-0018-entity-aware-request-authorization.md), and
> [ADR-0035](../../docs/adr/ADR-0035-entity-aware-request-authorization.md).

### Production: how `coop_id` is treated today

Passing a caller-chosen `coop_id` to `/auth/verify` is **fail-closed in production**
(issue [#2077](https://github.com/InterCooperative-Network/icn/issues/2077)): the gateway
refuses to bind self-asserted cooperative authority into a token. A trusted production
issuance path (first-admin bootstrap, invite/session, membership-resolved issuance) is
**planned, not yet shipped** — tracked by
[#2080](https://github.com/InterCooperative-Network/icn/issues/2080). Entity-aware
authorization enforcement is tracked by
[#2081](https://github.com/InterCooperative-Network/icn/issues/2081), and the canonical
`coop_id ↔ EntityId` mapping by
[#2082](https://github.com/InterCooperative-Network/icn/issues/2082).

Until #2080 lands, application code obtains a cooperative-scoped token from a trusted
issuer out of band and hands it to the client directly:

```typescript
// Production-shaped: the token comes from a trusted institutional path,
// not from a coop_id the client asserted. (See examples/seed-demo-data.ts.)
const client = new ICNClient({ baseUrl: 'http://localhost:8080', token });
```

### Dev/demo only: self-serve challenge → verify → token

The challenge/verify flow below mints a token carrying a **caller-supplied, unverified**
`coop_id`. The gateway honors it only when it is explicitly built for a dev/demo posture
(`AuthManager::with_self_asserted_coop(true)`, which production never enables); per ICN's
dev-gate doctrine such a posture must also be confined to an explicit opt-in
(`ICN_DEV_MODE`) plus a loopback bind. This is **not** how production cooperative
authority is obtained — do not ship it as a login flow.

1. Request a challenge for your DID
2. Sign the challenge with your Ed25519 private key
3. Verify the signature to get a JWT token *(dev/demo posture only)*
4. Use the token for authenticated requests

```typescript
// DEV/DEMO ONLY — fail-closed in production (see above).
// 1. Get challenge
const { challenge, expires_at } = await client.getChallenge('did:icn:alice');

// 2. Sign challenge (implement your own signing logic)
const signature = await ed25519Sign(challenge, privateKey);

// 3. Verify and get token — the coop_id here is a SELF-ASSERTED claim,
//    accepted only under a dev/demo gateway posture, not production authority.
const { token } = await client.verify(
  'did:icn:alice',
  signature,
  'my-coop',           // self-asserted coop_id (dev/demo only)
  ['ledger:read', 'ledger:write', 'coop:read']  // requested scopes
);

// 4. Set token for future requests
client.setToken(token);
```

#### Using a Signature Provider

`SignatureProvider` cleans up the signing step of the same **dev/demo** flow — the
self-asserted `coop_id` caveat above still applies:

```typescript
const signer: SignatureProvider = {
  async sign(challenge: string): Promise<string> {
    // Your Ed25519 signing logic here
    return hexSignature;
  }
};

// DEV/DEMO ONLY — self-asserted coop_id, fail-closed in production.
const auth = await client.authenticate('did:icn:alice', signer, 'my-coop');
// Token is automatically set
```

#### Automatic Token Refresh

`autoRefresh` re-runs whatever authentication you configured when the token nears expiry,
so it inherits that flow's posture: with the dev/demo challenge/verify flow it stays
dev/demo-only; in production it refreshes a token sourced from a trusted issuer.

```typescript
const client = new ICNClient({
  baseUrl: 'http://localhost:8080',
  autoRefresh: true,
  refreshBeforeExpiry: 60,  // Refresh 60 seconds before expiry
});

// Dev/demo: re-authenticates via challenge/verify before expiry.
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

### Batch Operations

Efficiently process multiple operations at once:

```typescript
// Batch settlements
const results = await client.batchSettle('food-coop', [
  { from: 'admin', to: 'alice', amount: 10, unit: 'hours', memo: 'January work' },
  { from: 'admin', to: 'bob', amount: 5, unit: 'hours', memo: 'Website help' },
]);
console.log(`${results.succeeded} succeeded, ${results.failed} failed`);

// Batch add members
await client.batchAddMembers('food-coop', [
  { did: 'did:icn:dave', role: 'member' },
  { did: 'did:icn:eve', role: 'member' },
]);

// Batch update members
await client.batchUpdateMembers('food-coop', [
  { did: 'did:icn:alice', updates: { role: 'admin' } },
  { did: 'did:icn:bob', updates: { role: 'admin' } },
]);
```

### Query Builder

Fluent API for filtering transaction history:

```typescript
// Get last 30 days of high-value transactions from Alice
const history = await client.queryHistory('food-coop')
  .fromDid('did:icn:alice')
  .minAmount(10)
  .lastDays(30)
  .limit(50)
  .execute();

// Complex date range query
const january = await client.queryHistory('food-coop')
  .startDate('2025-01-01T00:00:00Z')
  .endDate('2025-01-31T23:59:59Z')
  .toDid('did:icn:bob')
  .execute();

// Pagination
const page = await client.queryHistory('food-coop')
  .offset(100)
  .limit(50)
  .execute();
```

Available query methods:
- `.fromDid(did)` - Filter by sender
- `.toDid(did)` - Filter by recipient
- `.minAmount(amount)` - Minimum transaction amount
- `.maxAmount(amount)` - Maximum transaction amount
- `.startDate(date)` - Start of date range (ISO 8601)
- `.endDate(date)` - End of date range (ISO 8601)
- `.lastDays(days)` - Last N days
- `.offset(n)` - Pagination offset
- `.limit(n)` - Result limit
- `.execute()` - Execute the query

### Cooperatives

```typescript
// Create cooperative
const coop = await client.createCoop({
  id: 'food-coop',
  name: 'Food Cooperative',
  settings: { unit: 'hours' }
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
// Get position
const position = await client.getPosition('food-coop', 'did:icn:alice');
console.log(`${position.position} ${position.unit}`);

// Record a settlement
const settlement = await client.settle('food-coop', {
  from: 'did:icn:alice',
  to: 'did:icn:bob',
  amount: 2.5,
  unit: 'hours',
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

#### Event Filters

Use `EventFilter` helpers to process specific event types:

```typescript
import { ICNClient, EventFilter } from '@icn/client';

subscription.onEvent((event) => {
  // Filter only payment events
  if (EventFilter.payments()(event)) {
    console.log('Payment event:', event);
  }
  
  // Filter events involving specific DID
  if (EventFilter.byDid('did:icn:alice')(event)) {
    console.log('Event involving Alice:', event);
  }
  
  // Filter proposal-related events
  if (EventFilter.proposals()(event)) {
    console.log('Governance event:', event);
  }
  
  // Combine filters with AND
  if (EventFilter.and(
    EventFilter.payments(),
    EventFilter.byDid('did:icn:alice')
  )(event)) {
    console.log('Payment involving Alice:', event);
  }
  
  // Combine filters with OR
  if (EventFilter.or(
    EventFilter.payments(),
    EventFilter.proposals()
  )(event)) {
    console.log('Payment or proposal:', event);
  }
});
```

Available filters:
- `EventFilter.payments()` - Payment events
- `EventFilter.proposals()` - Proposal/voting events
- `EventFilter.members()` - Member management events
- `EventFilter.byType(eventType)` - Specific event type
- `EventFilter.byDid(did)` - Events involving a DID
- `EventFilter.and(...filters)` - Combine with AND
- `EventFilter.or(...filters)` - Combine with OR

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
  await client.settle('food-coop', { ... });
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

### Examples

See the [examples](./examples/) directory for practical demonstrations:

- **[batch-operations.ts](./examples/batch-operations.ts)** - Batch payments and member management
- **[query-builder.ts](./examples/query-builder.ts)** - Advanced transaction history queries
- **[websocket-filters.ts](./examples/websocket-filters.ts)** - Real-time event filtering

Run examples with:
```bash
npx ts-node examples/batch-operations.ts
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

## Pilot Features (v0.9.0+)

### Notifications

> **Not implemented in this SDK.** Earlier drafts documented
> `connectNotifications`, `listNotifications`, `markNotificationRead`, and
> `getNotificationCount`; none of those methods exist on `ICNClient`, and there
> is no in-app notification store.
>
> For real-time updates, subscribe to the gateway WebSocket event stream with the
> methods that *do* exist — `client.connectWebSocket(coopId, handlers)` or the
> managed `client.subscribe(coopId, handlers, opts)` — and react to
> `SettlementCreated`, governance, and compute events. See **WebSocket Events**
> above.

### Recurring payments, escrow & budgets

> **Not implemented in this SDK.** Earlier drafts of this README documented
> `createRecurringPayment`, `createEscrow`, `createBudget`, and related helpers
> using fiat-style fields (`from_account`, `currency: 'USD'`, dollar amounts).
> Those methods were never shipped on `ICNClient`, and ICN does not provide
> payment, escrow, or banking facilities.
>
> The ledger surface that *does* exist records mutual-credit settlements between
> members and reports their net positions. Amounts are denominated in a
> cooperative-defined `unit` (e.g. `hours`), never a currency:
>
> - `client.settle(coopId, { from, to, amount, unit, memo })` — record a settlement
> - `client.getPosition(coopId, did)` — read a member's net position
> - `client.getHistory(coopId, { offset, limit })` — list recorded settlements
> - `client.crossPay(coopId, …)` — cross-unit settlement via the exchange-rate oracle
>
> See the **Ledger** section above for runnable examples.

### Governance UI Support

Enhanced governance endpoints for building UIs:

```typescript
// Charter viewing
const summary = await client.getCharterSummary(charterId);
const founders = await client.getCharterFounders(charterId);
const timeline = await client.getCharterTimeline(charterId);

// Amendment voting
await client.castAmendmentVote(amendmentId, {
  vote: 'approve', // approve, reject, abstain
  comment: 'Fully support this change'
});

const results = await client.getAmendmentResults(amendmentId);
console.log(`Votes: ${results.approve_count} / ${results.total_votes}`);
console.log(`Quorum: ${results.has_quorum ? 'Met' : 'Not met'}`);

// Appeals management
const appealTimeline = await client.getAppealTimeline(appealId);
const appealStatus = await client.getAppealStatus(appealId);
console.log(`Next steps: ${appealStatus.next_steps.join(', ')}`);

// Governance dashboard
const dashboard = await client.getGovernanceDashboard(charterId);
console.log(`Active amendments: ${dashboard.pending_amendments}`);
console.log(`Open appeals: ${dashboard.open_appeals}`);
console.log('Recent activity:', dashboard.recent_activity);
```

## API Reference

See the full API documentation at [docs/api](../../docs/api/README.md).

## TypeScript Types

All API responses are fully typed. Import types as needed:

```typescript
import type {
  Position,
  SettlementRequest,
  SettlementResponse,
  Transaction,
  TransactionHistory,
  TreasuryStatus,
} from '@icn/client';
```

## Error Handling

```typescript
try {
  await client.settle('food-coop', settlementRequest);
} catch (error) {
  if (error.status === 401) {
    console.error('Not authenticated');
  } else if (error.status === 403) {
    console.error('Insufficient permissions');
  } else if (error.status === 429) {
    console.error('Rate limit exceeded');
  } else {
    console.error('API error:', error.message);
  }
}
```

## License

MIT
