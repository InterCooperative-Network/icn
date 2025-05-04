import React, { useState } from 'react';
import { Link } from 'react-router-dom';
import { 
  CheckCircleIcon, 
  XCircleIcon, 
  ClockIcon,
  DocumentTextIcon,
  ArrowTopRightOnSquareIcon,
  BellAlertIcon
} from '@heroicons/react/24/outline';
import { useCredentials } from '../contexts/CredentialContext';
import { useDagSync } from '../contexts/DagSyncContext';
import CredentialScopeSelector from './CredentialScopeSelector';

// Proposal status badges
const StatusBadge = ({ status }) => {
  switch (status.toLowerCase()) {
    case 'executed':
      return (
        <span className="badge badge-green">
          <CheckCircleIcon className="h-4 w-4 mr-1" />
          Executed
        </span>
      );
    case 'rejected':
      return (
        <span className="badge badge-red">
          <XCircleIcon className="h-4 w-4 mr-1" />
          Rejected
        </span>
      );
    case 'active':
      return (
        <span className="badge badge-blue">
          <ClockIcon className="h-4 w-4 mr-1" />
          Active
        </span>
      );
    case 'deliberating':
      return (
        <span className="badge badge-yellow">
          <DocumentTextIcon className="h-4 w-4 mr-1" />
          Deliberating
        </span>
      );
    default:
      return (
        <span className="badge bg-gray-100 text-gray-800">
          {status}
        </span>
      );
  }
};

export default function ProposalList({ proposals, isLoading, error, onFilterChange }) {
  const { federationId, hasPermission } = useCredentials();
  const { updatedProposals, clearUpdatedProposal } = useDagSync();
  const [selectedScope, setSelectedScope] = useState(federationId || 'all');
  const [creatorFilter, setCreatorFilter] = useState('');
  const [statusFilter, setStatusFilter] = useState('all');
  
  // Handle filter changes
  const handleFilterChange = (filterType, value) => {
    switch (filterType) {
      case 'scope':
        setSelectedScope(value);
        break;
      case 'creator':
        setCreatorFilter(value);
        break;
      case 'status':
        setStatusFilter(value);
        break;
      default:
        break;
    }
    
    // Pass the updated filters to parent
    onFilterChange({
      federationId: filterType === 'scope' ? value : selectedScope,
      creatorDid: filterType === 'creator' ? value : creatorFilter,
      status: filterType === 'status' ? value : statusFilter,
    });
  };
  
  // Check if a proposal is in the updated list
  const isProposalUpdated = (proposalId) => {
    return updatedProposals.some(p => p.id === proposalId);
  };
  
  // Clear highlight when clicking a proposal
  const handleProposalClick = (proposalId) => {
    if (isProposalUpdated(proposalId)) {
      clearUpdatedProposal(proposalId);
    }
  };
  
  // Placeholder for empty state
  if (proposals?.length === 0 && !isLoading) {
    return (
      <div className="text-center py-12">
        <DocumentTextIcon className="h-12 w-12 text-gray-400 mx-auto mb-4" />
        <h3 className="text-lg font-medium text-gray-900">No proposals found</h3>
        <p className="mt-1 text-sm text-gray-500">
          There are no proposals matching your filter criteria.
        </p>
        {hasPermission('create_proposal') && (
          <div className="mt-6">
            <Link
              to="/proposals/new"
              className="btn btn-primary"
            >
              Create New Proposal
            </Link>
          </div>
        )}
      </div>
    );
  }
  
  // Loading state
  if (isLoading) {
    return (
      <div className="flex justify-center py-12">
        <div className="animate-spin rounded-full h-12 w-12 border-t-2 border-b-2 border-agora-blue"></div>
      </div>
    );
  }
  
  // Error state
  if (error) {
    return (
      <div className="text-center py-12">
        <XCircleIcon className="h-12 w-12 text-red-500 mx-auto mb-4" />
        <h3 className="text-lg font-medium text-gray-900">Error loading proposals</h3>
        <p className="mt-1 text-sm text-gray-500">{error.message}</p>
      </div>
    );
  }
  
  return (
    <div>
      {/* Updates indicator */}
      {updatedProposals.length > 0 && (
        <div className="mb-4 bg-yellow-50 border border-yellow-200 rounded-md p-3 flex items-center">
          <BellAlertIcon className="h-5 w-5 text-yellow-500 mr-2" />
          <span className="text-yellow-700">
            {updatedProposals.length} proposal(s) have been updated via recent DAG anchoring
          </span>
        </div>
      )}
      
      {/* Filters */}
      <div className="mb-6 grid grid-cols-1 md:grid-cols-3 gap-4">
        <div>
          <label htmlFor="scopeFilter" className="block text-sm font-medium text-gray-700">
            Scope
          </label>
          <CredentialScopeSelector 
            value={selectedScope} 
            onChange={(value) => handleFilterChange('scope', value)}
          />
        </div>
        
        <div>
          <label htmlFor="creatorFilter" className="block text-sm font-medium text-gray-700">
            Creator DID
          </label>
          <input
            type="text"
            id="creatorFilter"
            className="mt-1 block w-full rounded-md border-gray-300 shadow-sm focus:border-agora-blue focus:ring-agora-blue sm:text-sm"
            placeholder="DID:ICN:..."
            value={creatorFilter}
            onChange={(e) => handleFilterChange('creator', e.target.value)}
          />
        </div>
        
        <div>
          <label htmlFor="statusFilter" className="block text-sm font-medium text-gray-700">
            Status
          </label>
          <select
            id="statusFilter"
            className="mt-1 block w-full rounded-md border-gray-300 shadow-sm focus:border-agora-blue focus:ring-agora-blue sm:text-sm"
            value={statusFilter}
            onChange={(e) => handleFilterChange('status', e.target.value)}
          >
            <option value="all">All Statuses</option>
            <option value="deliberating">Deliberating</option>
            <option value="active">Active</option>
            <option value="executed">Executed</option>
            <option value="rejected">Rejected</option>
          </select>
        </div>
      </div>
      
      {/* Proposals List */}
      <div className="bg-white shadow overflow-hidden sm:rounded-md">
        <ul className="divide-y divide-gray-200">
          {proposals.map((proposal) => {
            const isUpdated = isProposalUpdated(proposal.id);
            return (
              <li key={proposal.id} className={isUpdated ? "bg-yellow-50" : undefined}>
                <Link 
                  to={`/proposals/${proposal.id}`} 
                  className="block hover:bg-gray-50"
                  onClick={() => handleProposalClick(proposal.id)}
                >
                  <div className="px-4 py-4 sm:px-6">
                    <div className="flex items-center justify-between">
                      <div className="truncate">
                        <div className="flex text-sm">
                          <p className="font-medium text-agora-blue truncate">{proposal.title}</p>
                          {isUpdated && (
                            <span className="ml-2 flex items-center text-yellow-600">
                              <BellAlertIcon className="h-4 w-4 mr-1" />
                              Updated
                            </span>
                          )}
                        </div>
                        <div className="mt-2 flex">
                          <div className="flex items-center text-sm text-gray-500">
                            <p>
                              Created by: {proposal.creatorDid?.substring(0, 16)}...
                            </p>
                          </div>
                        </div>
                      </div>
                      <div className="ml-2 flex-shrink-0 flex flex-col items-end">
                        <StatusBadge status={proposal.status} />
                        
                        {proposal.threadId && (
                          <div className="mt-2 flex items-center text-sm text-gray-500">
                            <span className="mr-1">Thread:</span>
                            <a 
                              href={`/threads/${proposal.threadId}`}
                              className="text-agora-blue hover:underline flex items-center"
                              onClick={(e) => e.stopPropagation()}
                            >
                              {proposal.threadId.substring(0, 8)}...
                              <ArrowTopRightOnSquareIcon className="h-4 w-4 ml-1" />
                            </a>
                          </div>
                        )}
                      </div>
                    </div>
                  </div>
                </Link>
              </li>
            );
          })}
        </ul>
      </div>
    </div>
  );
} 