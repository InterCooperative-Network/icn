import React, { useState } from 'react';
import {
  Card,
  CardContent,
  Box,
  Typography,
  Button,
  Chip,
  IconButton,
  Tooltip,
  CircularProgress,
} from '@mui/material';
import InfoIcon from '@mui/icons-material/Info';
import HowToVoteIcon from '@mui/icons-material/HowToVote';
import CalendarTodayIcon from '@mui/icons-material/CalendarToday';
import AccountBalanceWalletIcon from '@mui/icons-material/AccountBalanceWallet';
import { PBVoteModal } from './PBVoteModal';
import { useProposalStatus } from '../hooks/useProposalStatus';

// Define interfaces for component props
export interface PBProposal {
  id: string;
  title: string;
  description: string;
  requested_amount: number;
  token_type: string;
  federation_id: string;
  recipient_did: string;
  proposer_did: string;
  voting_start: number;
  voting_end: number;
  voting_mechanism: 'simple_majority' | 'quadratic' | 'consensus';
  category: string;
  min_quorum_percent: number;
  required_approval_percent: number;
  status: 'deliberation' | 'voting' | 'approved' | 'rejected' | 'allocated' | 'completed';
}

interface PBVoteCardProps {
  proposal: PBProposal;
  userDid: string;
  hasVoted?: boolean;
  onVoteComplete?: () => void;
}

/**
 * Component for displaying a Participatory Budgeting proposal card
 * with voting capabilities
 */
export const PBVoteCard: React.FC<PBVoteCardProps> = ({
  proposal,
  userDid,
  hasVoted = false,
  onVoteComplete,
}) => {
  const [voteModalOpen, setVoteModalOpen] = useState(false);
  const { statusColor, statusText, timeRemaining } = useProposalStatus(
    proposal.status,
    proposal.voting_start,
    proposal.voting_end
  );

  // Format currency amount with token type
  const formatAmount = (amount: number, tokenType: string) => {
    return `${amount.toLocaleString()} ${tokenType.split('/').pop()}`;
  };

  // Format date from timestamp
  const formatDate = (timestamp: number) => {
    return new Date(timestamp).toLocaleString();
  };

  const handleVoteClick = () => {
    setVoteModalOpen(true);
  };

  const handleVoteComplete = () => {
    setVoteModalOpen(false);
    if (onVoteComplete) {
      onVoteComplete();
    }
  };

  // Determine if user can vote
  const canVote = proposal.status === 'voting' && !hasVoted;

  return (
    <Card sx={{ mb: 2, position: 'relative', borderRadius: 2, boxShadow: 3 }}>
      <CardContent>
        <Box sx={{ display: 'flex', justifyContent: 'space-between', mb: 2 }}>
          <Typography variant="h5" component="div" noWrap>
            {proposal.title}
          </Typography>
          <Chip
            label={statusText}
            color={statusColor as any}
            size="small"
            sx={{ height: 24 }}
          />
        </Box>

        <Box sx={{ display: 'flex', mb: 2 }}>
          <Chip 
            icon={<AccountBalanceWalletIcon />} 
            label={formatAmount(proposal.requested_amount, proposal.token_type)}
            sx={{ mr: 1 }}
          />
          <Chip 
            label={proposal.category} 
            variant="outlined" 
            size="small" 
            sx={{ mr: 1 }}
          />
          <Tooltip title={proposal.voting_mechanism === 'quadratic' ? 
            "Quadratic voting: vote weight costs credits squared" : 
            proposal.voting_mechanism === 'consensus' ? 
            "Consensus voting: requires 100% approval" :
            "Simple majority voting"}>
            <Chip 
              label={proposal.voting_mechanism.replace('_', ' ')} 
              variant="outlined" 
              size="small"
            />
          </Tooltip>
        </Box>

        <Typography variant="body2" color="text.secondary" sx={{ mb: 2, maxHeight: '80px', overflow: 'hidden' }}>
          {proposal.description}
        </Typography>

        <Box sx={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', mb: 2 }}>
          <Box sx={{ display: 'flex', alignItems: 'center' }}>
            <CalendarTodayIcon fontSize="small" sx={{ mr: 0.5 }} />
            <Typography variant="body2" color="text.secondary">
              {timeRemaining}
            </Typography>
          </Box>
          
          <Typography variant="body2" color="text.secondary">
            Federation: {proposal.federation_id}
          </Typography>
        </Box>

        <Box sx={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <Box>
            <Tooltip title="Required approval threshold">
              <Chip 
                size="small" 
                label={`Approval: ${proposal.required_approval_percent}%`} 
                sx={{ mr: 1 }}
              />
            </Tooltip>
            <Tooltip title="Minimum participation required">
              <Chip 
                size="small" 
                label={`Quorum: ${proposal.min_quorum_percent}%`}
              />
            </Tooltip>
          </Box>
          
          <Button
            variant="contained"
            color="primary"
            startIcon={<HowToVoteIcon />}
            onClick={handleVoteClick}
            disabled={!canVote}
          >
            {hasVoted ? 'Already Voted' : canVote ? 'Cast Vote' : 'Voting Closed'}
          </Button>
        </Box>
      </CardContent>

      <PBVoteModal
        open={voteModalOpen}
        onClose={() => setVoteModalOpen(false)}
        proposal={proposal}
        userDid={userDid}
        onVoteComplete={handleVoteComplete}
      />
    </Card>
  );
}; 