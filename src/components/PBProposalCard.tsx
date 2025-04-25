import React from 'react';
import {
  Box,
  Button,
  Card,
  CardActions,
  CardContent,
  Chip,
  Divider,
  LinearProgress,
  Typography,
  useTheme,
  Tooltip,
  Grid,
  Avatar
} from '@mui/material';
import HowToVoteIcon from '@mui/icons-material/HowToVote';
import CheckCircleIcon from '@mui/icons-material/CheckCircle';
import CancelIcon from '@mui/icons-material/Cancel';
import AccessTimeIcon from '@mui/icons-material/AccessTime';
import InfoOutlinedIcon from '@mui/icons-material/InfoOutlined';
import AccountBalanceWalletIcon from '@mui/icons-material/AccountBalanceWallet';
import ArrowForwardIcon from '@mui/icons-material/ArrowForward';
import { PBProposal, ProposalStatus } from './PBProposalList';
import { CalendarToday, Person } from '@mui/icons-material';

// Get status color based on proposal status
const getStatusColor = (status: ProposalStatus): string => {
  switch (status) {
    case 'active':
      return '#4CAF50'; // Green
    case 'approved':
      return '#2196F3'; // Blue
    case 'rejected':
      return '#F44336'; // Red
    case 'draft':
      return '#9E9E9E'; // Grey
    case 'expired':
      return '#FF9800'; // Orange
    default:
      return '#9E9E9E'; // Default grey
  }
};

interface PBProposalCardProps {
  proposal: PBProposal;
  onViewDetails: () => void;
  onVote: () => void;
}

const PBProposalCard: React.FC<PBProposalCardProps> = ({
  proposal,
  onViewDetails,
  onVote
}) => {
  const theme = useTheme();
  
  // Format currency with proper symbol
  const formatCurrency = (amount: number, currencyCode = 'USD'): string => {
    return new Intl.NumberFormat('en-US', {
      style: 'currency',
      currency: currencyCode,
      minimumFractionDigits: 0,
      maximumFractionDigits: 0
    }).format(amount);
  };
  
  // Format date to readable format
  const formatDate = (dateString: string): string => {
    const date = new Date(dateString);
    return date.toLocaleDateString('en-US', {
      year: 'numeric',
      month: 'short',
      day: 'numeric'
    });
  };
  
  // Calculate days remaining until expiration
  const getDaysRemaining = (expiresAt: string): number => {
    const now = new Date();
    const expiryDate = new Date(expiresAt);
    const diffTime = expiryDate.getTime() - now.getTime();
    const diffDays = Math.ceil(diffTime / (1000 * 60 * 60 * 24));
    return diffDays > 0 ? diffDays : 0;
  };
  
  // Calculate voting progress percentage
  const getVotingProgressPercentage = (): number => {
    if (!proposal.totalVoteWeight || !proposal.quorumRequired) return 0;
    return Math.min(
      100,
      (proposal.totalVoteWeight / proposal.quorumRequired) * 100
    );
  };
  
  // Calculate approval percentage
  const getApprovalPercentage = (): number => {
    if (!proposal.totalVoteWeight) return 0;
    return (proposal.votesApprove / proposal.totalVoteWeight) * 100;
  };
  
  // Calculate participation percentage
  const getParticipationPercentage = (): number => {
    if (!proposal.totalVoteWeight) return 0;
    const totalVotes = proposal.votesApprove + proposal.votesReject;
    return Math.round((totalVotes / proposal.totalVoteWeight) * 100);
  };
  
  // Get status color and icon
  const getStatusInfo = () => {
    switch (proposal.status) {
      case 'active':
        return {
          color: theme.palette.primary.main,
          icon: <HowToVoteIcon fontSize="small" />,
          text: 'Active'
        };
      case 'approved':
        return {
          color: theme.palette.success.main,
          icon: <CheckCircleIcon fontSize="small" />,
          text: 'Approved'
        };
      case 'rejected':
        return {
          color: theme.palette.error.main,
          icon: <CancelIcon fontSize="small" />,
          text: 'Rejected'
        };
      case 'expired':
        return {
          color: theme.palette.warning.main,
          icon: <AccessTimeIcon fontSize="small" />,
          text: 'Expired'
        };
      case 'draft':
        return {
          color: theme.palette.text.secondary,
          icon: <InfoOutlinedIcon fontSize="small" />,
          text: 'Draft'
        };
      case 'canceled':
        return {
          color: theme.palette.text.disabled,
          icon: <CancelIcon fontSize="small" />,
          text: 'Canceled'
        };
      default:
        return {
          color: theme.palette.text.primary,
          icon: <InfoOutlinedIcon fontSize="small" />,
          text: proposal.status.charAt(0).toUpperCase() + proposal.status.slice(1)
        };
    }
  };
  
  const statusInfo = getStatusInfo();
  const daysRemaining = getDaysRemaining(proposal.expiresAt);
  const votingProgressPercentage = getVotingProgressPercentage();
  const approvalPercentage = getApprovalPercentage();
  const participationPercentage = getParticipationPercentage();
  const isActive = proposal.status === 'active';
  
  // Calculate time remaining
  const getTimeRemaining = (expiresAt: string) => {
    const now = new Date();
    const expiration = new Date(expiresAt);
    const diffMs = expiration.getTime() - now.getTime();
    
    if (diffMs <= 0) return 'Expired';
    
    const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));
    const diffHours = Math.floor((diffMs % (1000 * 60 * 60 * 24)) / (1000 * 60 * 60));
    
    if (diffDays > 0) {
      return `${diffDays} day${diffDays > 1 ? 's' : ''} left`;
    }
    return `${diffHours} hour${diffHours > 1 ? 's' : ''} left`;
  };
  
  return (
    <Card sx={{ 
      height: '100%',
      display: 'flex', 
      flexDirection: 'column',
      transition: 'transform 0.2s, box-shadow 0.2s',
      '&:hover': {
        transform: 'translateY(-4px)',
        boxShadow: 4
      }
    }}>
      <CardContent sx={{ flexGrow: 1 }}>
        <Box sx={{ display: 'flex', justifyContent: 'space-between', mb: 1 }}>
          <Chip 
            label={proposal.status.charAt(0).toUpperCase() + proposal.status.slice(1)} 
            color={getStatusColor(proposal.status)}
            size="small"
          />
          <Chip 
            label={proposal.category} 
            variant="outlined"
            size="small"
          />
        </Box>
        
        <Typography variant="h6" component="div" sx={{ mb: 1, fontWeight: 'bold' }}>
          {proposal.title}
        </Typography>
        
        <Typography variant="body2" color="text.secondary" sx={{ 
          mb: 2,
          overflow: 'hidden',
          textOverflow: 'ellipsis',
          display: '-webkit-box',
          WebkitLineClamp: 3,
          WebkitBoxOrient: 'vertical'
        }}>
          {proposal.description}
        </Typography>
        
        <Divider sx={{ my: 1.5 }} />
        
        <Box sx={{ display: 'flex', alignItems: 'center', mb: 1 }}>
          <AccountBalanceWalletIcon fontSize="small" color="primary" sx={{ mr: 1 }} />
          <Typography variant="body1" fontWeight="medium">
            {formatCurrency(proposal.requestedAmount)}
          </Typography>
        </Box>
        
        <Grid container spacing={1} sx={{ mb: 2 }}>
          <Grid item xs={6}>
            <Box sx={{ display: 'flex', alignItems: 'center' }}>
              <CalendarToday fontSize="small" sx={{ mr: 0.5, color: 'text.secondary' }} />
              <Typography variant="body2">
                {isActive ? `${daysRemaining} days left` : formatDate(proposal.expiresAt)}
              </Typography>
            </Box>
          </Grid>
          <Grid item xs={6}>
            <Box sx={{ display: 'flex', alignItems: 'center' }}>
              <AccountBalanceWalletIcon fontSize="small" sx={{ mr: 0.5, color: 'text.secondary' }} />
              <Typography variant="body2" noWrap>
                {proposal.federation.name}
              </Typography>
            </Box>
          </Grid>
          <Grid item xs={6}>
            <Box sx={{ display: 'flex', alignItems: 'center' }}>
              <Person fontSize="small" sx={{ mr: 0.5, color: 'text.secondary' }} />
              <Typography variant="body2" noWrap>
                {proposal.author.name}
              </Typography>
            </Box>
          </Grid>
          <Grid item xs={6}>
            <Box sx={{ display: 'flex', alignItems: 'center' }}>
              <HowToVoteIcon fontSize="small" sx={{ mr: 0.5, color: 'text.secondary' }} />
              <Typography variant="body2">
                {participationPercentage}% voted
              </Typography>
            </Box>
          </Grid>
        </Grid>
        
        <Box sx={{ mt: 2 }}>
          <Typography variant="body2" color="text.secondary" sx={{ display: 'flex', justifyContent: 'space-between' }}>
            <span>Approval</span>
            <span>{approvalPercentage.toFixed(1)}%</span>
          </Typography>
          <LinearProgress 
            variant="determinate" 
            value={approvalPercentage}
            sx={{ height: 8, borderRadius: 4, my: 0.5 }}
          />
          <Typography variant="body2" color="text.secondary">
            Quorum: {((proposal.totalVoteWeight / proposal.quorumRequired) * 100).toFixed(1)}%
          </Typography>
        </Box>
        
        {isActive && (
          <Box>
            <Box sx={{ display: 'flex', justifyContent: 'space-between', mb: 0.5 }}>
              <Typography variant="caption" color="text.secondary">Participation</Typography>
              <Typography variant="caption" color="text.secondary">{participationPercentage}%</Typography>
            </Box>
            <LinearProgress 
              variant="determinate" 
              value={participationPercentage} 
              sx={{ 
                height: 8, 
                borderRadius: 4,
              }}
            />
            <Typography variant="caption" color="text.secondary" sx={{ display: 'block', mt: 0.5 }}>
              Quorum: {proposal.quorumRequired}%
            </Typography>
          </Box>
        )}
      </CardContent>
      
      <CardActions sx={{ p: 2, pt: 0 }}>
        <Button 
          size="small" 
          color="primary"
          onClick={onViewDetails}
          endIcon={<ArrowForwardIcon />}
        >
          View Details
        </Button>
        
        {isActive && (
          <Button 
            size="small" 
            variant="contained" 
            color="primary"
            onClick={onVote}
            startIcon={<HowToVoteIcon />}
            sx={{ ml: 'auto' }}
          >
            Vote
          </Button>
        )}
      </CardActions>
    </Card>
  );
};

export default PBProposalCard; 