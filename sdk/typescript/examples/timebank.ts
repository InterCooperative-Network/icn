/**
 * Timebank Example
 *
 * Shows how to use ICN for a timebank cooperative:
 * - Check positions
 * - Log service hours
 * - View transaction history
 *
 * Run: npx ts-node examples/timebank.ts
 */

import { ICNClient, ICNError } from '../src';

const COOP_ID = 'timebank-coop';

async function main() {
  // Create authenticated client (assume token is set)
  const client = new ICNClient({
    baseUrl: 'http://localhost:8080',
    token: 'your-jwt-token', // Get this via authentication
  });

  const alice = 'did:icn:alice';
  const bob = 'did:icn:bob';
  const carol = 'did:icn:carol';

  // -------------------------------------------------------------------------
  // Check Current Positions
  // -------------------------------------------------------------------------
  console.log('=== Current Positions ===\n');

  const alicePos = await client.getPosition(COOP_ID, alice);
  const bobPos = await client.getPosition(COOP_ID, bob);
  const carolPos = await client.getPosition(COOP_ID, carol);

  console.log(`Alice: ${alicePos.position} ${alicePos.unit}`);
  console.log(`Bob:   ${bobPos.position} ${bobPos.unit}`);
  console.log(`Carol: ${carolPos.position} ${carolPos.unit}`);

  // -------------------------------------------------------------------------
  // Log Service Hours
  // -------------------------------------------------------------------------
  console.log('\n=== Logging Service Hours ===\n');

  // Alice helped Bob with gardening for 2 hours
  try {
    const settlement = await client.settle(COOP_ID, {
      from: bob, // Bob owes Alice
      to: alice, // Alice receives credit
      amount: 2.0,
      unit: 'hours',
      memo: 'Gardening help - weeding and planting',
    });

    console.log(`Transaction ${settlement.id}:`);
    console.log(`  Bob -> Alice: ${settlement.amount} hours`);
    console.log(`  Memo: ${settlement.memo}`);
    console.log(`  Time: ${new Date(settlement.timestamp * 1000).toLocaleString()}`);
  } catch (error) {
    if (error instanceof ICNError) {
      if (error.statusCode === 400) {
        console.error('Settlement failed:', error.message);
        console.error('(Bob may have insufficient credit)');
      } else {
        console.error('API error:', error.statusCode, error.message);
      }
    } else {
      throw error;
    }
  }

  // Carol did tutoring for Alice for 1.5 hours
  const tutoring = await client.settle(COOP_ID, {
    from: alice,
    to: carol,
    amount: 1.5,
    unit: 'hours',
    memo: 'Spanish tutoring session',
  });
  console.log(`\nCarol earned ${tutoring.amount} hours from Alice (tutoring)`);

  // -------------------------------------------------------------------------
  // Updated Balances
  // -------------------------------------------------------------------------
  console.log('\n=== Updated Positions ===\n');

  const newAlice = await client.getPosition(COOP_ID, alice);
  const newBob = await client.getPosition(COOP_ID, bob);
  const newCarol = await client.getPosition(COOP_ID, carol);

  console.log(`Alice: ${newAlice.position} hours (was ${alicePos.position})`);
  console.log(`Bob:   ${newBob.position} hours (was ${bobPos.position})`);
  console.log(`Carol: ${newCarol.position} hours (was ${carolPos.position})`);

  // -------------------------------------------------------------------------
  // Transaction History
  // -------------------------------------------------------------------------
  console.log('\n=== Recent Transactions ===\n');

  const history = await client.getHistory(COOP_ID, { limit: 10 });

  console.log(`Showing ${history.transactions.length} of ${history.total} transactions:\n`);

  for (const tx of history.transactions) {
    const from = tx.from.split(':').pop()?.slice(0, 8);
    const to = tx.to.split(':').pop()?.slice(0, 8);
    const date = new Date(tx.timestamp * 1000).toLocaleDateString();

    console.log(`${date} | ${from}... -> ${to}... | ${tx.amount} ${tx.unit}`);
    if (tx.memo) {
      console.log(`         ${tx.memo}`);
    }
  }

  // -------------------------------------------------------------------------
  // Monthly Summary
  // -------------------------------------------------------------------------
  console.log('\n=== Monthly Summary ===\n');

  // Get all transactions for analysis
  const allTx = await client.getHistory(COOP_ID, { limit: 100 });

  const oneMonthAgo = Date.now() / 1000 - 30 * 24 * 60 * 60;
  const monthlyTx = allTx.transactions.filter((tx) => tx.timestamp > oneMonthAgo);

  const totalHours = monthlyTx.reduce((sum, tx) => sum + tx.amount, 0);
  const avgPerTx = monthlyTx.length > 0 ? totalHours / monthlyTx.length : 0;

  console.log(`Transactions this month: ${monthlyTx.length}`);
  console.log(`Total hours exchanged: ${totalHours}`);
  console.log(`Average per transaction: ${avgPerTx.toFixed(1)} hours`);
}

main().catch(console.error);
