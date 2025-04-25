import React, { useState } from 'react';
import {
  Container,
  Typography,
  Grid,
  Paper,
  Box,
  Tabs,
  Tab,
  Divider,
} from '@mui/material';
import { PBVoteReceiptView } from '../components/PBVoteReceiptView';

// Mock data for PB vote receipts
const mockVoteReceipts = [
  {
    id: 'vc-pb-vote-001',
    type: ['VerifiableCredential', 'ParticipatoryBudgetingVote'],
    title: 'Participatory Budgeting Vote',
    issuer: {
      id: 'did:icn:federation-123',
      name: 'Community Development Federation',
      did: 'did:icn:federation-123',
    },
    issuanceDate: '2023-09-15T10:30:45Z',
    expirationDate: '2028-09-15T10:30:45Z',
    trustLevel: 'Verified',
    credentialSubject: {
      id: 'did:icn:user-456',
      proposalId: 'prop-001',
      proposalTitle: 'Community Garden Improvement',
      proposalDescription: 'Funding to expand the community garden with new vegetable beds, a greenhouse, and irrigation system improvements.',
      voteChoice: 'approve',
      voteWeight: 3,
      votingMechanism: 'quadratic',
      federationId: 'community-development',
      requestedAmount: 5000,
      tokenType: 'icn:funds/community',
      comment: 'This project will have a significant positive impact on our community\'s food security and environmental education programs.'
    },
    metadata: {
      federation: {
        id: 'community-development',
        name: 'Community Development Federation'
      },
      credentialType: 'pbVote',
      dagAnchor: '0x7f83b1657ff1fc53b92dc18148a1d65dfc2d4b1fa3d677284addd200126d9069',
      agoranet: {
        threadUrl: 'https://agoranet.example/threads/community-garden-improvement'
      }
    },
    proof: {
      type: 'Ed25519Signature2020',
      created: '2023-09-15T10:30:45Z',
      verificationMethod: 'did:icn:federation-123#keys-1',
      proofPurpose: 'assertionMethod',
      jws: 'eyJhbGciOiJFZERTQSIsImI2NCI6ZmFsc2UsImNyaXQiOlsiYjY0Il19..AQ7eUBbC1BD0e9Z',
      merkleRoot: '0x7f83b1657ff1fc53b92dc18148a1d65dfc2d4b1fa3d677284addd200126d9069'
    },
    tags: ['vote', 'community-development', 'infrastructure']
  },
  {
    id: 'vc-pb-vote-002',
    type: ['VerifiableCredential', 'ParticipatoryBudgetingVote'],
    title: 'Participatory Budgeting Vote',
    issuer: {
      id: 'did:icn:federation-789',
      name: 'Education Initiatives Federation',
      did: 'did:icn:federation-789',
    },
    issuanceDate: '2023-09-20T14:22:10Z',
    expirationDate: '2028-09-20T14:22:10Z',
    trustLevel: 'Verified',
    credentialSubject: {
      id: 'did:icn:user-456',
      proposalId: 'prop-002',
      proposalTitle: 'Youth Education Program',
      proposalDescription: 'Support for coding workshops, STEM education, and after-school programs for local youth.',
      voteChoice: 'approve',
      voteWeight: 5,
      votingMechanism: 'quadratic',
      federationId: 'education-initiatives',
      requestedAmount: 7500,
      tokenType: 'icn:funds/community'
    },
    metadata: {
      federation: {
        id: 'education-initiatives',
        name: 'Education Initiatives Federation'
      },
      credentialType: 'pbVote',
      dagAnchor: '0x91ba0e068a1f2785d0a01f7d51d098497ed77e42ee7e5c5728558a87b1bd3326',
      agoranet: {
        threadUrl: 'https://agoranet.example/threads/youth-education-program'
      }
    },
    proof: {
      type: 'Ed25519Signature2020',
      created: '2023-09-20T14:22:10Z',
      verificationMethod: 'did:icn:federation-789#keys-1',
      proofPurpose: 'assertionMethod',
      jws: 'eyJhbGciOiJFZERTQSIsImI2NCI6ZmFsc2UsImNyaXQiOlsiYjY0Il19..BQ9eUCaD2CE1f0A',
      merkleRoot: '0x91ba0e068a1f2785d0a01f7d51d098497ed77e42ee7e5c5728558a87b1bd3326'
    },
    tags: ['vote', 'education', 'youth']
  },
  {
    id: 'vc-pb-vote-003',
    type: ['VerifiableCredential', 'ParticipatoryBudgetingVote'],
    title: 'Participatory Budgeting Vote',
    issuer: {
      id: 'did:icn:federation-321',
      name: 'Cultural Programs Federation',
      did: 'did:icn:federation-321',
    },
    issuanceDate: '2023-10-05T09:15:30Z',
    expirationDate: '2028-10-05T09:15:30Z',
    trustLevel: 'Verified',
    credentialSubject: {
      id: 'did:icn:user-456',
      proposalId: 'prop-003',
      proposalTitle: 'Local Artist Showcase',
      proposalDescription: 'Funding for a quarterly art exhibition featuring local artists, including venue rental, promotion, and artist stipends.',
      voteChoice: 'reject',
      voteWeight: 1,
      votingMechanism: 'simple_majority',
      federationId: 'cultural-programs',
      requestedAmount: 3000,
      tokenType: 'icn:funds/community',
      comment: 'I believe we should prioritize other community needs at this time.'
    },
    metadata: {
      federation: {
        id: 'cultural-programs',
        name: 'Cultural Programs Federation'
      },
      credentialType: 'pbVote',
      dagAnchor: '0x3a2e38d2d5e7f67bd44b93a82772728f23bd315a883a321fe81637c116147f75',
      agoranet: {
        threadUrl: 'https://agoranet.example/threads/local-artist-showcase'
      }
    },
    proof: {
      type: 'Ed25519Signature2020',
      created: '2023-10-05T09:15:30Z',
      verificationMethod: 'did:icn:federation-321#keys-1',
      proofPurpose: 'assertionMethod',
      jws: 'eyJhbGciOiJFZERTQSIsImI2NCI6ZmFsc2UsImNyaXQiOlsiYjY0Il19..CQ0fVDaE3DF2g1B',
      merkleRoot: '0x3a2e38d2d5e7f67bd44b93a82772728f23bd315a883a321fe81637c116147f75'
    },
    tags: ['vote', 'culture', 'arts']
  },
  {
    id: 'vc-pb-vote-004',
    type: ['VerifiableCredential', 'ParticipatoryBudgetingVote'],
    title: 'Participatory Budgeting Vote',
    issuer: {
      id: 'did:icn:federation-654',
      name: 'Disaster Response Federation',
      did: 'did:icn:federation-654',
    },
    issuanceDate: '2023-10-12T16:45:22Z',
    expirationDate: '2028-10-12T16:45:22Z',
    trustLevel: 'Verified',
    credentialSubject: {
      id: 'did:icn:user-456',
      proposalId: 'prop-004',
      proposalTitle: 'Emergency Food Distribution',
      proposalDescription: 'Fund for emergency food supplies and distribution to vulnerable community members during crisis situations.',
      voteChoice: 'approve',
      voteWeight: 1,
      votingMechanism: 'consensus',
      federationId: 'disaster-response',
      requestedAmount: 10000,
      tokenType: 'icn:asset/stablecoin_proxy'
    },
    metadata: {
      federation: {
        id: 'disaster-response',
        name: 'Disaster Response Federation'
      },
      credentialType: 'pbVote',
      dagAnchor: '0x4a80e4252f58f7d2c486388a3bb8d3a57c7a3b93cfbcb8e985f1095530c95a18',
      agoranet: {
        threadUrl: 'https://agoranet.example/threads/emergency-food-distribution'
      }
    },
    proof: {
      type: 'Ed25519Signature2020',
      created: '2023-10-12T16:45:22Z',
      verificationMethod: 'did:icn:federation-654#keys-1',
      proofPurpose: 'assertionMethod',
      jws: 'eyJhbGciOiJFZERTQSIsImI2NCI6ZmFsc2UsImNyaXQiOlsiYjY0Il19..DQ1gWEaF4EG3h2C',
      merkleRoot: '0x4a80e4252f58f7d2c486388a3bb8d3a57c7a3b93cfbcb8e985f1095530c95a18'
    },
    tags: ['vote', 'emergency', 'food-security']
  }
];

/**
 * Example component to demonstrate the PB Vote Receipt Viewer
 */
const PBVoteReceiptExample: React.FC = () => {
  const [selectedTab, setSelectedTab] = useState(0);
  const [selectedReceiptId, setSelectedReceiptId] = useState<string | null>(null);

  // Handle tab change
  const handleTabChange = (_: React.SyntheticEvent, newValue: number) => {
    setSelectedTab(newValue);
    setSelectedReceiptId(null);
  };

  // Get selected receipt
  const selectedReceipt = selectedReceiptId 
    ? mockVoteReceipts.find(receipt => receipt.id === selectedReceiptId)
    : null;

  return (
    <Container maxWidth="lg" sx={{ mt: 4, mb: 4 }}>
      <Typography variant="h4" gutterBottom>
        Participatory Budgeting Vote Receipts
      </Typography>
      
      <Typography variant="body1" paragraph>
        View your verifiable vote receipts from participatory budgeting proposals you've voted on.
      </Typography>
      
      <Paper sx={{ mb: 3 }}>
        <Box sx={{ borderBottom: 1, borderColor: 'divider' }}>
          <Tabs 
            value={selectedTab} 
            onChange={handleTabChange}
            aria-label="vote receipt tabs"
          >
            <Tab label="All Receipts" />
            <Tab label="Compact View" />
            <Tab label="Full View" />
          </Tabs>
        </Box>
        
        {selectedTab === 0 && (
          <Box p={3}>
            <Typography variant="h6" gutterBottom>
              Your Vote Receipts
            </Typography>
            
            <Grid container spacing={3}>
              {mockVoteReceipts.map(receipt => (
                <Grid item xs={12} md={6} key={receipt.id}>
                  <PBVoteReceiptView
                    credential={receipt}
                    compact={true}
                    onClick={() => setSelectedReceiptId(receipt.id)}
                  />
                </Grid>
              ))}
            </Grid>
            
            {selectedReceipt && (
              <Box mt={4}>
                <Divider sx={{ mb: 3 }} />
                <Typography variant="h6" gutterBottom>
                  Selected Receipt Details
                </Typography>
                <PBVoteReceiptView credential={selectedReceipt} />
              </Box>
            )}
          </Box>
        )}
        
        {selectedTab === 1 && (
          <Box p={3}>
            <Typography variant="h6" gutterBottom>
              Compact View
            </Typography>
            <Typography variant="body2" paragraph color="text.secondary">
              Compact view is ideal for displaying multiple receipts in lists or grids.
            </Typography>
            
            <Grid container spacing={2}>
              {mockVoteReceipts.map(receipt => (
                <Grid item xs={12} sm={6} md={4} key={receipt.id}>
                  <PBVoteReceiptView
                    credential={receipt}
                    compact={true}
                  />
                </Grid>
              ))}
            </Grid>
          </Box>
        )}
        
        {selectedTab === 2 && (
          <Box p={3}>
            <Typography variant="h6" gutterBottom>
              Full View
            </Typography>
            <Typography variant="body2" paragraph color="text.secondary">
              Full view displays all receipt details and verification information.
            </Typography>
            
            {mockVoteReceipts.map(receipt => (
              <Box key={receipt.id} mb={4}>
                <PBVoteReceiptView credential={receipt} />
              </Box>
            ))}
          </Box>
        )}
      </Paper>
    </Container>
  );
};

export default PBVoteReceiptExample; 