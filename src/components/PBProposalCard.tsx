import React from 'react';
import {
  Card,
  CardContent,
  CardActions,
  Typography,
  Box,
  Button,
  Chip,
  LinearProgress,
  Divider,
  Tooltip,
  Grid,
  Avatar,
} from '@mui/material';
import AccessTimeIcon from '@mui/icons-material/AccessTime';
import AccountBalanceIcon from '@mui/icons-material/AccountBalance';
import GroupIcon from '@mui/icons-material/Group';
import HowToVoteIcon from '@mui/icons-material/HowToVote';
import CheckCircleIcon from '@mui/icons-material/CheckCircle';
import CancelIcon from '@mui/icons-material/Cancel';
import { PBProposal, ProposalStatus } from './PBProposalList';
import PBProposalTally from './PBProposalTally';

interface PBProposalCardProps {
  proposal: PBProposal;
  userDid?: string;
  onVoteClick?: (proposalId: string) => void;
}

const PBProposalCard: React.FC<PBProposalCardProps> = ({
  proposal,
  userDid,
  onVoteClick,
}) => {
  // Format currency amount
  const formatCurrency = (amount: number, currency: string): string => {
    return `${amount.toLocaleString()} ${currency}`;
  };

  // Calculate remaining time until voting ends
  const getRemainingTime = (): { text: string; percentage: number } => {
    if (proposal.status === ProposalStatus.UPCOMING) {
      const now = new Date();
      const start = new Date(proposal.timeline.votingStart);
      const diffMs = start.getTime() - now.getTime();
      
      if (diffMs <= 0) return { text: 'Starting soon', percentage: 100 };
      
      const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));
      const diffHours = Math.floor((diffMs % (1000 * 60 * 60 * 24)) / (1000 * 60 * 60));
      
      if (diffDays > 0) {
        return { text: `Starts in ${diffDays}d ${diffHours}h`, percentage: 0 };
      } else {
        return { text: `Starts in ${diffHours}h`, percentage: 0 };
      }
    }
    
    if (proposal.status === ProposalStatus.ACTIVE) {
      const now = new Date();
      const end = new Date(proposal.timeline.votingEnd);
      const start = new Date(proposal.timeline.votingStart);
      const totalDuration = end.getTime() - start.getTime();
      const elapsed = now.getTime() - start.getTime();
      
      if (elapsed <= 0) return { text: 'Just started', percentage: 0 };
      if (elapsed >= totalDuration) return { text: 'Ending soon', percentage: 100 };
      
      const diffMs = end.getTime() - now.getTime();
      const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));
      const diffHours = Math.floor((diffMs % (1000 * 60 * 60 * 24)) / (1000 * 60 * 60));
      const percentage = Math.min(100, Math.floor((elapsed / totalDuration) * 100));
      
      if (diffDays > 0) {
        return { text: `${diffDays}d ${diffHours}h remaining`, percentage };
      } else if (diffHours > 0) {
        const diffMinutes = Math.floor((diffMs % (1000 * 60 * 60)) / (1000 * 60));
        return { text: `${diffHours}h ${diffMinutes}m remaining`, percentage };
      } else {
        const diffMinutes = Math.floor((diffMs % (1000 * 60 * 60)) / (1000 * 60));
        return { text: `${diffMinutes}m remaining`, percentage };
      }
    }
    
    return { text: 'Voting closed', percentage: 100 };
  };

  // Format date for display
  const formatDate = (dateString: string): string => {
    const date = new Date(dateString);
    return date.toLocaleDateString(undefined, {
      year: 'numeric',
      month: 'short',
      day: 'numeric',
    });
  };

  // Get status chip color and label
  const getStatusChip = () => {
    switch (proposal.status) {
      case ProposalStatus.UPCOMING:
        return { color: 'default', label: 'Upcoming' };
      case ProposalStatus.ACTIVE:
        return { color: 'success', label: 'Active' };
      case ProposalStatus.CLOSED:
        if (proposal.result === 'approved') {
          return { color: 'success', label: 'Approved' };
        } else if (proposal.result === 'rejected') {
          return { color: 'error', label: 'Rejected' };
        } else {
          return { color: 'default', label: 'Closed' };
        }
      default:
        return { color: 'default', label: 'Unknown' };
    }
  };

  // Get mechanism label
  const getMechanismLabel = (mechanism: string): string => {
    return mechanism.replace('_', ' ').replace(/\b\w/g, c => c.toUpperCase());
  };

  const remainingTime = getRemainingTime();
  const statusChip = getStatusChip();
  const canVote = proposal.status === ProposalStatus.ACTIVE && userDid;
  const showTally = proposal.status !== ProposalStatus.UPCOMING;

  return (
    <Card 
      sx={{ 
        height: '100%', 
        display: 'flex', 
        flexDirection: 'column',
        '&:hover': {
          boxShadow: 4
        },
        borderLeft: proposal.status === ProposalStatus.ACTIVE 
          ? '4px solid #4caf50' 
          : proposal.result === 'approved' 
            ? '4px solid #4caf50' 
            : proposal.result === 'rejected'
              ? '4px solid #f44336'
              : undefined,
      }}
    >
      <CardContent sx={{ flexGrow: 1 }}>
        <Box sx={{ display: 'flex', justifyContent: 'space-between', mb: 2 }}>
          <Chip 
            label={statusChip.label} 
            color={statusChip.color as any} 
            size="small" 
          />
          <Chip 
            icon={<AccountBalanceIcon />}
            label={formatCurrency(proposal.treasury.available, proposal.treasury.currency)}
            size="small"
            variant="outlined"
          />
        </Box>
        
        <Typography variant="h6" component="h2" gutterBottom>
          {proposal.title}
        </Typography>
        
        <Typography variant="body2" color="text.secondary" sx={{ mb: 2, minHeight: '3em' }}>
          {proposal.description.length > 120 
            ? `${proposal.description.substring(0, 120)}...` 
            : proposal.description}
        </Typography>
        
        <Grid container spacing={2} sx={{ mb: 2 }}>
          <Grid item xs={6}>
            <Typography variant="body2" component="div" sx={{ display: 'flex', alignItems: 'center', gap: 0.5 }}>
              <GroupIcon fontSize="small" color="action" />
              <Box component="span">
                Federation:
              </Box>
            </Typography>
            <Typography variant="body2" fontWeight="medium">
              {proposal.federationName}
            </Typography>
          </Grid>
          
          <Grid item xs={6}>
            <Typography variant="body2" component="div" sx={{ display: 'flex', alignItems: 'center', gap: 0.5 }}>
              <HowToVoteIcon fontSize="small" color="action" />
              <Box component="span">
                Mechanism:
              </Box>
            </Typography>
            <Typography variant="body2" fontWeight="medium">
              {getMechanismLabel(proposal.votingMechanism)}
            </Typography>
          </Grid>
        </Grid>
        
        <Box sx={{ mb: 2 }}>
          <Typography variant="body2" component="div" sx={{ display: 'flex', alignItems: 'center', gap: 0.5, mb: 0.5 }}>
            <AccountBalanceIcon fontSize="small" color="action" />
            <Box component="span">
              Requested Amount:
            </Box>
          </Typography>
          <Typography variant="body1" fontWeight="medium">
            {formatCurrency(proposal.requestedAmount, proposal.treasury.currency)}
          </Typography>
        </Box>
        
        <Divider sx={{ my: 2 }} />
        
        {/* Timeline section */}
        <Box sx={{ mb: 2 }}>
          <Typography variant="body2" component="div" sx={{ display: 'flex', alignItems: 'center', gap: 0.5, mb: 1 }}>
            <AccessTimeIcon fontSize="small" color="action" />
            <Box component="span">
              Voting Period:
            </Box>
          </Typography>
          
          <Grid container spacing={1}>
            <Grid item xs={6}>
              <Typography variant="caption" color="text.secondary">
                Starts:
              </Typography>
              <Typography variant="body2">
                {formatDate(proposal.timeline.votingStart)}
              </Typography>
            </Grid>
            <Grid item xs={6}>
              <Typography variant="caption" color="text.secondary">
                Ends:
              </Typography>
              <Typography variant="body2">
                {formatDate(proposal.timeline.votingEnd)}
              </Typography>
            </Grid>
          </Grid>
          
          {proposal.status === ProposalStatus.ACTIVE && (
            <Box sx={{ mt: 1 }}>
              <Box sx={{ display: 'flex', justifyContent: 'space-between', mb: 0.5 }}>
                <Typography variant="body2">
                  <AccessTimeIcon fontSize="small" sx={{ verticalAlign: 'middle', mr: 0.5 }} />
                  {remainingTime.text}
                </Typography>
                <Typography variant="body2">
                  {remainingTime.percentage}%
                </Typography>
              </Box>
              <LinearProgress 
                variant="determinate" 
                value={remainingTime.percentage} 
                sx={{ height: 8, borderRadius: 1 }}
              />
            </Box>
          )}
        </Box>
        
        {/* Quorum indicator */}
        {proposal.status !== ProposalStatus.UPCOMING && (
          <Box sx={{ mb: 2 }}>
            <Box sx={{ display: 'flex', justifyContent: 'space-between', mb: 0.5 }}>
              <Typography variant="body2">
                <GroupIcon fontSize="small" sx={{ verticalAlign: 'middle', mr: 0.5 }} />
                Quorum Progress
              </Typography>
              <Typography variant="body2">
                {proposal.quorum.percentage}%
              </Typography>
            </Box>
            <LinearProgress 
              variant="determinate" 
              value={proposal.quorum.percentage} 
              color={proposal.quorum.percentage >= 100 ? "success" : "primary"}
              sx={{ height: 8, borderRadius: 1 }}
            />
            <Typography variant="caption" color="text.secondary">
              {proposal.quorum.current} of {proposal.quorum.required} required votes
            </Typography>
          </Box>
        )}
        
        {/* Vote tally section */}
        {showTally && (
          <Box sx={{ mb: 2 }}>
            <Typography variant="subtitle2" gutterBottom>
              Vote Tally
            </Typography>
            <PBProposalTally proposal={proposal} />
          </Box>
        )}
      </CardContent>
      
      <CardActions sx={{ p: 2, pt: 0 }}>
        {proposal.status === ProposalStatus.CLOSED ? (
          <Box sx={{ display: 'flex', alignItems: 'center', width: '100%' }}>
            {proposal.result === 'approved' ? (
              <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
                <CheckCircleIcon color="success" />
                <Typography variant="body2" color="success.main">
                  Proposal Approved
                </Typography>
              </Box>
            ) : (
              <Box sx={{ display: 'flex', alignItems: 'center', gap: 1 }}>
                <CancelIcon color="error" />
                <Typography variant="body2" color="error">
                  Proposal Rejected
                </Typography>
              </Box>
            )}
            
            <Box sx={{ flexGrow: 1 }} />
            
            <Button 
              size="small" 
              variant="outlined"
              onClick={() => window.open(`/proposal/${proposal.id}`, '_blank')}
            >
              View Details
            </Button>
          </Box>
        ) : (
          <>
            <Button 
              size="small" 
              variant="outlined"
              onClick={() => window.open(`/proposal/${proposal.id}`, '_blank')}
            >
              View Details
            </Button>
            
            <Box sx={{ flexGrow: 1 }} />
            
            {canVote && (
              <Button 
                size="small" 
                variant="contained" 
                color="primary"
                startIcon={<HowToVoteIcon />}
                onClick={() => onVoteClick && onVoteClick(proposal.id)}
              >
                Vote Now
              </Button>
            )}
          </>
        )}
      </CardActions>
    </Card>
  );
};

export default PBProposalCard; 