import React, { useState, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { PlusIcon, ArrowPathIcon } from '@heroicons/react/24/outline';
import ProposalList from '../components/ProposalList';
import { proposalApi } from '../services/runtimeApi';
import { useCredentials } from '../contexts/CredentialContext';
import { useDagSync } from '../contexts/DagSyncContext';

export default function ProposalListPage() {
  const { isAuthenticated, federationId, hasPermission } = useCredentials();
  const { syncNow, lastSyncTime, syncStatus, updatedProposals } = useDagSync();
  const navigate = useNavigate();
  
  const [proposals, setProposals] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const [filters, setFilters] = useState({
    federationId: federationId || 'all',
    creatorDid: '',
    status: 'all'
  });

  // Load proposals with filters
  useEffect(() => {
    async function fetchProposals() {
      try {
        setLoading(true);
        
        // Prepare API filters
        const apiFilters = {};
        if (filters.federationId && filters.federationId !== 'all') {
          apiFilters.federationId = filters.federationId;
        }
        if (filters.creatorDid) {
          apiFilters.creatorDid = filters.creatorDid;
        }
        if (filters.status && filters.status !== 'all') {
          apiFilters.status = filters.status;
        }
        
        // Fetch proposals
        const data = await proposalApi.getProposals(apiFilters);
        setProposals(data);
      } catch (err) {
        console.error('Error fetching proposals:', err);
        setError('Failed to load proposals');
      } finally {
        setLoading(false);
      }
    }
    
    fetchProposals();
  }, [filters]);

  // Reload when DAG updates are detected
  useEffect(() => {
    if (updatedProposals.length > 0) {
      // Only reload if we're not in the middle of loading already
      if (!loading) {
        // Call the handleFilterChange with current filters to reload
        handleFilterChange({});
      }
    }
  }, [updatedProposals, loading]);

  // Handle filter changes from the proposal list component
  const handleFilterChange = (newFilters) => {
    setFilters({
      ...filters,
      ...newFilters
    });
  };

  // Create new proposal
  const handleCreateProposal = () => {
    navigate('/proposals/new');
  };
  
  // Manual refresh
  const handleRefresh = () => {
    // Trigger a DAG sync
    syncNow();
    
    // Also reload proposals from the API
    handleFilterChange({});
  };

  // Format the time since last sync
  const getTimeAgo = () => {
    if (!lastSyncTime) return 'Never';
    
    const seconds = Math.floor((new Date() - lastSyncTime) / 1000);
    
    if (seconds < 60) return `${seconds}s ago`;
    if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
    return `${Math.floor(seconds / 3600)}h ago`;
  };

  return (
    <div>
      <div className="mb-5 sm:flex sm:items-center">
        <div className="sm:flex-auto">
          <h1 className="text-xl font-semibold text-gray-900">Proposals</h1>
          <p className="mt-2 text-sm text-gray-700">
            A list of all proposals in the system with their status, creator, and linked deliberation threads.
          </p>
        </div>
        <div className="mt-4 sm:mt-0 sm:ml-16 sm:flex-none flex items-center space-x-4">
          <button
            type="button"
            onClick={handleRefresh}
            disabled={syncStatus === 'syncing'}
            className="inline-flex items-center rounded-md border border-gray-300 bg-white px-3 py-2 text-sm font-medium leading-4 text-gray-700 shadow-sm hover:bg-gray-50 focus:outline-none"
          >
            <ArrowPathIcon 
              className={`h-4 w-4 mr-2 ${syncStatus === 'syncing' ? 'animate-spin text-agora-blue' : 'text-gray-500'}`} 
            />
            {syncStatus === 'syncing' ? 'Syncing...' : `Refresh (${getTimeAgo()})`}
          </button>
          
          {isAuthenticated && hasPermission('create_proposal') && (
            <button
              type="button"
              onClick={handleCreateProposal}
              className="inline-flex items-center justify-center rounded-md border border-transparent bg-agora-blue px-4 py-2 text-sm font-medium text-white shadow-sm hover:bg-blue-700"
            >
              <PlusIcon className="-ml-1 mr-2 h-5 w-5" />
              New Proposal
            </button>
          )}
        </div>
      </div>
      
      <ProposalList
        proposals={proposals}
        isLoading={loading}
        error={error}
        onFilterChange={handleFilterChange}
      />
    </div>
  );
} 