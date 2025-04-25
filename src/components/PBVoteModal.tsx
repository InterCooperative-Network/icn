import React, { useState, useEffect } from 'react';
import {
  Dialog,
  DialogTitle,
  DialogContent,
  DialogActions,
  Button,
  Typography,
  Radio,
  RadioGroup,
  FormControlLabel,
  FormControl,
  FormLabel,
  TextField,
  Slider,
  Box,
  Alert,
  CircularProgress,
  Divider,
} from '@mui/material';
import { PBProposal } from './PBVoteCard';
import { usePBVote } from '../hooks/usePBVote';

interface PBVoteModalProps {
  open: boolean;
  onClose: () => void;
  proposal: PBProposal;
  userDid: string;
  onVoteComplete?: () => void;
}

/**
 * Modal for casting votes on participatory budgeting proposals
 */
export const PBVoteModal: React.FC<PBVoteModalProps> = ({
  open,
  onClose,
  proposal,
  userDid,
  onVoteComplete,
}) => {
  const [voteChoice, setVoteChoice] = useState<'approve' | 'reject' | 'abstain'>('approve');
  const [voteWeight, setVoteWeight] = useState<number>(1);
  const [comment, setComment] = useState<string>('');
  const [availableCredits, setAvailableCredits] = useState<number>(100); // Placeholder, should fetch from API
  
  const { 
    castVote, 
    loading, 
    error, 
    success,
    remainingCredits, 
    fetchVotingCredits 
  } = usePBVote(userDid, proposal.federation_id);

  // Fetch voting credits when modal opens
  useEffect(() => {
    if (open && proposal.voting_mechanism === 'quadratic') {
      fetchVotingCredits();
    }
  }, [open, proposal.federation_id, proposal.voting_mechanism]);

  // Calculate cost for quadratic voting
  const calculateQuadraticCost = (weight: number) => {
    return weight * weight;
  };

  const handleVoteSubmit = async () => {
    await castVote({
      proposal_id: proposal.id,
      federation_id: proposal.federation_id,
      choice: voteChoice,
      weight: voteWeight,
      comment: comment.trim() ? comment : undefined
    });
    
    if (success && onVoteComplete) {
      onVoteComplete();
    }
  };

  // Reset form when dialog closes
  const handleClose = () => {
    // Only close if not currently submitting
    if (!loading) {
      setVoteChoice('approve');
      setVoteWeight(1);
      setComment('');
      onClose();
    }
  };

  const isQuadraticVoting = proposal.voting_mechanism === 'quadratic';
  const currentCost = isQuadraticVoting ? calculateQuadraticCost(voteWeight) : 0;
  const canAfford = !isQuadraticVoting || currentCost <= (remainingCredits ?? 0);

  return (
    <Dialog 
      open={open} 
      onClose={handleClose} 
      maxWidth="sm" 
      fullWidth
      PaperProps={{ 
        sx: { borderRadius: 2, p: 1 } 
      }}
    >
      <DialogTitle>
        <Typography variant="h5">Cast Your Vote</Typography>
        <Typography variant="subtitle1" color="text.secondary">
          {proposal.title}
        </Typography>
      </DialogTitle>

      <DialogContent>
        {error && (
          <Alert severity="error" sx={{ mb: 2 }}>
            {error}
          </Alert>
        )}
        
        {success && (
          <Alert severity="success" sx={{ mb: 2 }}>
            Your vote has been recorded successfully!
          </Alert>
        )}

        <FormControl component="fieldset" sx={{ mb: 3, width: '100%' }}>
          <FormLabel component="legend">Your Vote</FormLabel>
          <RadioGroup
            value={voteChoice}
            onChange={(e) => setVoteChoice(e.target.value as any)}
          >
            <FormControlLabel value="approve" control={<Radio />} label="Approve" />
            <FormControlLabel value="reject" control={<Radio />} label="Reject" />
            <FormControlLabel value="abstain" control={<Radio />} label="Abstain" />
          </RadioGroup>
        </FormControl>

        {isQuadraticVoting && (
          <Box sx={{ mb: 3 }}>
            <Typography gutterBottom>
              Vote Weight: {voteWeight}
            </Typography>
            <Slider
              value={voteWeight}
              onChange={(_, newValue) => setVoteWeight(newValue as number)}
              min={1}
              max={10}
              step={1}
              marks
              valueLabelDisplay="auto"
              disabled={loading}
            />
            
            <Box sx={{ display: 'flex', justifyContent: 'space-between', mt: 1 }}>
              <Typography variant="body2" color="text.secondary">
                Cost: {currentCost} credits
              </Typography>
              <Typography variant="body2" color={canAfford ? 'success.main' : 'error.main'}>
                Available: {remainingCredits ?? 'Loading...'} credits
              </Typography>
            </Box>
            
            {!canAfford && (
              <Alert severity="warning" sx={{ mt: 1 }}>
                You don't have enough voting credits for this weight.
              </Alert>
            )}
          </Box>
        )}

        <TextField
          label="Comment (Optional)"
          multiline
          rows={3}
          value={comment}
          onChange={(e) => setComment(e.target.value)}
          fullWidth
          disabled={loading}
          sx={{ mb: 2 }}
        />

        <Divider sx={{ my: 2 }} />
        
        <Typography variant="subtitle2" gutterBottom>
          Voting Mechanism: {proposal.voting_mechanism.replace('_', ' ')}
        </Typography>
        
        <Typography variant="body2" color="text.secondary">
          {proposal.voting_mechanism === 'quadratic' 
            ? 'Quadratic voting allows you to express the strength of your preference by using more voting credits. The cost increases quadratically with weight.' 
            : proposal.voting_mechanism === 'consensus' 
            ? 'Consensus voting requires unanimous approval from all eligible voters.'
            : 'Simple majority voting requires more approve votes than reject votes.'}
        </Typography>
      </DialogContent>

      <DialogActions>
        <Button onClick={handleClose} disabled={loading}>
          Cancel
        </Button>
        <Button 
          onClick={handleVoteSubmit} 
          variant="contained" 
          color="primary"
          disabled={loading || (isQuadraticVoting && !canAfford) || success}
          startIcon={loading ? <CircularProgress size={20} /> : undefined}
        >
          {loading ? 'Submitting...' : 'Submit Vote'}
        </Button>
      </DialogActions>
    </Dialog>
  );
}; 