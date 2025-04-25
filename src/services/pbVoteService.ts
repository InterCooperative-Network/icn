import axios from 'axios';

// Define the API base URL
const API_BASE_URL = process.env.REACT_APP_API_URL || 'http://localhost:3001';

// Define interfaces for API responses
interface ApiResponse<T> {
  success: boolean;
  data?: T;
  error?: string;
}

interface VoteResponse {
  vote_id: string;
  proposal_id: string;
  federation_id: string;
  voter_did: string;
  vote: string;
  weight: number;
  timestamp: number;
}

interface VoteStatusResponse {
  hasVoted: boolean;
  vote?: {
    choice: string;
    weight: number;
    timestamp: number;
  };
}

interface VotingCreditsResponse {
  availableCredits: number;
  totalCredits: number;
  spentCredits: number;
  federation_id: string;
  voter_did: string;
}

interface ProposalVotesResponse {
  proposal_id: string;
  total_votes: number;
  approve_votes: number;
  reject_votes: number;
  abstain_votes: number;
  quorum_reached: boolean;
  approval_threshold_met: boolean;
  voters: Array<{
    did: string;
    vote: string;
    weight: number;
  }>;
}

interface VoteSubmitRequest {
  voter_did: string;
  proposal_id: string;
  federation_id: string;
  vote: string;
  vote_weight?: number;
  comment?: string;
}

/**
 * Service for interacting with the participatory budgeting voting API
 */
export const pbVoteService = {
  /**
   * Submit a vote on a proposal
   */
  async submitVote(voteData: VoteSubmitRequest): Promise<VoteResponse> {
    try {
      const response = await axios.post<ApiResponse<VoteResponse>>(
        `${API_BASE_URL}/api/pb/votes`,
        voteData
      );
      
      if (!response.data.success || !response.data.data) {
        throw new Error(response.data.error || 'Failed to submit vote');
      }
      
      return response.data.data;
    } catch (error) {
      console.error('API Error submitting vote:', error);
      throw new Error(
        error instanceof Error ? error.message : 'Unknown error occurred'
      );
    }
  },

  /**
   * Check if a user has already voted on a proposal
   */
  async checkVoteStatus(voterDid: string, proposalId: string): Promise<VoteStatusResponse> {
    try {
      const response = await axios.get<ApiResponse<VoteStatusResponse>>(
        `${API_BASE_URL}/api/pb/votes/status`,
        {
          params: {
            voter_did: voterDid,
            proposal_id: proposalId,
          },
        }
      );
      
      if (!response.data.success || !response.data.data) {
        throw new Error(response.data.error || 'Failed to check vote status');
      }
      
      return response.data.data;
    } catch (error) {
      console.error('API Error checking vote status:', error);
      throw new Error(
        error instanceof Error ? error.message : 'Unknown error occurred'
      );
    }
  },

  /**
   * Get available voting credits for a user in a federation
   */
  async getVotingCredits(
    voterDid: string,
    federationId: string
  ): Promise<VotingCreditsResponse> {
    try {
      const response = await axios.get<ApiResponse<VotingCreditsResponse>>(
        `${API_BASE_URL}/api/pb/voting-credits`,
        {
          params: {
            voter_did: voterDid,
            federation_id: federationId,
          },
        }
      );
      
      if (!response.data.success || !response.data.data) {
        throw new Error(response.data.error || 'Failed to get voting credits');
      }
      
      return response.data.data;
    } catch (error) {
      console.error('API Error getting voting credits:', error);
      throw new Error(
        error instanceof Error ? error.message : 'Unknown error occurred'
      );
    }
  },

  /**
   * Get vote tallies for a proposal
   */
  async getProposalVotes(proposalId: string): Promise<ProposalVotesResponse> {
    try {
      const response = await axios.get<ApiResponse<ProposalVotesResponse>>(
        `${API_BASE_URL}/api/pb/proposals/${proposalId}/votes`
      );
      
      if (!response.data.success || !response.data.data) {
        throw new Error(response.data.error || 'Failed to get proposal votes');
      }
      
      return response.data.data;
    } catch (error) {
      console.error('API Error getting proposal votes:', error);
      throw new Error(
        error instanceof Error ? error.message : 'Unknown error occurred'
      );
    }
  },

  /**
   * Export voting data for credential generation
   */
  async exportVoteForCredential(
    voterDid: string,
    proposalId: string
  ): Promise<{ credential_data: any }> {
    try {
      const response = await axios.get<ApiResponse<{ credential_data: any }>>(
        `${API_BASE_URL}/api/pb/votes/export`,
        {
          params: {
            voter_did: voterDid,
            proposal_id: proposalId,
          },
        }
      );
      
      if (!response.data.success || !response.data.data) {
        throw new Error(response.data.error || 'Failed to export vote data');
      }
      
      return response.data.data;
    } catch (error) {
      console.error('API Error exporting vote data:', error);
      throw new Error(
        error instanceof Error ? error.message : 'Unknown error occurred'
      );
    }
  },
}; 