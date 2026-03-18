/**
 * Commons Evolution Example
 *
 * Demonstrates the Commons Evolution features of ICN:
 * - Charter creation and signing
 * - Membership management
 * - Constitutional amendments
 * - Appeals process
 *
 * Run: npx ts-node examples/commons-evolution.ts
 */

import { ICNClient, ICNError } from '../src';

const DOMAIN_ID = 'coop:green-valley';

async function main() {
  // Create authenticated client
  const client = new ICNClient({
    baseUrl: 'http://localhost:8080',
    token: 'your-jwt-token',
  });

  const alice = 'did:icn:alice';
  const bob = 'did:icn:bob';
  const carol = 'did:icn:carol';

  // =========================================================================
  // PART 1: Charter Creation & Founding
  // =========================================================================
  console.log('=== Creating Cooperative Charter ===\n');

  let charter;
  try {
    charter = await client.createCharter({
      domain_id: DOMAIN_ID,
      name: 'Green Valley Food Cooperative',
      description: 'A community-owned grocery cooperative serving the valley.',
      org_type: 'cooperative',
      bootstrap_endpoints: ['https://gateway.greenvalley.coop'],
    });

    console.log(`Created charter: ${charter.charter_id}`);
    console.log(`  Name: ${charter.name}`);
    console.log(`  Status: ${charter.status}`);
    console.log(`  Founders: ${charter.founders.length}`);
  } catch (error) {
    if (error instanceof ICNError && error.statusCode === 409) {
      console.log('Charter already exists, fetching...');
      charter = await client.getCharterByDomain(DOMAIN_ID);
    } else {
      throw error;
    }
  }

  // -------------------------------------------------------------------------
  // Sign Charter as Founders
  // -------------------------------------------------------------------------
  console.log('\n=== Signing Charter ===\n');

  // Each founder signs the charter
  // In production, each founder would sign with their own private key
  const mockSignature = 'abc123...'; // Would be actual Ed25519 signature

  try {
    const signResult = await client.signCharter(charter.charter_id, mockSignature, 'founding_member');
    console.log(`Signed charter: ${signResult.total_founders} founders`);
    console.log(`Ready for activation: ${signResult.ready_for_activation}`);

    if (signResult.founders_needed > 0) {
      console.log(`Still need ${signResult.founders_needed} more founders`);
    }
  } catch (error) {
    if (error instanceof ICNError) {
      console.log(`Sign error: ${error.message}`);
    }
  }

  // -------------------------------------------------------------------------
  // Activate Charter (when enough founders)
  // -------------------------------------------------------------------------
  console.log('\n=== Activating Charter ===\n');

  try {
    const activated = await client.activateCharter(charter.charter_id);
    console.log(`Charter activated: ${activated.new_status}`);
  } catch (error) {
    if (error instanceof ICNError) {
      console.log(`Activation: ${error.message}`);
    }
  }

  // =========================================================================
  // PART 2: Membership Management
  // =========================================================================
  console.log('\n=== Membership Management ===\n');

  // Apply for membership
  try {
    const application = await client.applyForMembership(DOMAIN_ID, ['vote', 'transact']);
    console.log(`Applied for membership: ${application.holder_id}`);
    console.log(`  Status: ${application.status}`);
    console.log(`  Requested capabilities: vote, transact`);
  } catch (error) {
    if (error instanceof ICNError) {
      console.log(`Application: ${error.message}`);
    }
  }

  // List pending members (as admin)
  try {
    const members = await client.listJurisdictionMembers(DOMAIN_ID, 'candidate');
    console.log(`\nPending applications: ${members.total}`);

    for (const member of members.members) {
      console.log(`  - ${member.holder_did} (${member.status})`);
    }
  } catch (error) {
    console.log('Could not list members');
  }

  // Approve membership (as admin)
  console.log('\n--- Admin: Approving Membership ---');
  try {
    // In real usage, holderId comes from the application
    const holderId = 'holder123';
    const approval = await client.approveMembership(holderId, DOMAIN_ID);
    console.log(`Approved: ${approval.holder_id}`);
    console.log(`  New status: ${approval.new_status}`);
  } catch (error) {
    if (error instanceof ICNError) {
      console.log(`Approval: ${error.message}`);
    }
  }

  // Grant additional capability
  console.log('\n--- Admin: Granting Capability ---');
  try {
    const holderId = 'holder123';
    await client.grantCapability(holderId, DOMAIN_ID, 'propose');
    console.log('Granted propose capability');

    // Check capability
    const check = await client.checkCapability(holderId, DOMAIN_ID, 'propose');
    console.log(`Has propose capability: ${check.has_capability}`);
  } catch (error) {
    if (error instanceof ICNError) {
      console.log(`Capability: ${error.message}`);
    }
  }

  // =========================================================================
  // PART 3: Constitutional Amendments
  // =========================================================================
  console.log('\n=== Constitutional Amendments ===\n');

  // Create an amendment to change governance rules
  let amendment;
  try {
    amendment = await client.createAmendment({
      amendment_type: 'governance',
      scope_type: 'jurisdiction',
      scope_id: DOMAIN_ID,
      title: 'Lower Voting Quorum',
      description: 'Reduce the required quorum for regular proposals from 50% to 40%',
      changes: [
        {
          target: 'governance_rules',
          change_type: 'modify',
          description: 'Reduce quorum requirement',
          old_value: '50%',
          new_value: '40%',
        },
      ],
    });

    console.log(`Created amendment: ${amendment.id}`);
    console.log(`  Title: ${amendment.title}`);
    console.log(`  Status: ${amendment.status}`);
    console.log(`  Changes: ${amendment.changes.length}`);
  } catch (error) {
    if (error instanceof ICNError) {
      console.log(`Amendment creation: ${error.message}`);
    }
  }

  // Add another change to the draft
  if (amendment) {
    console.log('\n--- Adding More Changes ---');
    try {
      const updated = await client.addAmendmentChange(amendment.id, {
        target: 'governance_rules',
        change_type: 'add',
        description: 'Add expedited voting for emergencies',
        new_value: 'Emergency proposals may use 24-hour voting period',
      });
      console.log(`Updated amendment: ${updated.changes.length} changes`);
    } catch (error) {
      if (error instanceof ICNError) {
        console.log(`Add change: ${error.message}`);
      }
    }

    // Submit for review
    console.log('\n--- Submitting for Review ---');
    try {
      const submitted = await client.submitAmendment(amendment.id);
      console.log(`Amendment status: ${submitted.status}`);
    } catch (error) {
      if (error instanceof ICNError) {
        console.log(`Submit: ${error.message}`);
      }
    }
  }

  // List amendments
  console.log('\n--- All Amendments ---');
  try {
    const amendments = await client.listAmendments('draft');
    console.log(`Draft amendments: ${amendments.count}`);

    for (const a of amendments.amendments) {
      console.log(`  - ${a.title} (${a.status})`);
    }
  } catch (error) {
    console.log('Could not list amendments');
  }

  // =========================================================================
  // PART 4: Appeals Process
  // =========================================================================
  console.log('\n=== Appeals Process ===\n');

  // File an appeal against a membership decision
  let appeal;
  try {
    appeal = await client.fileAppeal({
      appeal_type: {
        category: 'membership_denial',
        target_id: DOMAIN_ID,
        details: 'Application was denied without proper review',
      },
      scope_type: 'jurisdiction',
      scope_id: DOMAIN_ID,
      grounds: [
        {
          ground_type: 'procedural_error',
          description: 'The membership committee did not meet the required review timeline',
        },
        {
          ground_type: 'rights_violation',
          description: 'Was not given opportunity to present additional documentation',
        },
      ],
      statement:
        'I applied for membership on November 1st and received a denial on November 15th ' +
        'without any explanation or opportunity to address concerns.',
      requested_remedy: 'reinstate',
      remedy_details: 'Request a new membership review with proper procedure',
    });

    console.log(`Filed appeal: ${appeal.id}`);
    console.log(`  Status: ${appeal.status}`);
    console.log(`  Grounds: ${appeal.grounds.length}`);
    console.log(`  Requested remedy: ${appeal.requested_remedy}`);
  } catch (error) {
    if (error instanceof ICNError) {
      console.log(`File appeal: ${error.message}`);
    }
  }

  // Add evidence to appeal
  if (appeal) {
    console.log('\n--- Adding Evidence ---');
    try {
      const withEvidence = await client.addAppealEvidence(appeal.id, {
        evidence_type: 'document',
        description: 'Original membership application with timestamps',
        uri: 'ipfs://QmXyz...applicationDoc',
      });
      console.log(`Evidence count: ${withEvidence.evidence_count}`);
    } catch (error) {
      if (error instanceof ICNError) {
        console.log(`Add evidence: ${error.message}`);
      }
    }

    // Add response (as respondent)
    console.log('\n--- Adding Response ---');
    try {
      const withResponse = await client.addAppealResponse(appeal.id, {
        response_type: 'initial_response',
        content:
          'The membership committee acknowledges the procedural delay. ' +
          'We propose scheduling a new review meeting.',
      });
      console.log(`Response count: ${withResponse.responses_count}`);
    } catch (error) {
      if (error instanceof ICNError) {
        console.log(`Add response: ${error.message}`);
      }
    }
  }

  // List appeals
  console.log('\n--- All Appeals ---');
  try {
    const appeals = await client.listAppeals();
    console.log(`Total appeals: ${appeals.count}`);

    for (const a of appeals.appeals) {
      console.log(`  - ${a.appeal_type} (${a.status})`);
    }
  } catch (error) {
    console.log('Could not list appeals');
  }

  // Admin: Resolve appeal
  if (appeal) {
    console.log('\n--- Admin: Resolving Appeal ---');
    try {
      // Begin review
      await client.beginAppealReview(appeal.id);
      console.log('Review begun');

      // Resolve
      const resolved = await client.resolveAppeal(appeal.id, {
        outcome: 'upheld',
        reason: 'Procedural error confirmed. Membership review timeline was not followed.',
        remedy: 'reinstate',
        remedy_details: 'New membership review scheduled for next committee meeting.',
      });
      console.log(`Appeal resolved: ${resolved.status}`);
    } catch (error) {
      if (error instanceof ICNError) {
        console.log(`Resolve: ${error.message}`);
      }
    }
  }

  // =========================================================================
  // Summary
  // =========================================================================
  console.log('\n=== Summary ===\n');
  console.log('Commons Evolution features demonstrated:');
  console.log('  1. Charter creation, signing, and activation');
  console.log('  2. Membership application, approval, and capability management');
  console.log('  3. Constitutional amendment drafting and submission');
  console.log('  4. Appeals filing, evidence, responses, and resolution');
  console.log('\nSee the ICN documentation for more details on each feature.');
}

main().catch(console.error);
