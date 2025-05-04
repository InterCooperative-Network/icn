import React, { useState, useEffect } from 'react';
import { 
  CheckCircleIcon, 
  XCircleIcon, 
  MinusCircleIcon,
  QuestionMarkCircleIcon,
  LockClosedIcon,
  UsersIcon
} from '@heroicons/react/24/outline';
import { proposalApi } from '../services/runtimeApi';
import { useCredentials } from '../contexts/CredentialContext';

export default function VotePanel({ proposalId, onVoteSuccess, disabled }) {
  const { userDid, hasPermission } = useCredentials();
  
  const [votes, setVotes] = useState({
    yes: 0,
    no: 0,
    abstain: 0,
    total: 0,
    voters: []
  });
  
  const [votingConfig, setVotingConfig] = useState({
    threshold: 0,
    majority: 0,
    quorum: 0,
    votingPeriod: null
  });
  
  const [userVote, setUserVote] = useState(null);
  const [isVoting, setIsVoting] = useState(false);
  const [error, setError] = useState(null);
  const [isLoading, setIsLoading] = useState(true);
  
  // Load voting data and config
  useEffect(() => {
    async function loadVotingData() {
      try {
        setIsLoading(true);
        
        // Load voting configuration
        const configResponse = await proposalApi.getVotingConfig(proposalId);
        setVotingConfig(configResponse);
        
        // Load current votes
        const votesResponse = await proposalApi.getVotes(proposalId);
        setVotes(votesResponse);
        
        // Check if user has already voted
        if (userDid && votesResponse.voters) {
          const userVoteRecord = votesResponse.voters.find(
            v => v.did === userDid
          );
          
          if (userVoteRecord) {
            setUserVote(userVoteRecord.vote);
          }
        }
      } catch (err) {
        console.error('Error loading voting data:', err);
        setError('Failed to load voting information');
      } finally {
        setIsLoading(false);
      }
    }
    
    loadVotingData();
    
    // Set up polling for vote updates
    const interval = setInterval(async () => {
      try {
        const votesResponse = await proposalApi.getVotes(proposalId);
        setVotes(votesResponse);
      } catch (err) {
        console.error('Error polling votes:', err);
      }
    }, 10000); // Poll every 10 seconds
    
    return () => clearInterval(interval);
  }, [proposalId, userDid]);
  
  // Calculate progress percentages
  const calculateProgress = (voteType) => {
    if (votes.total === 0) return 0;
    return Math.round((votes[voteType] / votes.total) * 100);
  };
  
  // Check if quorum and threshold are met
  const isQuorumMet = votes.total >= votingConfig.quorum;
  const isThresholdMet = calculateProgress('yes') >= votingConfig.threshold;
  
  // Submit a vote
  const handleVote = async (vote) => {
    if (isVoting || disabled || userVote) return;
    
    try {
      setIsVoting(true);
      setError(null);
      
      // In a real implementation, we would sign the vote with the user's DID
      // For now, we'll mock the signature
      const voteData = {
        vote,
        did: userDid,
        signature: 'mock-signature', // In a real app, this would be a proper signature
        timestamp: new Date().toISOString()
      };
      
      await proposalApi.submitVote(proposalId, voteData);
      
      // Update local state
      setUserVote(vote);
      setVotes(prev => ({
        ...prev,
        [vote]: prev[vote] + 1,
        total: prev.total + 1,
        voters: [...prev.voters, { did: userDid, vote, timestamp: new Date().toISOString() }]
      }));
      
      // Call success callback if provided
      if (onVoteSuccess) {
        onVoteSuccess(vote);
      }
    } catch (err) {
      console.error('Error submitting vote:', err);
      setError('Failed to submit vote');
    } finally {
      setIsVoting(false);
    }
  };
  
  // Format date
  const formatDate = (dateString) => {
    if (!dateString) return 'Not set';
    return new Date(dateString).toLocaleString();
  };
  
  // Check if the user can vote
  const canVote = !disabled && !userVote && hasPermission('vote_proposal');
  
  if (isLoading) {
    return (
      <div className="bg-white shadow sm:rounded-lg p-6">
        <div className="flex justify-center">
          <div className="animate-spin rounded-full h-8 w-8 border-t-2 border-b-2 border-agora-blue"></div>
        </div>
      </div>
    );
  }
  
  return (
    <div className="bg-white shadow sm:rounded-lg overflow-hidden">
      <div className="px-4 py-5 sm:p-6">
        <h3 className="text-lg leading-6 font-medium text-gray-900">
          Proposal Voting
        </h3>
        
        <div className="mt-4 flex justify-between items-center text-sm text-gray-500">
          <div>
            <span className="flex items-center">
              <UsersIcon className="h-4 w-4 mr-1" />
              Quorum: {votes.total}/{votingConfig.quorum} votes
              {isQuorumMet && (
                <CheckCircleIcon className="h-4 w-4 ml-1 text-green-500" />
              )}
            </span>
          </div>
          
          {votingConfig.votingPeriod && (
            <div>
              <span>Ends: {formatDate(votingConfig.votingPeriod.end)}</span>
            </div>
          )}
        </div>
        
        {/* Progress bars */}
        <div className="mt-6 space-y-4">
          {/* Yes votes */}
          <div>
            <div className="flex items-center justify-between mb-1">
              <span className="text-sm font-medium text-green-700 flex items-center">
                <CheckCircleIcon className="h-4 w-4 mr-1" />
                Yes
              </span>
              <span className="text-sm font-medium text-green-700">{votes.yes} votes ({calculateProgress('yes')}%)</span>
            </div>
            <div className="w-full bg-gray-200 rounded-full h-2.5">
              <div 
                className="bg-green-500 h-2.5 rounded-full" 
                style={{ width: `${calculateProgress('yes')}%` }}
              ></div>
            </div>
          </div>
          
          {/* No votes */}
          <div>
            <div className="flex items-center justify-between mb-1">
              <span className="text-sm font-medium text-red-700 flex items-center">
                <XCircleIcon className="h-4 w-4 mr-1" />
                No
              </span>
              <span className="text-sm font-medium text-red-700">{votes.no} votes ({calculateProgress('no')}%)</span>
            </div>
            <div className="w-full bg-gray-200 rounded-full h-2.5">
              <div 
                className="bg-red-500 h-2.5 rounded-full" 
                style={{ width: `${calculateProgress('no')}%` }}
              ></div>
            </div>
          </div>
          
          {/* Abstain votes */}
          <div>
            <div className="flex items-center justify-between mb-1">
              <span className="text-sm font-medium text-gray-700 flex items-center">
                <MinusCircleIcon className="h-4 w-4 mr-1" />
                Abstain
              </span>
              <span className="text-sm font-medium text-gray-700">{votes.abstain} votes ({calculateProgress('abstain')}%)</span>
            </div>
            <div className="w-full bg-gray-200 rounded-full h-2.5">
              <div 
                className="bg-gray-500 h-2.5 rounded-full" 
                style={{ width: `${calculateProgress('abstain')}%` }}
              ></div>
            </div>
          </div>
        </div>
        
        {/* Threshold indicator */}
        <div className="mt-4 text-sm text-gray-500">
          <div className="flex items-center">
            <span>Threshold: {votingConfig.threshold}% needed</span>
            {isThresholdMet && (
              <CheckCircleIcon className="h-4 w-4 ml-1 text-green-500" />
            )}
          </div>
        </div>
        
        {/* Vote buttons */}
        {canVote ? (
          <div className="mt-6">
            <div className="flex space-x-3">
              <button
                onClick={() => handleVote('yes')}
                disabled={isVoting}
                className="flex-1 bg-green-100 text-green-800 hover:bg-green-200 py-2 px-4 rounded-md flex items-center justify-center"
              >
                <CheckCircleIcon className="h-5 w-5 mr-2" />
                Yes
              </button>
              
              <button
                onClick={() => handleVote('no')}
                disabled={isVoting}
                className="flex-1 bg-red-100 text-red-800 hover:bg-red-200 py-2 px-4 rounded-md flex items-center justify-center"
              >
                <XCircleIcon className="h-5 w-5 mr-2" />
                No
              </button>
              
              <button
                onClick={() => handleVote('abstain')}
                disabled={isVoting}
                className="flex-1 bg-gray-100 text-gray-800 hover:bg-gray-200 py-2 px-4 rounded-md flex items-center justify-center"
              >
                <MinusCircleIcon className="h-5 w-5 mr-2" />
                Abstain
              </button>
            </div>
            
            {error && (
              <div className="mt-2 text-sm text-red-600">
                {error}
              </div>
            )}
          </div>
        ) : (
          <div className="mt-6 bg-gray-50 p-4 rounded-md flex items-center justify-center text-gray-500">
            {userVote ? (
              <div className="text-center">
                <div className="font-medium mb-1">You voted: {userVote}</div>
                <div className="text-sm">Your vote has been recorded</div>
              </div>
            ) : (
              <div className="flex items-center text-sm">
                <LockClosedIcon className="h-5 w-5 mr-2" />
                {!hasPermission('vote_proposal') 
                  ? "You don't have permission to vote on this proposal" 
                  : "Voting is not available at this time"}
              </div>
            )}
          </div>
        )}
        
        {/* Recent voters */}
        {votes.voters && votes.voters.length > 0 && (
          <div className="mt-6">
            <h4 className="text-sm font-medium text-gray-700 mb-2">Recent Votes</h4>
            <ul className="space-y-2 max-h-32 overflow-y-auto">
              {votes.voters.slice(0, 5).map((voter, index) => (
                <li key={index} className="text-xs flex justify-between bg-gray-50 p-2 rounded-md">
                  <span className="font-mono">{voter.did.substring(0, 16)}...</span>
                  <div className="flex items-center">
                    <span>{voter.vote}</span>
                    <span className="ml-2 text-gray-500">
                      {new Date(voter.timestamp).toLocaleTimeString()}
                    </span>
                  </div>
                </li>
              ))}
            </ul>
          </div>
        )}
      </div>
    </div>
  );
} 