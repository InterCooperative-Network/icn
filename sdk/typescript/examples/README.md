# ICN SDK Examples

Practical examples demonstrating common patterns and use cases with the ICN TypeScript SDK.

## Running Examples

```bash
# Install dependencies
npm install

# Run an example
npx ts-node examples/batch-operations.ts
npx ts-node examples/query-builder.ts
npx ts-node examples/websocket-filters.ts
```

## Examples

### 1. Commons Evolution (`commons-evolution.ts`)

Comprehensive example of Commons Evolution governance features:

- **Charter Management**: Create, sign, and activate cooperative charters
- **Membership Lifecycle**: Apply, approve, promote, and manage capabilities
- **Constitutional Amendments**: Draft, modify, submit, and ratify amendments
- **Appeals Process**: File appeals, add evidence, and resolve disputes

**Use cases**:
- Founding a new cooperative with multiple co-founders
- Managing member onboarding and progression
- Democratic governance rule changes
- Fair dispute resolution processes

```typescript
// Create a cooperative charter
const charter = await client.createCharter({
  domain_id: 'coop:green-valley',
  name: 'Green Valley Food Cooperative',
  org_type: 'cooperative',
});

// Sign as a founder
const signed = await client.signCharter(charter.charter_id, signature, 'founding_member');
console.log(`${signed.total_founders} founders signed, ${signed.founders_needed} more needed`);

// Apply for membership
const member = await client.applyForMembership('coop:green-valley', ['vote', 'transact']);

// File an appeal
const appeal = await client.fileAppeal({
  appeal_type: { category: 'membership_denial' },
  grounds: [{ ground_type: 'procedural_error', description: 'Not given proper review' }],
  statement: 'Request fair review of my application',
  requested_remedy: 'reinstate',
});
```

### 2. Batch Operations (`batch-operations.ts`)

Learn how to efficiently process multiple operations:

- **Batch Settlements**: Send multiple settlements in one operation
- **Batch Member Management**: Add or update multiple members at once
- **Error Handling**: Handle partial failures gracefully

**Use cases**:
- Monthly payroll processing
- Bulk onboarding of new cooperative members
- Batch updates to member roles

```typescript
// Send multiple settlements at once
const results = await client.batchSettle('food-coop', [
  { from: 'admin', to: 'alice', amount: 10, unit: 'hours', memo: 'Work' },
  { from: 'admin', to: 'bob', amount: 5, unit: 'hours', memo: 'Help' },
]);
console.log(`${results.succeeded} succeeded, ${results.failed} failed`);
```

### 2. Query Builder (`query-builder.ts`)

Fluent API for filtering transaction history:

- **Time-based Queries**: Last N days, date ranges
- **DID Filters**: Payments from/to specific members
- **Amount Filters**: High-value transactions
- **Pagination**: Process large result sets
- **Aggregations**: Calculate summaries

**Use cases**:
- Generate monthly reports
- Find specific transactions
- Calculate member contribution statistics
- Export data for accounting

```typescript
// Get high-value transactions from last week
const history = await client.queryHistory('food-coop')
  .fromDid('did:icn:alice')
  .minAmount(10)
  .lastDays(7)
  .execute();
```

### 3. WebSocket Filters (`websocket-filters.ts`)

Process real-time events efficiently:

- **Event Type Filters**: Payments, proposals, members
- **DID Filters**: Events involving specific members
- **Compound Filters**: AND/OR logic
- **Custom Filters**: Write your own filter functions
- **Event Routing**: Route events to different handlers
- **Aggregation**: Collect statistics

**Use cases**:
- Real-time dashboards
- Notification systems
- Activity feeds
- Audit logs
- Event-driven workflows

```typescript
// Subscribe to high-value payments only
subscription.onEvent((event) => {
  const filter = EventFilter.and(
    EventFilter.payments(),
    (msg) => (msg as any).payload.amount > 10
  );
  
  if (filter(event)) {
    console.log('High-value payment detected!');
  }
});
```

## Common Patterns

### Authentication

All examples need a token. **DID key control is not cooperative authority**, so *how* you
get that token matters:

**Production** — obtain a cooperative-scoped token from a trusted institutional path
(membership / invite / session / SDIS) out of band and pass it to the client. Self-asserted
`coop_id` issuance at `/auth/verify` is fail-closed in production (PR
[#2077](https://github.com/InterCooperative-Network/icn/pull/2077); trusted issuance is
tracked by issue [#2080](https://github.com/InterCooperative-Network/icn/issues/2080)). See
[`seed-demo-data.ts`](./seed-demo-data.ts) for this pattern.

```typescript
import { ICNClient } from '@icn/client';

// Production-shaped: token comes from a trusted issuer, not a self-asserted coop_id.
const client = new ICNClient({ baseUrl: 'http://localhost:8080', token });
```

**Dev/demo only** — the challenge/verify flow mints a token from a caller-supplied,
**unverified** `coop_id`, accepted only under a dev/demo gateway posture. Do not ship it as
a login flow. (See the SDK README's Authentication section for the full caveat.)

```typescript
const client = new ICNClient({ baseUrl: 'http://localhost:8080' });

// DEV/DEMO ONLY — self-asserted coop_id, fail-closed in production.
const challenge = await client.getChallenge('did:icn:alice');
const signature = await signChallenge(challenge.nonce);  // your Ed25519 signing
const auth = await client.verify(
  'did:icn:alice',
  signature,
  'food-coop',         // self-asserted coop_id (dev/demo only)
  ['ledger:read', 'ledger:write', 'coop:admin']
);
client.setToken(auth.token, auth.expires_at);
```

### Error Handling

All SDK methods throw `ICNError` on failure:

```typescript
try {
  await client.settle('food-coop', { ... });
} catch (error) {
  if (error instanceof ICNError) {
    if (error.statusCode === 401) {
      // Re-authenticate
    } else if (error.statusCode === 429) {
      // Rate limited, back off
    }
  }
}
```

### Automatic Token Refresh

Enable auto-refresh to avoid managing token expiration. It only re-runs the **dev/demo
challenge/verify flow** (it needs the signer + DID stored by `authenticate()`); a
production token injected directly is **not** auto-refreshed. See the Authentication note
above.

```typescript
const client = new ICNClient({
  baseUrl: 'http://localhost:8080',
  autoRefresh: true,
  refreshBeforeExpiry: 60,  // Refresh 60s before expiry
});

// Dev/demo: re-authenticates via challenge/verify (self-asserted coop_id) before expiry.
await client.authenticate('did:icn:alice', signer, 'food-coop');

// Token automatically refreshes - no manual handling needed
```

### WebSocket Reconnection

Use `ICNSubscription` for resilient connections:

```typescript
const subscription = client.subscribe('food-coop', {
  onEvent: (event) => console.log('Event:', event),
  onReconnect: (attempt) => console.log(`Reconnecting... attempt ${attempt}`),
}, {
  autoReconnect: true,
  maxReconnectAttempts: 10,
});
```

## Advanced Patterns

### Transaction History Export

```typescript
// Export all transactions to CSV
const all = await client.queryHistory('food-coop')
  .limit(10000)
  .execute();

const csv = all.transactions.map(tx => 
  `${tx.timestamp},${tx.from},${tx.to},${tx.amount},${tx.unit},${tx.memo}`
).join('\n');

await fs.writeFile('transactions.csv', csv);
```

### Real-time Position Updates

```typescript
let position = (await client.getPosition('food-coop', 'did:icn:alice')).position;

subscription.onEvent((event) => {
  if (EventFilter.byDid('did:icn:alice')(event)) {
    if (EventFilter.payments()(event)) {
      const payload = (event as any).payload;
      if (payload.from === 'did:icn:alice') {
        position -= payload.amount;
      } else if (payload.to === 'did:icn:alice') {
        position += payload.amount;
      }
      console.log(`New position: ${position}`);
    }
  }
});
```

### Proposal Notification System

```typescript
subscription.onEvent((event) => {
  if (EventFilter.proposals()(event)) {
    const eventType = (event as any).event_type;
    const payload = (event as any).payload;
    
    switch (eventType) {
      case 'ProposalCreated':
        await sendEmail({
          to: 'members@coop.org',
          subject: `New Proposal: ${payload.title}`,
          body: `A new proposal has been created. Vote now!`,
        });
        break;
      
      case 'ProposalOpened':
        await sendSMS({
          to: getAllMemberPhones(),
          message: `Voting is now open on proposal ${payload.proposal_id}`,
        });
        break;
    }
  }
});
```

### Monthly Report Generation

```typescript
async function generateMonthlyReport(coopId: string, month: string) {
  // Get all transactions for the month
  const history = await client.queryHistory(coopId)
    .startDate(`${month}-01T00:00:00Z`)
    .endDate(`${month}-31T23:59:59Z`)
    .limit(10000)
    .execute();

  // Calculate statistics
  const stats = {
    totalVolume: history.transactions.reduce((sum, tx) => sum + tx.amount, 0),
    avgTransaction: 0,
    topContributors: new Map<string, number>(),
  };

  history.transactions.forEach(tx => {
    const current = stats.topContributors.get(tx.from) || 0;
    stats.topContributors.set(tx.from, current + tx.amount);
  });

  stats.avgTransaction = stats.totalVolume / history.total;

  return {
    month,
    totalTransactions: history.total,
    totalVolume: stats.totalVolume,
    avgTransaction: stats.avgTransaction,
    topContributors: Array.from(stats.topContributors.entries())
      .sort((a, b) => b[1] - a[1])
      .slice(0, 10),
  };
}
```

## Best Practices

1. **Always handle errors**: Use try-catch and check for `ICNError`
2. **Use batch operations**: When processing multiple items
3. **Filter events early**: Use EventFilter to reduce processing
4. **Enable auto-reconnect**: For production WebSocket connections
5. **Set appropriate limits**: Don't fetch more data than you need
6. **Use query builders**: For complex history queries
7. **Cache positions**: Update from events instead of polling
8. **Close connections**: Always clean up WebSocket subscriptions

## Need Help?

- 📖 [SDK Documentation](../README.md)
- 🌐 [ICN Documentation](../../../docs/)
- 💬 [Community Forum](https://forum.icn.coop)
- 🐛 [Report Issues](https://github.com/InterCooperative-Network/icn/issues)
