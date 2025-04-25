import React, { useState, useEffect } from 'react';
import {
  Box,
  Container,
  Typography,
  Grid,
  Tabs,
  Tab,
  FormControl,
  InputLabel,
  Select,
  MenuItem,
  CircularProgress,
  Alert,
  Paper,
  Divider,
} from '@mui/material';
import PBProposalCard from './PBProposalCard';

// Proposal status options
export enum ProposalStatus {
  UPCOMING = 'upcoming',
  ACTIVE = 'active',
  CLOSED = 'closed',
}

// Interface for proposal data structure
export interface PBProposal {
  id: string;
  title: string;
  description: string;
  federationId: string;
  federationName: string;
  treasury: {
    available: number;
    allocated: number;
    currency: string;
  };
  timeline: {
    created: string;
    votingStart: string;
    votingEnd: string;
  };
  status: ProposalStatus;
  requestedAmount: number;
  quorum: {
    required: number;
    current: number;
    percentage: number;
  };
  tally: {
    approve: number;
    reject: number;
    abstain: number;
    totalVotes: number;
    totalWeight: number;
  };
  votingMechanism: string;
  result?: 'approved' | 'rejected' | null;
  threadId?: string;
  dagAnchor?: string;
}

interface FederationOption {
  id: string;
  name: string;
}

interface PBProposalListProps {
  proposals?: PBProposal[];
  loading?: boolean;
  error?: string;
  federations?: FederationOption[];
  userDid?: string;
  onVoteClick?: (proposalId: string) => void;
}

const PBProposalList: React.FC<PBProposalListProps> = ({
  proposals = [],
  loading = false,
  error = '',
  federations = [],
  userDid = '',
  onVoteClick,
}) => {
  // State for filters
  const [selectedStatus, setSelectedStatus] = useState<number>(1); // Default to ACTIVE tab
  const [selectedFederation, setSelectedFederation] = useState<string>('all');
  
  // Map tab index to proposal status
  const getStatusFromTabIndex = (index: number): ProposalStatus => {
    switch (index) {
      case 0: return ProposalStatus.UPCOMING;
      case 1: return ProposalStatus.ACTIVE;
      case 2: return ProposalStatus.CLOSED;
      default: return ProposalStatus.ACTIVE;
    }
  };

  // Filter proposals based on selected status and federation
  const filteredProposals = proposals.filter(proposal => {
    const statusMatch = proposal.status === getStatusFromTabIndex(selectedStatus);
    const federationMatch = selectedFederation === 'all' || proposal.federationId === selectedFederation;
    return statusMatch && federationMatch;
  });

  // Handle tab change
  const handleStatusChange = (_: React.SyntheticEvent, newValue: number) => {
    setSelectedStatus(newValue);
  };

  // Handle federation change
  const handleFederationChange = (event: React.ChangeEvent<{ value: unknown }>) => {
    setSelectedFederation(event.target.value as string);
  };

  // Status labels
  const statusLabels = [
    { value: ProposalStatus.UPCOMING, label: 'Upcoming' },
    { value: ProposalStatus.ACTIVE, label: 'Active' },
    { value: ProposalStatus.CLOSED, label: 'Closed' },
  ];

  return (
    <Container maxWidth="lg">
      <Box sx={{ mb: 4 }}>
        <Typography variant="h4" component="h1" gutterBottom>
          Participatory Budgeting Proposals
        </Typography>
        <Typography variant="body1" color="text.secondary">
          View and vote on community treasury allocation proposals
        </Typography>
      </Box>

      <Paper sx={{ mb: 3, p: 0 }}>
        <Box sx={{ borderBottom: 1, borderColor: 'divider', display: 'flex', justifyContent: 'space-between', alignItems: 'center', px: 2 }}>
          <Tabs
            value={selectedStatus}
            onChange={handleStatusChange}
            aria-label="proposal status tabs"
          >
            {statusLabels.map((status, index) => (
              <Tab key={status.value} label={status.label} id={`proposal-tab-${index}`} />
            ))}
          </Tabs>
          
          <Box sx={{ minWidth: 200 }}>
            <FormControl fullWidth size="small" sx={{ my: 1 }}>
              <InputLabel id="federation-select-label">Federation</InputLabel>
              <Select
                labelId="federation-select-label"
                id="federation-select"
                value={selectedFederation}
                label="Federation"
                onChange={handleFederationChange as any}
              >
                <MenuItem value="all">All Federations</MenuItem>
                {federations.map(federation => (
                  <MenuItem key={federation.id} value={federation.id}>
                    {federation.name}
                  </MenuItem>
                ))}
              </Select>
            </FormControl>
          </Box>
        </Box>

        <Divider />

        <Box sx={{ p: 3 }}>
          {loading ? (
            <Box sx={{ display: 'flex', justifyContent: 'center', p: 4 }}>
              <CircularProgress />
            </Box>
          ) : error ? (
            <Alert severity="error" sx={{ mb: 3 }}>
              {error}
            </Alert>
          ) : filteredProposals.length === 0 ? (
            <Box sx={{ textAlign: 'center', p: 4 }}>
              <Typography variant="body1" color="text.secondary">
                No {statusLabels[selectedStatus].label.toLowerCase()} proposals found
                {selectedFederation !== 'all' ? ' for this federation' : ''}.
              </Typography>
            </Box>
          ) : (
            <Grid container spacing={3}>
              {filteredProposals.map(proposal => (
                <Grid item xs={12} md={6} lg={4} key={proposal.id}>
                  <PBProposalCard 
                    proposal={proposal}
                    userDid={userDid}
                    onVoteClick={onVoteClick}
                  />
                </Grid>
              ))}
            </Grid>
          )}
        </Box>
      </Paper>
    </Container>
  );
};

export default PBProposalList; 