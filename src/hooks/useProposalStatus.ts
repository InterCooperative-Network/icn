import React, { useMemo } from 'react';

type ProposalStatus = 'deliberation' | 'voting' | 'approved' | 'rejected' | 'allocated' | 'completed';

/**
 * Hook to get formatted status, color, and time remaining for a proposal
 */
export const useProposalStatus = (
  status: ProposalStatus,
  votingStart: number,
  votingEnd: number
) => {
  // Calculate time remaining for the current phase
  const timeRemaining = useMemo(() => {
    const now = Date.now();
    
    // For voting phase
    if (status === 'voting') {
      const remainingMs = votingEnd - now;
      
      if (remainingMs <= 0) {
        return 'Voting ended';
      }
      
      const days = Math.floor(remainingMs / (1000 * 60 * 60 * 24));
      const hours = Math.floor((remainingMs % (1000 * 60 * 60 * 24)) / (1000 * 60 * 60));
      
      if (days > 0) {
        return `${days}d ${hours}h remaining`;
      } else {
        const minutes = Math.floor((remainingMs % (1000 * 60 * 60)) / (1000 * 60));
        return `${hours}h ${minutes}m remaining`;
      }
    }
    
    // For deliberation phase
    if (status === 'deliberation') {
      const remainingMs = votingStart - now;
      
      if (remainingMs <= 0) {
        return 'Deliberation ended';
      }
      
      const days = Math.floor(remainingMs / (1000 * 60 * 60 * 24));
      const hours = Math.floor((remainingMs % (1000 * 60 * 60 * 24)) / (1000 * 60 * 60));
      
      if (days > 0) {
        return `Voting in ${days}d ${hours}h`;
      } else {
        const minutes = Math.floor((remainingMs % (1000 * 60 * 60)) / (1000 * 60));
        return `Voting in ${hours}h ${minutes}m`;
      }
    }
    
    // For other statuses, show when voting ended
    const votingEndDate = new Date(votingEnd);
    return `Ended ${votingEndDate.toLocaleDateString()}`;
  }, [status, votingStart, votingEnd]);
  
  // Get status color and text
  const statusColor = useMemo(() => {
    switch (status) {
      case 'deliberation':
        return 'primary';
      case 'voting':
        return 'warning';
      case 'approved':
        return 'success';
      case 'allocated':
        return 'success';
      case 'completed':
        return 'success';
      case 'rejected':
        return 'error';
      default:
        return 'default';
    }
  }, [status]);
  
  const statusText = useMemo(() => {
    switch (status) {
      case 'deliberation':
        return 'Deliberation';
      case 'voting':
        return 'Voting Open';
      case 'approved':
        return 'Approved';
      case 'allocated':
        return 'Funds Allocated';
      case 'completed':
        return 'Completed';
      case 'rejected':
        return 'Rejected';
      default:
        // Handle any other string safely
        return status ? status.charAt(0).toUpperCase() + status.slice(1) : 'Unknown';
    }
  }, [status]);
  
  return {
    statusColor,
    statusText,
    timeRemaining
  };
}; 