import React, { useState, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { PlusIcon } from '@heroicons/react/24/outline';
import ProposalList from '../components/ProposalList';
import { proposalApi } from '../services/runtimeApi';
import { useCredentials } from '../contexts/CredentialContext';

export default function ProposalListPage() {
  const { isAuthenticated, federationId, hasPermission } = useCredentials();
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

  return (
    <div>
      <div className="mb-5 sm:flex sm:items-center">
        <div className="sm:flex-auto">
          <h1 className="text-xl font-semibold text-gray-900">Proposals</h1>
          <p className="mt-2 text-sm text-gray-700">
            A list of all proposals in the system with their status, creator, and linked deliberation threads.
          </p>
        </div>
        <div className="mt-4 sm:mt-0 sm:ml-16 sm:flex-none">
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