import React from 'react';
import { FederationQuorumDashboard } from './FederationQuorumDashboard';
import { WalletCredential } from '../../packages/credential-utils/types';
import { FederationManifest } from '../../packages/credential-utils/types/federation';

/**
 * Example component that demonstrates the FederationQuorumDashboard with sample data
 */
export const FederationQuorumExample: React.FC = () => {
  // Sample federation manifests
  const federationManifests: Record<string, FederationManifest> = {
    'fed-gov-123': {
      federation_id: 'fed-gov-123',
      name: 'Governance Federation',
      members: {
        'did:icn:member1': { role: 'Admin', weight: 5, voting_power: 5, can_veto: true },
        'did:icn:member2': { role: 'Guardian', weight: 3, voting_power: 3 },
        'did:icn:member3': { role: 'Guardian', weight: 3, voting_power: 3 },
        'did:icn:member4': { role: 'Member', weight: 1, voting_power: 1 },
        'did:icn:member5': { role: 'Member', weight: 1, voting_power: 1 }
      },
      quorum_rules: {
        policy_type: 'Weighted',
        min_participants: 3,
        min_approvals: 3,
        threshold_percentage: 60
      },
      created: '2023-01-01T00:00:00Z',
      version: 1,
      description: 'A federation for governance decisions',
      health_metrics: {
        overall_health: 85,
        metrics: { uptime: 98, responsiveness: 85 },
        warnings: [],
        recommendations: []
      }
    },
    'fed-eco-456': {
      federation_id: 'fed-eco-456',
      name: 'Economic Federation',
      members: {
        'did:icn:eco1': { role: 'Admin', weight: 4, voting_power: 4 },
        'did:icn:eco2': { role: 'Member', weight: 2, voting_power: 2 },
        'did:icn:eco3': { role: 'Member', weight: 2, voting_power: 2 },
        'did:icn:eco4': { role: 'Member', weight: 1, voting_power: 1 }
      },
      quorum_rules: {
        policy_type: 'Majority',
        min_participants: 2,
        min_approvals: 3,
        threshold_percentage: 51
      },
      created: '2023-02-01T00:00:00Z',
      version: 1,
      description: 'A federation for economic decisions',
      health_metrics: {
        overall_health: 90,
        metrics: { uptime: 99, responsiveness: 90 },
        warnings: [],
        recommendations: []
      }
    }
  };
  
  // Sample credentials including federation reports
  const credentials: WalletCredential[] = [
    // Governance Federation Credentials
    {
      id: 'cred-123',
      title: 'Proposal Submission',
      type: 'proposal',
      issuer: {
        did: 'did:icn:member1',
        name: 'Admin Member'
      },
      subjectDid: 'did:icn:user1',
      issuanceDate: '2023-03-01T12:00:00Z',
      credentialSubject: {
        id: 'did:icn:user1',
        proposalId: 'prop-abc'
      },
      metadata: {
        federation: {
          id: 'fed-gov-123',
          name: 'Governance Federation'
        }
      }
    },
    {
      id: 'cred-124',
      title: 'Vote on Proposal',
      type: 'vote',
      issuer: {
        did: 'did:icn:member2',
        name: 'Guardian Member'
      },
      subjectDid: 'did:icn:user1',
      issuanceDate: '2023-03-02T14:00:00Z',
      credentialSubject: {
        id: 'did:icn:user1',
        proposalId: 'prop-abc',
        parentCredentialId: 'cred-123'
      },
      metadata: {
        federation: {
          id: 'fed-gov-123',
          name: 'Governance Federation'
        }
      }
    },
    {
      id: 'cred-125',
      title: 'Finalization',
      type: 'finalization',
      issuer: {
        did: 'did:icn:member1',
        name: 'Admin Member'
      },
      subjectDid: 'did:icn:user1',
      issuanceDate: '2023-03-05T10:00:00Z',
      credentialSubject: {
        id: 'did:icn:user1',
        proposalId: 'prop-abc',
        parentCredentialId: 'cred-124'
      },
      metadata: {
        federation: {
          id: 'fed-gov-123',
          name: 'Governance Federation'
        }
      }
    },
    
    // Economic Federation Credentials
    {
      id: 'cred-456',
      title: 'Economic Proposal',
      type: 'proposal',
      issuer: {
        did: 'did:icn:eco1',
        name: 'Eco Admin'
      },
      subjectDid: 'did:icn:user2',
      issuanceDate: '2023-04-01T09:00:00Z',
      credentialSubject: {
        id: 'did:icn:user2',
        proposalId: 'eco-prop-123'
      },
      metadata: {
        federation: {
          id: 'fed-eco-456',
          name: 'Economic Federation'
        }
      }
    },
    
    // Federation Report with Full Quorum (Governance Federation)
    {
      id: 'report-gov-full',
      title: 'Governance Federation Report',
      type: ['VerifiablePresentation', 'FederationReport', 'MultiSignedCredential'],
      issuer: {
        did: 'did:icn:federation/fed-gov-123',
        name: 'Governance Federation'
      },
      subjectDid: 'did:icn:user1',
      issuanceDate: '2023-03-10T15:00:00Z',
      expirationDate: '2023-06-10T15:00:00Z',
      credentialSubject: {
        id: 'did:icn:user1',
        parentCredentialId: ['cred-123', 'cred-124', 'cred-125']
      },
      metadata: {
        federation: {
          id: 'fed-gov-123',
          name: 'Governance Federation'
        },
        federationMetadata: {
          federation_id: 'fed-gov-123',
          name: 'Governance Federation',
          issuanceDate: '2023-03-10T15:00:00Z',
          totalCredentials: 3,
          quorum_policy: 'Weighted'
        }
      },
      proof: {
        type: 'Ed25519Signature2020',
        created: '2023-03-10T15:00:00Z',
        verificationMethod: 'did:icn:federation/fed-gov-123#controller',
        proofPurpose: 'assertionMethod',
        jws: 'eyJhbGciOiJFZERTQSIsInR5cCI6IkpXVCIsImtpZCI6ImRpZDppY246ZmVkZXJhdGlvbi9mZWQtZ292LTEyMyNrZXlzLTEifQ.eyJjb250ZXh0IjpbImh0dHBzOi8vd3d3LnczLm9yZy8yMDE4L2NyZWRlbnRpYWxzL3YxIl0sImlkIjoicmVwb3J0LWdvdi1mdWxsIn0.c2lnLWRpZDppY246ZmVkZXJhdGlvbi9mZWQtZ292LTEyMy0xNjc4NDY2NDAwMDAw'
      },
      // Add multi-signature proof
      multiSignatureProof: {
        type: 'Ed25519MultisignatureQuorum2023',
        created: '2023-03-10T15:00:00Z',
        proofPurpose: 'assertionMethod',
        signatures: [
          {
            verificationMethod: 'did:icn:federation/fed-gov-123#controller',
            created: '2023-03-10T15:00:00Z',
            jws: 'eyJhbGciOiJFZERTQSIsInR5cCI6IkpXVCIsImtpZCI6ImRpZDppY246ZmVkZXJhdGlvbi9mZWQtZ292LTEyMyNrZXlzLTEifQ.eyJjb250ZXh0IjpbImh0dHBzOi8vd3d3LnczLm9yZy8yMDE4L2NyZWRlbnRpYWxzL3YxIl0sImlkIjoicmVwb3J0LWdvdi1mdWxsIn0.c2lnLWRpZDppY246ZmVkZXJhdGlvbi9mZWQtZ292LTEyMy0xNjc4NDY2NDAwMDAw'
          },
          {
            verificationMethod: 'did:icn:member1#keys-1',
            created: '2023-03-10T15:01:00Z',
            jws: 'eyJhbGciOiJFZERTQSIsInR5cCI6IkpXVCIsImtpZCI6ImRpZDppY246bWVtYmVyMSNrZXlzLTEifQ.eyJjb250ZXh0IjpbImh0dHBzOi8vd3d3LnczLm9yZy8yMDE4L2NyZWRlbnRpYWxzL3YxIl0sImlkIjoicmVwb3J0LWdvdi1mdWxsIn0.c2lnLWRpZDppY246bWVtYmVyMS0xNjc4NDY2NDYwMDAw'
          },
          {
            verificationMethod: 'did:icn:member2#keys-1',
            created: '2023-03-10T15:02:00Z',
            jws: 'eyJhbGciOiJFZERTQSIsInR5cCI6IkpXVCIsImtpZCI6ImRpZDppY246bWVtYmVyMiNrZXlzLTEifQ.eyJjb250ZXh0IjpbImh0dHBzOi8vd3d3LnczLm9yZy8yMDE4L2NyZWRlbnRpYWxzL3YxIl0sImlkIjoicmVwb3J0LWdvdi1mdWxsIn0.c2lnLWRpZDppY246bWVtYmVyMi0xNjc4NDY2NTIwMDAw'
          },
          {
            verificationMethod: 'did:icn:member3#keys-1',
            created: '2023-03-10T15:03:00Z',
            jws: 'eyJhbGciOiJFZERTQSIsInR5cCI6IkpXVCIsImtpZCI6ImRpZDppY246bWVtYmVyMyNrZXlzLTEifQ.eyJjb250ZXh0IjpbImh0dHBzOi8vd3d3LnczLm9yZy8yMDE4L2NyZWRlbnRpYWxzL3YxIl0sImlkIjoicmVwb3J0LWdvdi1mdWxsIn0.c2lnLWRpZDppY246bWVtYmVyMy0xNjc4NDY2NTgwMDAw'
          }
        ]
      }
    },
    
    // Federation Report with Partial Quorum (Economic Federation)
    {
      id: 'report-eco-partial',
      title: 'Economic Federation Report',
      type: ['VerifiablePresentation', 'FederationReport', 'MultiSignedCredential'],
      issuer: {
        did: 'did:icn:federation/fed-eco-456',
        name: 'Economic Federation'
      },
      subjectDid: 'did:icn:user2',
      issuanceDate: '2023-04-10T11:00:00Z',
      expirationDate: '2023-07-10T11:00:00Z',
      credentialSubject: {
        id: 'did:icn:user2',
        parentCredentialId: ['cred-456']
      },
      metadata: {
        federation: {
          id: 'fed-eco-456',
          name: 'Economic Federation'
        },
        federationMetadata: {
          federation_id: 'fed-eco-456',
          name: 'Economic Federation',
          issuanceDate: '2023-04-10T11:00:00Z',
          totalCredentials: 1,
          quorum_policy: 'Majority'
        }
      },
      proof: {
        type: 'Ed25519Signature2020',
        created: '2023-04-10T11:00:00Z',
        verificationMethod: 'did:icn:federation/fed-eco-456#controller',
        proofPurpose: 'assertionMethod',
        jws: 'eyJhbGciOiJFZERTQSIsInR5cCI6IkpXVCIsImtpZCI6ImRpZDppY246ZmVkZXJhdGlvbi9mZWQtZWNvLTQ1NiNrZXlzLTEifQ.eyJjb250ZXh0IjpbImh0dHBzOi8vd3d3LnczLm9yZy8yMDE4L2NyZWRlbnRpYWxzL3YxIl0sImlkIjoicmVwb3J0LWVjby1wYXJ0aWFsIn0.c2lnLWRpZDppY246ZmVkZXJhdGlvbi9mZWQtZWNvLTQ1Ni0xNjgxMTE3MjAwMDAw'
      },
      // Add multi-signature proof (partial - only 2 signatures)
      multiSignatureProof: {
        type: 'Ed25519MultisignatureQuorum2023',
        created: '2023-04-10T11:00:00Z',
        proofPurpose: 'assertionMethod',
        signatures: [
          {
            verificationMethod: 'did:icn:federation/fed-eco-456#controller',
            created: '2023-04-10T11:00:00Z',
            jws: 'eyJhbGciOiJFZERTQSIsInR5cCI6IkpXVCIsImtpZCI6ImRpZDppY246ZmVkZXJhdGlvbi9mZWQtZWNvLTQ1NiNrZXlzLTEifQ.eyJjb250ZXh0IjpbImh0dHBzOi8vd3d3LnczLm9yZy8yMDE4L2NyZWRlbnRpYWxzL3YxIl0sImlkIjoicmVwb3J0LWVjby1wYXJ0aWFsIn0.c2lnLWRpZDppY246ZmVkZXJhdGlvbi9mZWQtZWNvLTQ1Ni0xNjgxMTE3MjAwMDAw'
          },
          {
            verificationMethod: 'did:icn:eco1#keys-1',
            created: '2023-04-10T11:01:00Z',
            jws: 'eyJhbGciOiJFZERTQSIsInR5cCI6IkpXVCIsImtpZCI6ImRpZDppY246ZWNvMSNrZXlzLTEifQ.eyJjb250ZXh0IjpbImh0dHBzOi8vd3d3LnczLm9yZy8yMDE4L2NyZWRlbnRpYWxzL3YxIl0sImlkIjoicmVwb3J0LWVjby1wYXJ0aWFsIn0.c2lnLWRpZDppY246ZWNvMS0xNjgxMTE3MjYwMDAw'
          }
        ]
      }
    }
  ];

  return (
    <div style={{ padding: '20px' }}>
      <div style={{ marginBottom: '30px' }}>
        <h1>Federation Quorum Validation Visualization</h1>
        <p>
          This example shows how federation-signed reports can be visualized along with their
          quorum validation status. The visualization includes information about who signed
          the reports and whether quorum requirements were met.
        </p>
        <p>
          In this example:
        </p>
        <ul>
          <li>The <strong>Governance Federation</strong> report has a fully satisfied quorum (shown in green)</li>
          <li>The <strong>Economic Federation</strong> report has a partial quorum that doesn't meet requirements (shown in yellow)</li>
        </ul>
      </div>
      
      <FederationQuorumDashboard
        credentials={credentials}
        federationManifests={federationManifests}
        width={1000}
        height={800}
      />
    </div>
  );
};

export default FederationQuorumExample; 