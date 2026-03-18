/**
 * Example: Query Builder
 *
 * Demonstrates how to use the fluent query builder API to filter
 * transaction history with various criteria.
 */

import { ICNClient } from '../src';

async function main() {
  const client = new ICNClient({
    baseUrl: 'http://localhost:8080',
  });

  // Authenticate (implementation omitted for brevity)
  // await client.authenticate('did:icn:alice', signer, 'food-coop');

  // Example 1: Get last 30 days of transactions
  console.log('=== Last 30 Days ===');
  const recent = await client.queryHistory('food-coop')
    .lastDays(30)
    .limit(50)
    .execute();
  console.log(`Found ${recent.total} transactions in last 30 days`);

  // Example 2: Get all payments sent by Alice
  console.log('\n=== Payments from Alice ===');
  const fromAlice = await client.queryHistory('food-coop')
    .fromDid('did:icn:alice')
    .limit(100)
    .execute();
  console.log(`Alice sent ${fromAlice.total} payments`);

  // Example 3: Get all payments received by Bob
  console.log('\n=== Payments to Bob ===');
  const toBob = await client.queryHistory('food-coop')
    .toDid('did:icn:bob')
    .limit(100)
    .execute();
  console.log(`Bob received ${toBob.total} payments`);

  // Example 4: Get high-value transactions (>10 hours)
  console.log('\n=== High-Value Transactions ===');
  const highValue = await client.queryHistory('food-coop')
    .minAmount(10)
    .limit(50)
    .execute();
  console.log(`Found ${highValue.total} transactions over 10 hours`);

  // Example 5: Get transactions in a specific date range
  console.log('\n=== January 2025 Transactions ===');
  const january = await client.queryHistory('food-coop')
    .startDate('2025-01-01T00:00:00Z')
    .endDate('2025-01-31T23:59:59Z')
    .limit(100)
    .execute();
  console.log(`Found ${january.total} transactions in January`);

  // Example 6: Complex query - Alice's large outgoing payments last week
  console.log('\n=== Complex Query ===');
  const complex = await client.queryHistory('food-coop')
    .fromDid('did:icn:alice')
    .minAmount(5)
    .lastDays(7)
    .limit(20)
    .execute();
  console.log(`Alice made ${complex.total} payments >5 hours in the last week`);

  // Example 7: Pagination through results
  console.log('\n=== Paginated Results ===');
  let offset = 0;
  const pageSize = 10;
  let hasMore = true;

  while (hasMore) {
    const page = await client.queryHistory('food-coop')
      .offset(offset)
      .limit(pageSize)
      .execute();

    console.log(`Page ${offset / pageSize + 1}: ${page.transactions.length} transactions`);
    
    page.transactions.forEach(tx => {
      console.log(`  ${tx.from} → ${tx.to}: ${tx.amount} ${tx.currency}`);
    });

    offset += pageSize;
    hasMore = page.transactions.length === pageSize && offset < page.total;
  }

  // Example 8: Get transaction summaries
  console.log('\n=== Transaction Summary ===');
  const all = await client.queryHistory('food-coop')
    .limit(1000)
    .execute();

  const summary = {
    total: all.total,
    totalVolume: all.transactions.reduce((sum, tx) => sum + tx.amount, 0),
    avgAmount: 0,
  };
  summary.avgAmount = summary.totalVolume / (all.total || 1);

  console.log(`Total transactions: ${summary.total}`);
  console.log(`Total volume: ${summary.totalVolume.toFixed(2)} hours`);
  console.log(`Average amount: ${summary.avgAmount.toFixed(2)} hours`);
}

main().catch(console.error);
