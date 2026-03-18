/**
 * Demo Data Seeder for ICN Pilots
 *
 * Creates sample data to demonstrate ICN timebank functionality:
 * - Sample members with different roles
 * - Service transactions showing timebank exchanges
 * - Governance proposals demonstrating decision-making
 *
 * Usage:
 *   npx ts-node seed-demo-data.ts --gateway http://localhost:8080 --token <jwt>
 *
 * Or with environment variables:
 *   export ICN_GATEWAY=http://localhost:8080
 *   export ICN_TOKEN=<jwt>
 *   npx ts-node seed-demo-data.ts
 */

import { ICNClient } from '../src/index';

// Demo member data
const DEMO_MEMBERS = [
  { name: 'Alice', did: 'did:icn:alice-demo-001', role: 'admin', services: ['gardening', 'cooking'] },
  { name: 'Bob', did: 'did:icn:bob-demo-002', role: 'member', services: ['carpentry', 'plumbing'] },
  { name: 'Carol', did: 'did:icn:carol-demo-003', role: 'member', services: ['tutoring', 'translation'] },
  { name: 'David', did: 'did:icn:david-demo-004', role: 'member', services: ['tech-support', 'web-design'] },
  { name: 'Eve', did: 'did:icn:eve-demo-005', role: 'member', services: ['childcare', 'eldercare'] },
];

// Demo transaction data (service exchanges)
const DEMO_TRANSACTIONS = [
  { from: 'alice', to: 'bob', amount: 2.0, memo: 'Garden maintenance - 2 hours' },
  { from: 'bob', to: 'carol', amount: 1.5, memo: 'Fixed kitchen cabinet - 1.5 hours' },
  { from: 'carol', to: 'david', amount: 3.0, memo: 'Spanish tutoring sessions - 3 hours' },
  { from: 'david', to: 'eve', amount: 2.0, memo: 'Computer repair - 2 hours' },
  { from: 'eve', to: 'alice', amount: 4.0, memo: 'Childcare for weekend - 4 hours' },
  { from: 'alice', to: 'carol', amount: 1.0, memo: 'Meal preparation - 1 hour' },
  { from: 'bob', to: 'david', amount: 2.5, memo: 'Bathroom pipe repair - 2.5 hours' },
  { from: 'carol', to: 'eve', amount: 2.0, memo: 'Document translation - 2 hours' },
  { from: 'david', to: 'alice', amount: 1.5, memo: 'Website update - 1.5 hours' },
  { from: 'eve', to: 'bob', amount: 3.0, memo: 'Weekly eldercare visits - 3 hours' },
];

// Demo governance proposals
const DEMO_PROPOSALS = [
  {
    title: 'Adopt community garden project',
    description: 'Proposal to allocate 20 hours of community time to establish a shared garden space.',
    kind: 'text',
  },
  {
    title: 'New member onboarding process',
    description: 'Implement a buddy system where experienced members mentor newcomers for their first month.',
    kind: 'text',
  },
  {
    title: 'Quarterly community gathering',
    description: 'Host quarterly potluck dinners to strengthen community bonds and share skills.',
    kind: 'text',
  },
];

async function seedDemoData() {
  // Parse command line arguments
  const args = process.argv.slice(2);
  let gateway = process.env.ICN_GATEWAY || 'http://localhost:8080';
  let token = process.env.ICN_TOKEN || '';
  let coopId = process.env.ICN_COOP_ID || 'demo-timebank';
  let domainId = 'demo-timebank-governance';

  for (let i = 0; i < args.length; i++) {
    if (args[i] === '--gateway' && args[i + 1]) {
      gateway = args[++i];
    } else if (args[i] === '--token' && args[i + 1]) {
      token = args[++i];
    } else if (args[i] === '--coop' && args[i + 1]) {
      coopId = args[++i];
    } else if (args[i] === '--help') {
      console.log(`
ICN Demo Data Seeder

Usage: npx ts-node seed-demo-data.ts [options]

Options:
  --gateway <url>   Gateway URL (default: http://localhost:8080)
  --token <jwt>     JWT authentication token
  --coop <id>       Cooperative ID (default: demo-timebank)
  --help            Show this help message

Environment variables:
  ICN_GATEWAY       Gateway URL
  ICN_TOKEN         JWT token
  ICN_COOP_ID       Cooperative ID
`);
      process.exit(0);
    }
  }

  if (!token) {
    console.error('Error: JWT token required');
    console.error('Use --token <jwt> or set ICN_TOKEN environment variable');
    console.error('Get a token with: icnctl auth token --coop <coop-id>');
    process.exit(1);
  }

  const client = new ICNClient({ baseUrl: gateway, token });

  console.log('ICN Demo Data Seeder');
  console.log('====================');
  console.log(`Gateway: ${gateway}`);
  console.log(`Cooperative: ${coopId}`);
  console.log();

  // Step 1: Create cooperative if it doesn't exist
  console.log('Step 1: Setting up cooperative...');
  try {
    await client.getCoop(coopId);
    console.log(`  Cooperative "${coopId}" already exists`);
  } catch (e) {
    try {
      await client.createCoop({
        id: coopId,
        name: `Demo Timebank - ${coopId}`,
        settings: { currency: 'hours' },
      });
      console.log(`  Created cooperative "${coopId}"`);
    } catch (createError: any) {
      if (createError.message?.includes('already exists')) {
        console.log(`  Cooperative "${coopId}" already exists`);
      } else {
        throw createError;
      }
    }
  }

  // Step 2: Add members
  console.log('\nStep 2: Adding demo members...');
  const memberDids: Record<string, string> = {};

  for (const member of DEMO_MEMBERS) {
    try {
      await client.addMember(coopId, {
        did: member.did,
        role: member.role as 'owner' | 'admin' | 'member',
      });
      console.log(`  Added ${member.name} (${member.role})`);
      memberDids[member.name.toLowerCase()] = member.did;
    } catch (e: any) {
      if (e.message?.includes('already') || e.statusCode === 409) {
        console.log(`  ${member.name} already exists`);
        memberDids[member.name.toLowerCase()] = member.did;
      } else {
        console.error(`  Failed to add ${member.name}: ${e.message}`);
      }
    }
  }

  // Step 3: Create transactions
  console.log('\nStep 3: Creating demo transactions...');
  let txCount = 0;

  for (const tx of DEMO_TRANSACTIONS) {
    const fromDid = memberDids[tx.from];
    const toDid = memberDids[tx.to];

    if (!fromDid || !toDid) {
      console.error(`  Skipping: Unknown member ${tx.from} or ${tx.to}`);
      continue;
    }

    try {
      // Note: In a real system, this would need to be signed by the payer
      // For demo purposes, we're using the admin token to create transactions
      await client.pay(coopId, {
        from: fromDid,
        to: toDid,
        amount: tx.amount,
        currency: 'hours',
        memo: tx.memo,
      });
      console.log(`  ${tx.from} -> ${tx.to}: ${tx.amount}h (${tx.memo.substring(0, 30)}...)`);
      txCount++;
    } catch (e: any) {
      console.error(`  Failed: ${tx.from} -> ${tx.to}: ${e.message}`);
    }
  }
  console.log(`  Created ${txCount} transactions`);

  // Step 4: Create governance domain and proposals
  console.log('\nStep 4: Creating governance proposals...');

  // Create governance domain
  try {
    const memberList = Object.values(memberDids);
    await client.createDomain({
      domain_id: domainId,
      name: `${coopId} Governance`,
      members: memberList,
    });
    console.log(`  Created governance domain "${domainId}"`);
  } catch (e: any) {
    if (e.message?.includes('already exists') || e.statusCode === 409) {
      console.log(`  Governance domain "${domainId}" already exists`);
    } else {
      console.error(`  Failed to create governance domain: ${e.message}`);
    }
  }

  // Create proposals
  let proposalCount = 0;
  for (const proposal of DEMO_PROPOSALS) {
    try {
      const result = await client.createProposal({
        domain_id: domainId,
        title: proposal.title,
        description: proposal.description,
        kind: proposal.kind as 'text' | 'budget' | 'membership' | 'config_change',
      });
      console.log(`  Created proposal: "${proposal.title}"`);

      // Open the proposal for voting
      if (result.id) {
        await client.openProposal(result.id);
        console.log(`    -> Opened for voting`);
      }
      proposalCount++;
    } catch (e: any) {
      console.error(`  Failed to create proposal "${proposal.title}": ${e.message}`);
    }
  }
  console.log(`  Created ${proposalCount} proposals`);

  // Summary
  console.log('\n====================');
  console.log('Demo data seeding complete!');
  console.log(`  Members: ${DEMO_MEMBERS.length}`);
  console.log(`  Transactions: ${txCount}`);
  console.log(`  Proposals: ${proposalCount}`);
  console.log('\nYou can now:');
  console.log(`  - View balances: GET ${gateway}/v1/ledger/${coopId}/balance/:did`);
  console.log(`  - View history: GET ${gateway}/v1/ledger/${coopId}/history`);
  console.log(`  - Vote on proposals in the web UI`);
  console.log(`  - Connect via WebSocket for real-time updates`);
}

// Run the seeder
seedDemoData().catch((error) => {
  console.error('Error:', error.message);
  process.exit(1);
});
