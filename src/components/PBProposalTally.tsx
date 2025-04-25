import React from 'react';
import {
  Box,
  Typography,
  Paper,
  Divider,
  LinearProgress,
  Tooltip,
  Card,
  CardContent,
  Grid,
  Chip
} from '@mui/material';
import {
  CheckCircle as CheckCircleIcon,
  Cancel as CancelIcon,
  HowToVote as HowToVoteIcon,
  People as PeopleIcon
} from '@mui/icons-material';

export interface VoteOption {
  id: string;
  label: string;
  count: number;
  percentage: number;
}

export interface VoteTally {
  totalVotes: number;
  totalVoters: number;
  totalPossibleVotes: number;
  quorum: number;
  quorumReached: boolean;
  approvalThreshold: number;
  thresholdReached: boolean;
  options: VoteOption[];
  winningOption?: VoteOption;
}

interface PBProposalTallyProps {
  tally: VoteTally;
  showDetails?: boolean;
  compact?: boolean;
}

const PBProposalTally: React.FC<PBProposalTallyProps> = ({
  tally,
  showDetails = true,
  compact = false
}) => {
  const participation = tally.totalVotes > 0 
    ? (tally.totalVotes / tally.totalPossibleVotes) * 100 
    : 0;
  
  return (
    <Card elevation={compact ? 0 : 1} sx={{ borderRadius: compact ? 0 : 2 }}>
      <CardContent sx={{ p: compact ? 1 : 2 }}>
        {/* Title */}
        {!compact && (
          <Typography variant="h6" gutterBottom>
            Voting Results
          </Typography>
        )}
        
        {/* Main voting numbers */}
        <Box sx={{ mb: 2 }}>
          <Grid container spacing={2}>
            <Grid item xs={compact ? 12 : 6}>
              <Box sx={{ display: 'flex', alignItems: 'center', mb: 1 }}>
                <HowToVoteIcon fontSize="small" sx={{ mr: 1, color: 'primary.main' }} />
                <Typography variant={compact ? 'body2' : 'body1'}>
                  Total Votes: <strong>{tally.totalVotes}</strong>
                </Typography>
              </Box>
              
              {!compact && (
                <Box sx={{ display: 'flex', alignItems: 'center' }}>
                  <PeopleIcon fontSize="small" sx={{ mr: 1, color: 'primary.main' }} />
                  <Typography variant="body1">
                    Total Voters: <strong>{tally.totalVoters}</strong>
                  </Typography>
                </Box>
              )}
            </Grid>
            
            <Grid item xs={compact ? 12 : 6}>
              <Box sx={{ mb: 1 }}>
                <Box sx={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', mb: 0.5 }}>
                  <Typography variant={compact ? 'caption' : 'body2'}>
                    Participation
                  </Typography>
                  <Box sx={{ display: 'flex', alignItems: 'center' }}>
                    <Typography variant={compact ? 'caption' : 'body2'}>
                      {participation.toFixed(1)}%
                    </Typography>
                    {tally.quorumReached && (
                      <Tooltip title="Quorum reached">
                        <CheckCircleIcon 
                          fontSize="small" 
                          color="success" 
                          sx={{ ml: 0.5, fontSize: compact ? 14 : 16 }} 
                        />
                      </Tooltip>
                    )}
                  </Box>
                </Box>
                <LinearProgress 
                  variant="determinate" 
                  value={Math.min(participation, 100)} 
                  color={tally.quorumReached ? "success" : "primary"}
                  sx={{ height: compact ? 4 : 6, borderRadius: 1 }}
                />
                <Typography variant="caption" color="text.secondary" sx={{ display: 'block', mt: 0.5 }}>
                  Quorum: {tally.quorum}%
                </Typography>
              </Box>
            </Grid>
          </Grid>
        </Box>

        {/* Vote options */}
        <Divider sx={{ my: compact ? 1 : 2 }} />
        
        <Box>
          <Typography variant={compact ? 'body2' : 'subtitle1'} gutterBottom>
            Vote Distribution
          </Typography>
          
          {tally.options.map((option) => (
            <Box key={option.id} sx={{ mb: 1.5 }}>
              <Box sx={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', mb: 0.5 }}>
                <Box sx={{ display: 'flex', alignItems: 'center' }}>
                  {option.id === 'approve' ? (
                    <CheckCircleIcon 
                      fontSize="small" 
                      color="success" 
                      sx={{ mr: 0.5, fontSize: compact ? 14 : 16 }} 
                    />
                  ) : option.id === 'reject' ? (
                    <CancelIcon 
                      fontSize="small" 
                      color="error" 
                      sx={{ mr: 0.5, fontSize: compact ? 14 : 16 }} 
                    />
                  ) : null}
                  <Typography variant={compact ? 'caption' : 'body2'}>
                    {option.label}
                  </Typography>
                </Box>
                <Box sx={{ display: 'flex', alignItems: 'center' }}>
                  <Typography variant={compact ? 'caption' : 'body2'}>
                    {option.percentage.toFixed(1)}% ({option.count})
                  </Typography>
                  {tally.winningOption?.id === option.id && (
                    <Chip 
                      label="Winning" 
                      size="small" 
                      color="success" 
                      sx={{ 
                        ml: 1, 
                        height: compact ? 16 : 24, 
                        fontSize: compact ? 10 : 12 
                      }} 
                    />
                  )}
                </Box>
              </Box>
              <LinearProgress 
                variant="determinate" 
                value={Math.min(option.percentage, 100)} 
                color={tally.winningOption?.id === option.id ? "success" : "primary"}
                sx={{ 
                  height: compact ? 4 : 6, 
                  borderRadius: 1,
                  bgcolor: option.id === 'reject' ? 'error.light' : undefined,
                  '& .MuiLinearProgress-bar': {
                    bgcolor: option.id === 'reject' ? 'error.main' : undefined
                  }
                }}
              />
              
              {option.id === 'approve' && !compact && (
                <Typography variant="caption" color="text.secondary" sx={{ display: 'block', mt: 0.5 }}>
                  Threshold: {tally.approvalThreshold}%
                </Typography>
              )}
            </Box>
          ))}
        </Box>
        
        {/* Details section */}
        {showDetails && !compact && (
          <>
            <Divider sx={{ my: 2 }} />
            <Typography variant="subtitle1" gutterBottom>
              Vote Details
            </Typography>
            <Grid container spacing={1}>
              <Grid item xs={6}>
                <Paper variant="outlined" sx={{ p: 1 }}>
                  <Typography variant="caption" color="text.secondary">
                    Quorum Status
                  </Typography>
                  <Typography variant="body2">
                    {tally.quorumReached ? 'Reached' : 'Not Reached'}
                  </Typography>
                </Paper>
              </Grid>
              <Grid item xs={6}>
                <Paper variant="outlined" sx={{ p: 1 }}>
                  <Typography variant="caption" color="text.secondary">
                    Threshold Status
                  </Typography>
                  <Typography variant="body2">
                    {tally.thresholdReached ? 'Reached' : 'Not Reached'}
                  </Typography>
                </Paper>
              </Grid>
              <Grid item xs={6}>
                <Paper variant="outlined" sx={{ p: 1 }}>
                  <Typography variant="caption" color="text.secondary">
                    Total Possible Votes
                  </Typography>
                  <Typography variant="body2">
                    {tally.totalPossibleVotes}
                  </Typography>
                </Paper>
              </Grid>
              <Grid item xs={6}>
                <Paper variant="outlined" sx={{ p: 1 }}>
                  <Typography variant="caption" color="text.secondary">
                    Threshold Required
                  </Typography>
                  <Typography variant="body2">
                    {tally.approvalThreshold}%
                  </Typography>
                </Paper>
              </Grid>
            </Grid>
          </>
        )}
      </CardContent>
    </Card>
  );
};

export default PBProposalTally; 