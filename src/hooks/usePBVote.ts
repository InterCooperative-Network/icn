import { useState, useCallback } from 'react';
import { pbVoteService } from '../services/pbVoteService';

interface VoteParams {
  proposal_id: string;
  federation_id: string;
  choice: 'approve' | 'reject' | 'abstain';
  weight?: number;
  comment?: string;
}

/**
 * Hook for participatory budgeting voting functionality
 * 
 * Provides methods to cast votes, check voting credit balances,
 * and access API response states
 */
export const usePBVote = (userDid: string, federationId: string) => {
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [success, setSuccess] = useState(false);
  const [remainingCredits, setRemainingCredits] = useState<number | null>(null);
  
  /**
   * Fetch voter's available voting credits 
   * Used for quadratic voting mechanisms
   */
  const fetchVotingCredits = useCallback(async () => {
    try {
      setLoading(true);
      const result = await pbVoteService.getVotingCredits(userDid, federationId);
      setRemainingCredits(result.availableCredits);
      setError(null);
    } catch (err) {
      console.error('Failed to fetch voting credits:', err);
      setError('Unable to retrieve voting credits');
    } finally {
      setLoading(false);
    }
  }, [userDid, federationId]);
  
  /**
   * Cast a vote on a proposal
   */
  const castVote = useCallback(async (params: VoteParams) => {
    try {
      setLoading(true);
      setError(null);
      setSuccess(false);
      
      // Format vote data for API
      const voteData = {
        voter_did: userDid,
        proposal_id: params.proposal_id,
        federation_id: params.federation_id,
        vote: params.choice,
        ...(params.weight ? { vote_weight: params.weight } : {}),
        ...(params.comment ? { comment: params.comment } : {})
      };
      
      // Submit vote to API
      const result = await pbVoteService.submitVote(voteData);
      
      // If quadratic voting, update the remaining credits
      if (params.weight && params.weight > 1) {
        const cost = params.weight * params.weight;
        if (remainingCredits !== null) {
          setRemainingCredits(remainingCredits - cost);
        }
      }
      
      // Set success state
      setSuccess(true);
      
      return result;
    } catch (err) {
      console.error('Vote submission failed:', err);
      setError((err as Error)?.message || 'Failed to submit vote');
      setSuccess(false);
      return null;
    } finally {
      setLoading(false);
    }
  }, [userDid, remainingCredits]);
  
  /**
   * Check if the user has already voted on this proposal
   */
  const checkVoteStatus = useCallback(async (proposalId: string) => {
    try {
      setLoading(true);
      const result = await pbVoteService.checkVoteStatus(userDid, proposalId);
      return result.hasVoted;
    } catch (err) {
      console.error('Failed to check vote status:', err);
      return false;
    } finally {
      setLoading(false);
    }
  }, [userDid]);
  
  /**
   * Reset the hook state
   */
  const reset = useCallback(() => {
    setLoading(false);
    setError(null);
    setSuccess(false);
  }, []);
  
  return {
    loading,
    error,
    success,
    remainingCredits,
    castVote,
    fetchVotingCredits,
    checkVoteStatus,
    reset
  };
}; 