/**
 * Example: Batch Operations
 *
 * Demonstrates how to use batch operations to efficiently process
 * multiple transactions or member updates.
 */

import { ICNClient } from '../src';

async function main() {
  const client = new ICNClient({
    baseUrl: 'http://localhost:8080',
  });

  // Authenticate (you need to implement signing logic)
  // const challenge = await client.getChallenge('did:icn:admin');
  // const signature = await signChallenge(challenge.challenge);
  // await client.authenticate('did:icn:admin', { sign: () => signature }, 'food-coop');

  // Example 1: Batch payments
  console.log('=== Batch Payments ===');
  const payments = [
    {
      from: 'did:icn:admin',
      to: 'did:icn:alice',
      amount: 10,
      currency: 'hours',
      memo: 'January coordination work',
    },
    {
      from: 'did:icn:admin',
      to: 'did:icn:bob',
      amount: 5,
      currency: 'hours',
      memo: 'Website updates',
    },
    {
      from: 'did:icn:admin',
      to: 'did:icn:carol',
      amount: 8,
      currency: 'hours',
      memo: 'Bookkeeping',
    },
  ];

  const paymentResults = await client.batchPay('food-coop', payments);
  console.log(`Payments: ${paymentResults.succeeded} succeeded, ${paymentResults.failed} failed`);
  
  paymentResults.results.forEach((result, i) => {
    if (result.success) {
      console.log(`  ✓ Payment ${i + 1}: ${result.payment?.hash}`);
    } else {
      console.log(`  ✗ Payment ${i + 1}: ${result.error}`);
    }
  });

  // Example 2: Batch add members
  console.log('\n=== Batch Add Members ===');
  const newMembers = [
    { did: 'did:icn:dave', role: 'member' as const },
    { did: 'did:icn:eve', role: 'member' as const },
    { did: 'did:icn:frank', role: 'member' as const },
  ];

  const memberResults = await client.batchAddMembers('food-coop', newMembers);
  console.log(`Members: ${memberResults.succeeded} added, ${memberResults.failed} failed`);

  // Example 3: Batch update members (promote to admin)
  console.log('\n=== Batch Update Members ===');
  const updates = [
    { did: 'did:icn:alice', updates: { role: 'admin' as const } },
    { did: 'did:icn:bob', updates: { role: 'admin' as const } },
  ];

  const updateResults = await client.batchUpdateMembers('food-coop', updates);
  console.log(`Updates: ${updateResults.succeeded} succeeded, ${updateResults.failed} failed`);

  // Example 4: Handle partial failures
  console.log('\n=== Handling Partial Failures ===');
  const mixedPayments = [
    {
      from: 'did:icn:admin',
      to: 'did:icn:alice',
      amount: 5,
      currency: 'hours',
      memo: 'Valid payment',
    },
    {
      from: 'did:icn:nonexistent',
      to: 'did:icn:alice',
      amount: 999999,  // This will likely fail
      currency: 'hours',
      memo: 'Invalid payment',
    },
  ];

  const mixedResults = await client.batchPay('food-coop', mixedPayments);
  if (mixedResults.failed > 0) {
    console.log('Some payments failed:');
    mixedResults.results.forEach((result, i) => {
      if (!result.success) {
        console.log(`  Payment ${i + 1}: ${result.error}`);
      }
    });
  }
}

main().catch(console.error);
