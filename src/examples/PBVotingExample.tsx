import React, { useState, useEffect } from 'react';
import {
  Box,
  Container,
  Typography,
  Grid,
  Paper,
  Tabs,
  Tab,
  CircularProgress,
  FormControl,
  InputLabel,
  Select,
  MenuItem,
} from '@mui/material';
import { PBVoteCard, PBProposal } from '../components/PBVoteCard';
import { PBProposalPreview } from '../components/PBProposalPreview';

// Mock data for PB proposals
const mockProposals: PBProposal[] = [
  {
    id: 'prop-001',
    title: 'Community Garden Improvement',
    description: 'Funding to expand the community garden with new vegetable beds, a greenhouse, and irrigation system improvements.',
    requested_amount: 5000,
    token_type: 'icn:funds/community',
    federation_id: 'community-development',
    recipient_did: 'did:icn:12345',
    proposer_did: 'did:icn:67890',
    voting_start: Date.now() - 3 * 24 * 60 * 60 * 1000, // 3 days ago
    voting_end: Date.now() + 4 * 24 * 60 * 60 * 1000, // 4 days from now
    voting_mechanism: 'simple_majority',
    category: 'infrastructure',
    min_quorum_percent: 30,
    required_approval_percent: 51,
    status: 'voting',
  },
  {
    id: 'prop-002',
    title: 'Youth Education Program',
    description: 'Support for coding workshops, STEM education, and after-school programs for local youth.',
    requested_amount: 7500,
    token_type: 'icn:funds/community',
    federation_id: 'education-initiatives',
    recipient_did: 'did:icn:abcde',
    proposer_did: 'did:icn:fghij',
    voting_start: Date.now() - 5 * 24 * 60 * 60 * 1000, // 5 days ago
    voting_end: Date.now() + 2 * 24 * 60 * 60 * 1000, // 2 days from now
    voting_mechanism: 'quadratic',
    category: 'education',
    min_quorum_percent: 40,
    required_approval_percent: 60,
    status: 'voting',
  },
  {
    id: 'prop-003',
    title: 'Local Artist Showcase',
    description: 'Funding for a quarterly art exhibition featuring local artists, including venue rental, promotion, and artist stipends.',
    requested_amount: 3000,
    token_type: 'icn:funds/community',
    federation_id: 'cultural-programs',
    recipient_did: 'did:icn:klmno',
    proposer_did: 'did:icn:pqrst',
    voting_start: Date.now() - 10 * 24 * 60 * 60 * 1000, // 10 days ago
    voting_end: Date.now() - 3 * 24 * 60 * 60 * 1000, // 3 days ago
    voting_mechanism: 'simple_majority',
    category: 'culture',
    min_quorum_percent: 25,
    required_approval_percent: 51,
    status: 'approved',
  },
  {
    id: 'prop-004',
    title: 'Emergency Food Distribution',
    description: 'Fund for emergency food supplies and distribution to vulnerable community members during crisis situations.',
    requested_amount: 10000,
    token_type: 'icn:asset/stablecoin_proxy',
    federation_id: 'disaster-response',
    recipient_did: 'did:icn:uvwxy',
    proposer_did: 'did:icn:zabcd',
    voting_start: Date.now() - 15 * 24 * 60 * 60 * 1000, // 15 days ago
    voting_end: Date.now() - 8 * 24 * 60 * 60 * 1000, // 8 days ago
    voting_mechanism: 'consensus',
    category: 'social',
    min_quorum_percent: 75,
    required_approval_percent: 100,
    status: 'allocated',
  },
  {
    id: 'prop-005',
    title: 'Renewable Energy Installation',
    description: 'Install solar panels on the community center to reduce energy costs and environmental impact.',
    requested_amount: 12000,
    token_type: 'icn:asset/resource_credit',
    federation_id: 'sustainability',
    recipient_did: 'did:icn:efghi',
    proposer_did: 'did:icn:jklmn',
    voting_start: Date.now() + 3 * 24 * 60 * 60 * 1000, // 3 days from now
    voting_end: Date.now() + 10 * 24 * 60 * 60 * 1000, // 10 days from now
    voting_mechanism: 'quadratic',
    category: 'infrastructure',
    min_quorum_percent: 50,
    required_approval_percent: 67,
    status: 'deliberation',
  },
];

/**
 * Example component to demonstrate the PB voting UI
 */
const PBVotingExample: React.FC = () => {
  const [selectedTab, setSelectedTab] = useState(0);
  const [selectedProposal, setSelectedProposal] = useState<PBProposal | null>(null);
  const [userDid] = useState('did:icn:example-user');
  const [votedProposals, setVotedProposals] = useState<Set<string>>(new Set());
  const [federationFilter, setFederationFilter] = useState<string>('all');
  const [loading, setLoading] = useState(false);
  
  // Filter proposals based on tab and federation
  const filteredProposals = mockProposals.filter(proposal => {
    // Filter by tab
    if (selectedTab === 0) { // Active
      return ['voting', 'deliberation'].includes(proposal.status);
    } else if (selectedTab === 1) { // Completed
      return ['approved', 'rejected', 'allocated', 'completed'].includes(proposal.status);
    } else {
      return true;
    }
  }).filter(proposal => {
    // Filter by federation
    return federationFilter === 'all' || proposal.federation_id === federationFilter;
  });
  
  // Get unique federation IDs for the filter
  const federationIds = ['all', ...new Set(mockProposals.map(p => p.federation_id))];

  // Handle tab change
  const handleTabChange = (_: React.SyntheticEvent, newValue: number) => {
    setSelectedTab(newValue);
    setSelectedProposal(null);
  };

  // Handle proposal click
  const handleProposalClick = (proposal: PBProposal) => {
    setSelectedProposal(proposal);
  };

  // Handle vote completion
  const handleVoteComplete = (proposalId: string) => {
    setVotedProposals(prev => new Set(prev).add(proposalId));
  };

  // Simulate loading federation data
  useEffect(() => {
    setLoading(true);
    const timer = setTimeout(() => {
      setLoading(false);
    }, 1000);
    return () => clearTimeout(timer);
  }, [federationFilter]);

  return (
    <Container maxWidth="lg" sx={{ mt: 4, mb: 4 }}>
      <Typography variant="h4" gutterBottom>
        Participatory Budgeting
      </Typography>
      
      <Paper sx={{ p: 2, mb: 3 }}>
        <Box sx={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', mb: 2 }}>
          <Tabs value={selectedTab} onChange={handleTabChange}>
            <Tab label="Active Proposals" />
            <Tab label="Completed Proposals" />
            <Tab label="All Proposals" />
          </Tabs>
          
          <FormControl sx={{ minWidth: 200 }}>
            <InputLabel>Federation</InputLabel>
            <Select
              value={federationFilter}
              label="Federation"
              onChange={(e) => setFederationFilter(e.target.value)}
            >
              {federationIds.map(id => (
                <MenuItem key={id} value={id}>
                  {id === 'all' ? 'All Federations' : id}
                </MenuItem>
              ))}
            </Select>
          </FormControl>
        </Box>
        
        {loading ? (
          <Box sx={{ display: 'flex', justifyContent: 'center', p: 4 }}>
            <CircularProgress />
          </Box>
        ) : (
          <Grid container spacing={2}>
            <Grid item xs={12} md={4}>
              <Typography variant="h6" gutterBottom>
                Proposals ({filteredProposals.length})
              </Typography>
              
              {filteredProposals.length === 0 ? (
                <Typography variant="body2" color="text.secondary">
                  No proposals matching your filters.
                </Typography>
              ) : (
                filteredProposals.map(proposal => (
                  <PBProposalPreview
                    key={proposal.id}
                    proposal={proposal}
                    onClick={() => handleProposalClick(proposal)}
                  />
                ))
              )}
            </Grid>
            
            <Grid item xs={12} md={8}>
              {selectedProposal ? (
                <Box>
                  <Typography variant="h6" gutterBottom>
                    Selected Proposal
                  </Typography>
                  <PBVoteCard
                    proposal={selectedProposal}
                    userDid={userDid}
                    hasVoted={votedProposals.has(selectedProposal.id)}
                    onVoteComplete={() => handleVoteComplete(selectedProposal.id)}
                  />
                </Box>
              ) : (
                <Paper sx={{ p: 4, textAlign: 'center' }}>
                  <Typography variant="h6" color="text.secondary">
                    Select a proposal to view details and vote
                  </Typography>
                </Paper>
              )}
            </Grid>
          </Grid>
        )}
      </Paper>
    </Container>
  );
};

export default PBVotingExample; 