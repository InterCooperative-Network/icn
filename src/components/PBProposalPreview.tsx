import React from 'react';
import {
  Card,
  CardContent,
  Box,
  Typography,
  Chip,
  Tooltip,
  Button,
} from '@mui/material';
import AccountBalanceWalletIcon from '@mui/icons-material/AccountBalanceWallet';
import CalendarTodayIcon from '@mui/icons-material/CalendarToday';
import ArticleIcon from '@mui/icons-material/Article';
import { PBProposal } from './PBVoteCard';
import { useProposalStatus } from '../hooks/useProposalStatus';

interface PBProposalPreviewProps {
  proposal: PBProposal;
  onClick?: () => void;
}

/**
 * A compact, read-only preview of a Participatory Budgeting proposal
 * Useful for lists and grids of proposals
 */
export const PBProposalPreview: React.FC<PBProposalPreviewProps> = ({
  proposal,
  onClick,
}) => {
  const { statusColor, statusText, timeRemaining } = useProposalStatus(
    proposal.status,
    proposal.voting_start,
    proposal.voting_end
  );

  // Format currency amount with token type
  const formatAmount = (amount: number, tokenType: string) => {
    return `${amount.toLocaleString()} ${tokenType.split('/').pop()}`;
  };

  return (
    <Card 
      sx={{ 
        mb: 1, 
        cursor: onClick ? 'pointer' : 'default',
        '&:hover': {
          boxShadow: onClick ? 4 : 1,
        }
      }}
      onClick={onClick}
    >
      <CardContent sx={{ p: 2 }}>
        <Box sx={{ display: 'flex', justifyContent: 'space-between', mb: 1 }}>
          <Typography variant="h6" component="div" noWrap sx={{ maxWidth: '70%' }}>
            {proposal.title}
          </Typography>
          <Chip
            label={statusText}
            color={statusColor as any}
            size="small"
            sx={{ height: 22 }}
          />
        </Box>

        <Box sx={{ display: 'flex', mb: 1, flexWrap: 'wrap', gap: 0.5 }}>
          <Chip 
            icon={<AccountBalanceWalletIcon fontSize="small" />} 
            label={formatAmount(proposal.requested_amount, proposal.token_type)}
            size="small"
          />
          <Chip 
            label={proposal.category} 
            variant="outlined" 
            size="small" 
          />
        </Box>

        <Typography 
          variant="body2" 
          color="text.secondary" 
          sx={{ 
            mb: 1, 
            height: '40px', 
            overflow: 'hidden', 
            textOverflow: 'ellipsis', 
            display: '-webkit-box', 
            WebkitLineClamp: 2, 
            WebkitBoxOrient: 'vertical' 
          }}
        >
          {proposal.description}
        </Typography>

        <Box sx={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <Box sx={{ display: 'flex', alignItems: 'center' }}>
            <CalendarTodayIcon fontSize="small" sx={{ mr: 0.5, fontSize: '0.875rem' }} />
            <Typography variant="caption" color="text.secondary">
              {timeRemaining}
            </Typography>
          </Box>
          
          <Tooltip title={`Federation: ${proposal.federation_id}`}>
            <Typography variant="caption" color="text.secondary" noWrap sx={{ maxWidth: '120px' }}>
              {proposal.federation_id}
            </Typography>
          </Tooltip>
        </Box>
      </CardContent>
    </Card>
  );
}; 