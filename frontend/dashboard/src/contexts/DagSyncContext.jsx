import React, { createContext, useContext, useState, useEffect, useCallback } from 'react';
import { dagApi } from '../services/runtimeApi';

const DagSyncContext = createContext();

export function useDagSync() {
  return useContext(DagSyncContext);
}

export function DagSyncProvider({ children }) {
  const [latestAnchor, setLatestAnchor] = useState(null);
  const [previousAnchor, setPreviousAnchor] = useState(null);
  const [syncStatus, setSyncStatus] = useState('idle'); // 'idle', 'syncing', 'error'
  const [lastSyncTime, setLastSyncTime] = useState(null);
  const [updatedProposals, setUpdatedProposals] = useState([]);
  const [error, setError] = useState(null);
  const [isPaused, setIsPaused] = useState(false);

  // Load the initial anchor from localStorage
  useEffect(() => {
    const cachedAnchor = localStorage.getItem('dagAnchorCache');
    if (cachedAnchor) {
      try {
        const parsed = JSON.parse(cachedAnchor);
        setLatestAnchor(parsed);
      } catch (err) {
        console.error('Error parsing cached DAG anchor:', err);
        localStorage.removeItem('dagAnchorCache');
      }
    }
  }, []);

  // Function to fetch latest anchors
  const fetchAnchors = useCallback(async () => {
    if (isPaused) return;
    
    try {
      setSyncStatus('syncing');
      const since = latestAnchor?.cid || null;
      const anchors = await dagApi.getAnchors(since);
      
      if (anchors && anchors.latestAnchor) {
        // If there's an update
        if (anchors.latestAnchor.cid !== latestAnchor?.cid) {
          setPreviousAnchor(latestAnchor);
          setLatestAnchor(anchors.latestAnchor);
          
          // Save to localStorage
          localStorage.setItem('dagAnchorCache', JSON.stringify(anchors.latestAnchor));
          
          // Collect updated proposals
          if (anchors.updatedProposals && anchors.updatedProposals.length > 0) {
            setUpdatedProposals(anchors.updatedProposals);
          }
        }
      }
      
      setLastSyncTime(new Date());
      setSyncStatus('idle');
      setError(null);
    } catch (err) {
      console.error('Error fetching DAG anchors:', err);
      setSyncStatus('error');
      setError(err.message || 'Error syncing with DAG');
    }
  }, [latestAnchor, isPaused]);

  // Poll for anchors every 10 seconds
  useEffect(() => {
    fetchAnchors(); // Initial fetch
    
    const interval = setInterval(() => {
      fetchAnchors();
    }, 10000); // 10 seconds
    
    return () => clearInterval(interval);
  }, [fetchAnchors]);

  // Clear a proposal from the updated list
  const clearUpdatedProposal = (proposalId) => {
    setUpdatedProposals(prev => prev.filter(p => p.id !== proposalId));
  };

  // Pause/resume syncing
  const togglePause = () => {
    setIsPaused(prev => !prev);
  };

  // Manual sync
  const syncNow = () => {
    if (!isPaused) {
      fetchAnchors();
    }
  };

  const value = {
    latestAnchor,
    previousAnchor,
    syncStatus,
    lastSyncTime,
    error,
    isPaused,
    updatedProposals,
    clearUpdatedProposal,
    togglePause,
    syncNow
  };

  return (
    <DagSyncContext.Provider value={value}>
      {children}
    </DagSyncContext.Provider>
  );
} 