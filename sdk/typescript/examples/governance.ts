/**
 * Governance Example
 *
 * Shows how to use ICN governance features:
 * - Create governance domain
 * - Create and manage proposals
 * - Cast votes
 * - Check outcomes
 *
 * Run: npx ts-node examples/governance.ts
 */

import { ICNClient, ICNError } from '../src';

const DOMAIN_ID = 'coop:food';

async function main() {
  // Create authenticated client
  const client = new ICNClient({
    baseUrl: 'http://localhost:8080',
    token: 'your-jwt-token',
  });

  const alice = 'did:icn:alice';
  const bob = 'did:icn:bob';
  const carol = 'did:icn:carol';

  // -------------------------------------------------------------------------
  // Create Governance Domain
  // -------------------------------------------------------------------------
  console.log('=== Creating Governance Domain ===\n');

  try {
    const domain = await client.createDomain({
      domain_id: DOMAIN_ID,
      name: 'Food Cooperative Governance',
      members: [alice, bob, carol],
    });

    console.log(`Created domain: ${domain.name}`);
    console.log(`Members: ${domain.members.length}`);
  } catch (error) {
    if (error instanceof ICNError && error.statusCode === 409) {
      console.log('Domain already exists, continuing...');
    } else {
      throw error;
    }
  }

  // List existing domains
  const domains = await client.listDomains();
  console.log(`\nTotal domains: ${domains.length}`);

  // -------------------------------------------------------------------------
  // Create a Proposal
  // -------------------------------------------------------------------------
  console.log('\n=== Creating Proposal ===\n');

  const proposal = await client.createProposal({
    domain_id: DOMAIN_ID,
    title: 'Approve new supplier contract',
    description:
      'Proposal to approve a 6-month contract with Green Valley Farms ' +
      'for organic produce at $500/week.',
    kind: 'budget',
  });

  console.log(`Created proposal: ${proposal.id}`);
  console.log(`  Title: ${proposal.title}`);
  console.log(`  Kind: ${proposal.kind}`);
  console.log(`  State: ${proposal.state}`);

  // -------------------------------------------------------------------------
  // Open for Voting
  // -------------------------------------------------------------------------
  console.log('\n=== Opening for Voting ===\n');

  const openedProposal = await client.openProposal(proposal.id);
  console.log(`Proposal state: ${openedProposal.state}`);
  console.log(`Opened at: ${new Date(openedProposal.opened_at! * 1000).toLocaleString()}`);

  // -------------------------------------------------------------------------
  // Cast Votes
  // -------------------------------------------------------------------------
  console.log('\n=== Casting Votes ===\n');

  // Simulate voting from different members
  // In practice, each member would authenticate separately

  // Alice votes for
  await client.vote(proposal.id, { choice: 'for' });
  console.log('Alice voted: for');

  // For demo purposes, we'll show what other votes would look like
  // In production, Bob and Carol would authenticate and vote themselves

  console.log('Bob voted: for (simulated)');
  console.log('Carol voted: against (simulated)');

  // -------------------------------------------------------------------------
  // Check Vote Tally
  // -------------------------------------------------------------------------
  console.log('\n=== Vote Tally ===\n');

  const tally = await client.getVotes(proposal.id);

  console.log(`Proposal: ${tally.proposal_id}`);
  console.log(`  For:     ${tally.votes_for}`);
  console.log(`  Against: ${tally.votes_against}`);
  console.log(`  Abstain: ${tally.votes_abstain}`);
  console.log(`  Total:   ${tally.total_votes}`);

  console.log('\nIndividual votes:');
  for (const vote of tally.votes) {
    const voter = vote.voter.split(':').pop()?.slice(0, 8);
    console.log(`  ${voter}... : ${vote.choice}`);
  }

  // -------------------------------------------------------------------------
  // Close Proposal
  // -------------------------------------------------------------------------
  console.log('\n=== Closing Proposal ===\n');

  const outcome = await client.closeProposal(proposal.id);

  console.log('Outcome:');
  console.log(`  Accepted: ${outcome.accepted ? 'YES' : 'NO'}`);
  console.log(`  Quorum met: ${outcome.quorum_met}`);
  console.log(`  Approval met: ${outcome.approval_met}`);
  console.log(`  Final count: ${outcome.votes_for} for, ${outcome.votes_against} against`);

  if (outcome.accepted) {
    console.log('\nThe proposal passed! The supplier contract is approved.');
  } else {
    console.log('\nThe proposal did not pass.');
    if (!outcome.quorum_met) {
      console.log('Reason: Quorum was not met');
    } else if (!outcome.approval_met) {
      console.log('Reason: Did not achieve required approval threshold');
    }
  }

  // -------------------------------------------------------------------------
  // List All Proposals
  // -------------------------------------------------------------------------
  console.log('\n=== All Proposals ===\n');

  // Filter by state
  const openProposals = await client.listProposals(DOMAIN_ID, 'open');
  const closedProposals = await client.listProposals(DOMAIN_ID, 'closed');

  console.log(`Open proposals: ${openProposals.length}`);
  console.log(`Closed proposals: ${closedProposals.length}`);

  // List all
  const allProposals = await client.listProposals(DOMAIN_ID);
  console.log(`\nAll proposals in ${DOMAIN_ID}:`);

  for (const p of allProposals) {
    const status = p.state === 'closed' ? '(closed)' : p.state === 'open' ? '(voting)' : '(draft)';
    console.log(`  - ${p.title} ${status}`);
  }
}

main().catch(console.error);
