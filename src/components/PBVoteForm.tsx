import React, { useState } from 'react';
import {
  Box,
  Button,
  Typography,
  Paper,
  Radio,
  RadioGroup,
  FormControlLabel,
  FormControl,
  FormLabel,
  TextField,
  Divider,
  Alert,
  AlertTitle,
  CircularProgress,
  Collapse,
  Grid,
  Card,
  CardContent,
  Chip
} from '@mui/material';
import {
  ThumbUp as ThumbUpIcon,
  ThumbDown as ThumbDownIcon,
  HowToVote as HowToVoteIcon,
  Check as CheckIcon,
  Info as InfoIcon
} from '@mui/icons-material';
import { PBProposal } from './PBProposalList';

interface PBVoteFormProps {
  proposal: PBProposal;
  userVoteWeight: number;
  previousVote?: {
    choice: 'approve' | 'reject';
    reason?: string;
  };
  onSubmitVote: (choice: 'approve' | 'reject', reason?: string) => Promise<void>;
  onCancel: () => void;
  isSubmitting?: boolean;
}

const PBVoteForm: React.FC<PBVoteFormProps> = ({
  proposal,
  userVoteWeight,
  previousVote,
  onSubmitVote,
  onCancel,
  isSubmitting = false
}) => {
  const [voteChoice, setVoteChoice] = useState<'approve' | 'reject'>(
    previousVote?.choice || 'approve'
  );
  const [voteReason, setVoteReason] = useState<string>(
    previousVote?.reason || ''
  );
  const [error, setError] = useState<string | null>(null);
  const [showConfirmation, setShowConfirmation] = useState<boolean>(false);

  const handleVoteChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    setVoteChoice(event.target.value as 'approve' | 'reject');
  };

  const handleReasonChange = (event: React.ChangeEvent<HTMLInputElement>) => {
    setVoteReason(event.target.value);
  };

  const handleSubmit = async (event: React.FormEvent) => {
    event.preventDefault();
    
    if (!showConfirmation) {
      setShowConfirmation(true);
      return;
    }

    setError(null);
    
    try {
      await onSubmitVote(voteChoice, voteReason);
    } catch (err) {
      setError(err instanceof Error ? err.message : 'Failed to submit vote. Please try again.');
      setShowConfirmation(false);
    }
  };

  const handleCancelConfirmation = () => {
    setShowConfirmation(false);
  };

  // Format currency
  const formatCurrency = (amount: number, currencyCode: string) => {
    return new Intl.NumberFormat('en-US', {
      style: 'currency',
      currency: currencyCode,
      maximumFractionDigits: 0
    }).format(amount);
  };

  // Calculate time remaining
  const getTimeRemaining = (expiresAt: string) => {
    const now = new Date();
    const expiration = new Date(expiresAt);
    const diffMs = expiration.getTime() - now.getTime();
    
    if (diffMs <= 0) return 'Expired';
    
    const diffDays = Math.floor(diffMs / (1000 * 60 * 60 * 24));
    const diffHours = Math.floor((diffMs % (1000 * 60 * 60 * 24)) / (1000 * 60 * 60));
    
    if (diffDays > 0) {
      return `${diffDays} day${diffDays > 1 ? 's' : ''} remaining`;
    }
    return `${diffHours} hour${diffHours > 1 ? 's' : ''} remaining`;
  };

  return (
    <Paper elevation={2} sx={{ p: { xs: 2, sm: 3 }, borderRadius: 2 }}>
      <Typography variant="h5" gutterBottom>
        Cast Your Vote
      </Typography>
      
      <Divider sx={{ mb: 3 }} />
      
      {/* Proposal Summary */}
      <Box sx={{ mb: 3 }}>
        <Card variant="outlined" sx={{ mb: 3 }}>
          <CardContent>
            <Grid container spacing={2}>
              <Grid item xs={12}>
                <Typography variant="h6" gutterBottom>
                  {proposal.title}
                </Typography>
                <Chip 
                  label={proposal.category} 
                  size="small" 
                  sx={{ mb: 1 }} 
                />
                <Typography variant="body2" color="text.secondary" paragraph>
                  {proposal.description.length > 200 
                    ? `${proposal.description.substring(0, 200)}...` 
                    : proposal.description}
                </Typography>
              </Grid>
              
              <Grid item xs={6}>
                <Typography variant="body2" color="text.secondary">
                  Requested Amount
                </Typography>
                <Typography variant="h6" color="primary.main">
                  {formatCurrency(proposal.requestedAmount, proposal.currencyCode)}
                </Typography>
              </Grid>
              
              <Grid item xs={6}>
                <Typography variant="body2" color="text.secondary">
                  Voting Deadline
                </Typography>
                <Typography variant="body1" fontWeight="medium">
                  {getTimeRemaining(proposal.expiresAt)}
                </Typography>
              </Grid>
            </Grid>
          </CardContent>
        </Card>
      </Box>
      
      {/* Vote Weight Info */}
      <Alert severity="info" sx={{ mb: 3 }}>
        <AlertTitle>Your Vote Information</AlertTitle>
        <Typography variant="body2">
          Your vote weight: <strong>{userVoteWeight}</strong>
        </Typography>
        {previousVote && (
          <Typography variant="body2" sx={{ mt: 1 }}>
            You have already voted: <strong>{previousVote.choice === 'approve' ? 'Approve' : 'Reject'}</strong>
            {previousVote.reason && (
              <Box component="span" sx={{ display: 'block', mt: 1, fontStyle: 'italic' }}>
                "{previousVote.reason}"
              </Box>
            )}
          </Typography>
        )}
      </Alert>
      
      {error && (
        <Alert severity="error" sx={{ mb: 3 }}>
          {error}
        </Alert>
      )}
      
      {/* Vote Form */}
      <form onSubmit={handleSubmit}>
        <FormControl component="fieldset" sx={{ width: '100%', mb: 3 }}>
          <FormLabel component="legend">Your Vote</FormLabel>
          <RadioGroup
            name="vote-choice"
            value={voteChoice}
            onChange={handleVoteChange}
          >
            <Box 
              sx={{ 
                display: 'flex', 
                flexDirection: { xs: 'column', sm: 'row' }, 
                gap: 2,
                mt: 1
              }}
            >
              <Paper
                elevation={0}
                sx={{
                  p: 2,
                  border: '1px solid',
                  borderColor: voteChoice === 'approve' ? 'success.main' : 'divider',
                  borderRadius: 2,
                  flex: 1,
                  backgroundColor: voteChoice === 'approve' ? 'success.light' : 'transparent',
                  cursor: 'pointer',
                  '&:hover': {
                    borderColor: voteChoice === 'approve' ? 'success.main' : 'primary.main',
                  }
                }}
                onClick={() => setVoteChoice('approve')}
              >
                <FormControlLabel
                  value="approve"
                  control={<Radio color="success" />}
                  label={
                    <Box sx={{ display: 'flex', alignItems: 'center' }}>
                      <ThumbUpIcon color="success" sx={{ mr: 1 }} />
                      <Typography fontWeight={voteChoice === 'approve' ? 'bold' : 'normal'}>
                        Approve
                      </Typography>
                    </Box>
                  }
                  sx={{ 
                    m: 0,
                    width: '100%',
                  }}
                />
              </Paper>
              
              <Paper
                elevation={0}
                sx={{
                  p: 2,
                  border: '1px solid',
                  borderColor: voteChoice === 'reject' ? 'error.main' : 'divider',
                  borderRadius: 2,
                  flex: 1,
                  backgroundColor: voteChoice === 'reject' ? 'error.light' : 'transparent',
                  cursor: 'pointer',
                  '&:hover': {
                    borderColor: voteChoice === 'reject' ? 'error.main' : 'primary.main',
                  }
                }}
                onClick={() => setVoteChoice('reject')}
              >
                <FormControlLabel
                  value="reject"
                  control={<Radio color="error" />}
                  label={
                    <Box sx={{ display: 'flex', alignItems: 'center' }}>
                      <ThumbDownIcon color="error" sx={{ mr: 1 }} />
                      <Typography fontWeight={voteChoice === 'reject' ? 'bold' : 'normal'}>
                        Reject
                      </Typography>
                    </Box>
                  }
                  sx={{ 
                    m: 0,
                    width: '100%',
                  }}
                />
              </Paper>
            </Box>
          </RadioGroup>
        </FormControl>
        
        <TextField
          fullWidth
          label="Reason (Optional)"
          multiline
          rows={3}
          value={voteReason}
          onChange={handleReasonChange}
          placeholder="Explain why you are voting this way..."
          variant="outlined"
          sx={{ mb: 3 }}
        />
        
        {/* Confirmation Alert */}
        <Collapse in={showConfirmation}>
          <Alert 
            severity="warning" 
            sx={{ mb: 3 }}
            action={
              <Button 
                color="inherit" 
                size="small" 
                onClick={handleCancelConfirmation}
              >
                CANCEL
              </Button>
            }
          >
            <AlertTitle>Confirm Your Vote</AlertTitle>
            <Typography variant="body2">
              You are about to vote: <strong>{voteChoice === 'approve' ? 'APPROVE' : 'REJECT'}</strong> this proposal.
            </Typography>
            <Typography variant="body2" sx={{ mt: 1 }}>
              Your vote will be recorded on the blockchain and cannot be changed once submitted.
            </Typography>
          </Alert>
        </Collapse>
        
        {/* Action Buttons */}
        <Box sx={{ display: 'flex', justifyContent: 'flex-end', gap: 2 }}>
          <Button
            variant="outlined"
            onClick={onCancel}
            disabled={isSubmitting}
          >
            Cancel
          </Button>
          <Button
            type="submit"
            variant="contained"
            color={showConfirmation ? (voteChoice === 'approve' ? 'success' : 'error') : 'primary'}
            disabled={isSubmitting}
            startIcon={
              isSubmitting ? (
                <CircularProgress size={20} color="inherit" />
              ) : showConfirmation ? (
                <CheckIcon />
              ) : (
                <HowToVoteIcon />
              )
            }
          >
            {isSubmitting 
              ? 'Submitting...' 
              : showConfirmation 
                ? 'Confirm Vote' 
                : 'Cast Vote'}
          </Button>
        </Box>
      </form>
    </Paper>
  );
};

export default PBVoteForm; 