import React from 'react';
import { Box, Typography, LinearProgress, Tooltip } from '@mui/material';
import { PBProposal, ProposalStatus } from './PBProposalList';

interface PBProposalTallyProps {
  proposal: PBProposal;
}

const PBProposalTally: React.FC<PBProposalTallyProps> = ({ proposal }) => {
  // If there are no votes yet, show a placeholder
  if (!proposal.tally || Object.keys(proposal.tally).length === 0) {
    return (
      <Box sx={{ textAlign: 'center', my: 2 }}>
        <Typography variant="body2" color="text.secondary">
          No votes have been cast yet.
        </Typography>
      </Box>
    );
  }

  // Calculate total votes
  const totalVotes = Object.values(proposal.tally).reduce((sum, count) => sum + count, 0);
  
  // If no votes yet
  if (totalVotes === 0) {
    return (
      <Box sx={{ textAlign: 'center', my: 2 }}>
        <Typography variant="body2" color="text.secondary">
          No votes have been cast yet.
        </Typography>
      </Box>
    );
  }

  // Function to get color based on vote option
  const getVoteColor = (option: string): string => {
    switch (option.toLowerCase()) {
      case 'yes':
      case 'approve':
      case 'for':
        return '#4caf50'; // Green
      case 'no':
      case 'reject':
      case 'against':
        return '#f44336'; // Red
      case 'abstain':
        return '#ff9800'; // Orange
      default:
        // For numeric options (1-5 ranking, etc)
        if (!isNaN(Number(option))) {
          // Create a gradient of blues based on the number
          const value = Math.min(Number(option), 5); // Cap at 5 for the gradient
          const intensity = 55 + (value * 30); // 55-205 range for the blue component
          return `rgb(25, ${intensity}, 220)`;
        }
        return '#2196f3'; // Default blue
    }
  };

  // Sort options by vote count in descending order
  const sortedOptions = Object.entries(proposal.tally)
    .sort(([, countA], [, countB]) => countB - countA);

  // Check if it's a simple yes/no vote
  const isYesNoVote = sortedOptions.length <= 3 && 
    sortedOptions.every(([option]) => 
      ['yes', 'no', 'abstain'].includes(option.toLowerCase()));

  // Format vote count with percentage
  const formatVoteCount = (count: number): string => {
    const percentage = Math.round((count / totalVotes) * 100);
    return `${count} (${percentage}%)`;
  };

  return (
    <Box>
      {sortedOptions.map(([option, count]) => {
        const percentage = (count / totalVotes) * 100;
        const color = getVoteColor(option);
        
        // Format the option label
        const formattedOption = option.charAt(0).toUpperCase() + option.slice(1);
        
        return (
          <Box key={option} sx={{ mb: 1 }}>
            <Box sx={{ display: 'flex', justifyContent: 'space-between', mb: 0.5 }}>
              <Typography variant="body2" fontWeight={isYesNoVote ? 'medium' : 'normal'}>
                {formattedOption}
              </Typography>
              <Typography variant="body2">
                {formatVoteCount(count)}
              </Typography>
            </Box>
            <Tooltip title={`${Math.round(percentage)}% of votes`}>
              <LinearProgress
                variant="determinate"
                value={percentage}
                sx={{
                  height: isYesNoVote ? 10 : 8,
                  borderRadius: 1,
                  backgroundColor: 'rgba(0,0,0,0.1)',
                  '& .MuiLinearProgress-bar': {
                    backgroundColor: color,
                  }
                }}
              />
            </Tooltip>
          </Box>
        );
      })}
      
      <Box sx={{ mt: 1, display: 'flex', justifyContent: 'flex-end' }}>
        <Typography variant="caption" color="text.secondary">
          Total votes: {totalVotes}
        </Typography>
      </Box>
    </Box>
  );
};

export default PBProposalTally; 