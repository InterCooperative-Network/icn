/**
 * Timebank Example
 *
 * Shows how to use ICN for a timebank cooperative:
 * - Check balances
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
  // Check Current Balances
  // -------------------------------------------------------------------------
  console.log('=== Current Balances ===\n');

  const aliceBalance = await client.getBalance(COOP_ID, alice);
  const bobBalance = await client.getBalance(COOP_ID, bob);
  const carolBalance = await client.getBalance(COOP_ID, carol);

  console.log(`Alice: ${aliceBalance.balance} ${aliceBalance.currency}`);
  console.log(`Bob:   ${bobBalance.balance} ${bobBalance.currency}`);
  console.log(`Carol: ${carolBalance.balance} ${carolBalance.currency}`);

  // -------------------------------------------------------------------------
  // Log Service Hours
  // -------------------------------------------------------------------------
  console.log('\n=== Logging Service Hours ===\n');

  // Alice helped Bob with gardening for 2 hours
  try {
    const payment = await client.pay(COOP_ID, {
      from: bob, // Bob owes Alice
      to: alice, // Alice receives credit
      amount: 2.0,
      currency: 'hours',
      memo: 'Gardening help - weeding and planting',
    });

    console.log(`Transaction ${payment.id}:`);
    console.log(`  Bob -> Alice: ${payment.amount} hours`);
    console.log(`  Memo: ${payment.memo}`);
    console.log(`  Time: ${new Date(payment.timestamp * 1000).toLocaleString()}`);
  } catch (error) {
    if (error instanceof ICNError) {
      if (error.statusCode === 400) {
        console.error('Payment failed:', error.message);
        console.error('(Bob may have insufficient credit)');
      } else {
        console.error('API error:', error.statusCode, error.message);
      }
    } else {
      throw error;
    }
  }

  // Carol did tutoring for Alice for 1.5 hours
  const tutoring = await client.pay(COOP_ID, {
    from: alice,
    to: carol,
    amount: 1.5,
    currency: 'hours',
    memo: 'Spanish tutoring session',
  });
  console.log(`\nCarol earned ${tutoring.amount} hours from Alice (tutoring)`);

  // -------------------------------------------------------------------------
  // Updated Balances
  // -------------------------------------------------------------------------
  console.log('\n=== Updated Balances ===\n');

  const newAlice = await client.getBalance(COOP_ID, alice);
  const newBob = await client.getBalance(COOP_ID, bob);
  const newCarol = await client.getBalance(COOP_ID, carol);

  console.log(`Alice: ${newAlice.balance} hours (was ${aliceBalance.balance})`);
  console.log(`Bob:   ${newBob.balance} hours (was ${bobBalance.balance})`);
  console.log(`Carol: ${newCarol.balance} hours (was ${carolBalance.balance})`);

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

    console.log(`${date} | ${from}... -> ${to}... | ${tx.amount} ${tx.currency}`);
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
