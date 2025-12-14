/**
 * Example: WebSocket Event Filtering
 *
 * Demonstrates how to use event filters to process specific
 * types of real-time events from WebSocket subscriptions.
 */

import { ICNClient, EventFilter, WsMessage } from '../src';

async function main() {
  const client = new ICNClient({
    baseUrl: 'http://localhost:8080',
  });

  // Authenticate (implementation omitted for brevity)
  // await client.authenticate('did:icn:alice', signer, 'food-coop');

  // Example 1: Filter only payment events
  console.log('=== Subscribing to Payments ===');
  const subscription = client.subscribe('food-coop', {
    onEvent: (event) => {
      // Only process payment events
      if (EventFilter.payments()(event)) {
        const payload = (event as any).payload;
        console.log(`💰 Payment: ${payload.from} → ${payload.to}: ${payload.amount}`);
      }
    },
    onAuthOk: (did) => {
      console.log(`Authenticated as: ${did}`);
    },
  });

  // Example 2: Filter events for a specific user
  console.log('\n=== Events for Alice ===');
  const aliceSubscription = client.subscribe('food-coop', {
    onEvent: (event) => {
      // Only process events involving Alice
      if (EventFilter.byDid('did:icn:alice')(event)) {
        console.log('Event involving Alice:', event);
      }
    },
  });

  // Example 3: Filter proposal-related events
  console.log('\n=== Governance Events ===');
  const govSubscription = client.subscribe('food-coop', {
    onEvent: (event) => {
      if (EventFilter.proposals()(event)) {
        const eventType = (event as any).event_type;
        const payload = (event as any).payload;
        
        switch (eventType) {
          case 'ProposalCreated':
            console.log(`📋 New proposal: ${payload.title}`);
            break;
          case 'ProposalOpened':
            console.log(`🗳️  Voting opened: ${payload.proposal_id}`);
            break;
          case 'VoteCast':
            console.log(`✅ Vote cast by ${payload.voter}: ${payload.choice}`);
            break;
          case 'ProposalClosed':
            console.log(`🏁 Proposal closed: ${payload.outcome}`);
            break;
        }
      }
    },
  });

  // Example 4: Combine filters with AND logic
  console.log('\n=== Combined Filters ===');
  const combinedSubscription = client.subscribe('food-coop', {
    onEvent: (event) => {
      // Only payments involving Alice
      const filter = EventFilter.and(
        EventFilter.payments(),
        EventFilter.byDid('did:icn:alice')
      );
      
      if (filter(event)) {
        console.log('Payment involving Alice:', event);
      }
    },
  });

  // Example 5: Multiple event types with OR logic
  console.log('\n=== Payments OR Proposals ===');
  const multiSubscription = client.subscribe('food-coop', {
    onEvent: (event) => {
      const filter = EventFilter.or(
        EventFilter.payments(),
        EventFilter.proposals()
      );
      
      if (filter(event)) {
        console.log('Payment or proposal event:', event);
      }
    },
  });

  // Example 6: Custom filter function
  console.log('\n=== Custom Filter ===');
  const customSubscription = client.subscribe('food-coop', {
    onEvent: (event) => {
      // Only high-value payments (>10 hours)
      const isHighValuePayment = (msg: WsMessage): boolean => {
        if (!EventFilter.payments()(msg)) return false;
        const payload = (msg as any).payload;
        return payload.amount > 10;
      };

      if (isHighValuePayment(event)) {
        const payload = (event as any).payload;
        console.log(`🚨 High-value payment: ${payload.amount} hours`);
      }
    },
  });

  // Example 7: Event aggregation
  console.log('\n=== Event Statistics ===');
  const stats = {
    payments: 0,
    proposals: 0,
    members: 0,
    total: 0,
  };

  const statsSubscription = client.subscribe('food-coop', {
    onEvent: (event) => {
      stats.total++;
      
      if (EventFilter.payments()(event)) {
        stats.payments++;
      } else if (EventFilter.proposals()(event)) {
        stats.proposals++;
      } else if (EventFilter.members()(event)) {
        stats.members++;
      }

      // Log stats every 10 events
      if (stats.total % 10 === 0) {
        console.log('Stats:', stats);
      }
    },
  });

  // Example 8: Event routing to different handlers
  console.log('\n=== Event Routing ===');
  const routingSubscription = client.subscribe('food-coop', {
    onEvent: (event) => {
      if (EventFilter.payments()(event)) {
        handlePayment((event as any).payload);
      } else if (EventFilter.proposals()(event)) {
        handleProposal((event as any).payload);
      } else if (EventFilter.members()(event)) {
        handleMember((event as any).payload);
      }
    },
  });

  // Keep running to receive events
  await new Promise(resolve => setTimeout(resolve, 60000)); // Run for 1 minute

  // Clean up
  subscription.close();
  aliceSubscription.close();
  govSubscription.close();
  combinedSubscription.close();
  multiSubscription.close();
  customSubscription.close();
  statsSubscription.close();
  routingSubscription.close();
}

function handlePayment(payload: any) {
  console.log(`💰 Payment handler: ${payload.from} → ${payload.to}: ${payload.amount}`);
}

function handleProposal(payload: any) {
  console.log(`📋 Proposal handler: ${payload.title || payload.proposal_id}`);
}

function handleMember(payload: any) {
  console.log(`👤 Member handler: ${payload.did}`);
}

main().catch(console.error);
