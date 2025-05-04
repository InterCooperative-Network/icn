import React, { useState, useEffect } from 'react';
import { Link } from 'react-router-dom';
import { 
  DocumentTextIcon, 
  CheckCircleIcon,
  XCircleIcon,
  ClockIcon,
  ChatBubbleLeftRightIcon,
  ArrowPathIcon,
  LinkIcon,
  BellAlertIcon
} from '@heroicons/react/24/outline';
import { proposalApi } from '../services/runtimeApi';
import { useCredentials } from '../contexts/CredentialContext';
import { useDagSync } from '../contexts/DagSyncContext';

export default function Dashboard() {
  const { isAuthenticated, userDid, federationId } = useCredentials();
  const { latestAnchor, updatedProposals, lastSyncTime } = useDagSync();
  const [stats, setStats] = useState({
    totalProposals: 0,
    executedProposals: 0,
    activeProposals: 0,
    linkedThreads: 0
  });
  const [recentProposals, setRecentProposals] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);

  // Load dashboard data
  useEffect(() => {
    async function fetchDashboardData() {
      try {
        setLoading(true);
        
        // Get proposals with federation filter if authenticated
        const filters = {};
        if (isAuthenticated && federationId) {
          filters.federationId = federationId;
        }
        
        const proposals = await proposalApi.getProposals(filters);
        
        // Calculate stats
        const executed = proposals.filter(p => p.status.toLowerCase() === 'executed').length;
        const active = proposals.filter(p => p.status.toLowerCase() === 'active').length;
        const withThreads = proposals.filter(p => p.threadId).length;
        
        setStats({
          totalProposals: proposals.length,
          executedProposals: executed,
          activeProposals: active,
          linkedThreads: withThreads
        });
        
        // Get 5 most recent proposals
        const sortedProposals = [...proposals].sort((a, b) => 
          new Date(b.createdAt) - new Date(a.createdAt)
        ).slice(0, 5);
        
        setRecentProposals(sortedProposals);
      } catch (err) {
        console.error('Error fetching dashboard data:', err);
        setError('Failed to load dashboard data');
      } finally {
        setLoading(false);
      }
    }
    
    fetchDashboardData();
    
    // Refresh data when updatedProposals changes
    if (updatedProposals.length > 0) {
      fetchDashboardData();
    }
  }, [isAuthenticated, federationId, updatedProposals]);

  // Format the time ago
  const getTimeAgo = () => {
    if (!lastSyncTime) return 'Never';
    
    const seconds = Math.floor((new Date() - lastSyncTime) / 1000);
    
    if (seconds < 60) return `${seconds}s ago`;
    if (seconds < 3600) return `${Math.floor(seconds / 60)}m ago`;
    return `${Math.floor(seconds / 3600)}h ago`;
  };

  if (loading) {
    return (
      <div className="flex justify-center py-12">
        <div className="animate-spin rounded-full h-12 w-12 border-t-2 border-b-2 border-agora-blue"></div>
      </div>
    );
  }

  if (error) {
    return (
      <div className="text-center py-12">
        <XCircleIcon className="h-12 w-12 text-red-500 mx-auto mb-4" />
        <h3 className="text-lg font-medium text-gray-900">Error loading dashboard</h3>
        <p className="mt-1 text-sm text-gray-500">{error}</p>
      </div>
    );
  }

  return (
    <div>
      <div className="mb-8">
        <h1 className="text-3xl font-bold text-gray-900">
          {isAuthenticated ? `Welcome, ${userDid?.substring(0, 16)}...` : 'Welcome to AgoraNet Dashboard'}
        </h1>
        <p className="mt-2 text-gray-600">
          {federationId 
            ? `Viewing federation: ${federationId}`
            : 'View and manage federation proposals, deliberation threads, and execution receipts'}
        </p>
      </div>
      
      {/* Real-time updates banner */}
      {updatedProposals.length > 0 && (
        <div className="mb-8 bg-yellow-50 border border-yellow-100 rounded-md p-4">
          <div className="flex">
            <div className="flex-shrink-0">
              <BellAlertIcon className="h-5 w-5 text-yellow-400" />
            </div>
            <div className="ml-3">
              <h3 className="text-sm font-medium text-yellow-800">Recent Updates Detected</h3>
              <div className="mt-2 text-sm text-yellow-700">
                <p>
                  {updatedProposals.length} proposal(s) have been updated through DAG anchoring.
                </p>
                {updatedProposals.length > 0 && (
                  <ul className="mt-1 list-disc list-inside">
                    {updatedProposals.slice(0, 3).map(proposal => (
                      <li key={proposal.id}>
                        <Link 
                          to={`/proposals/${proposal.id}`} 
                          className="hover:underline text-yellow-900"
                        >
                          {proposal.title || proposal.id}
                        </Link>
                      </li>
                    ))}
                    {updatedProposals.length > 3 && (
                      <li>
                        <Link 
                          to="/proposals" 
                          className="hover:underline text-yellow-900"
                        >
                          ... and {updatedProposals.length - 3} more
                        </Link>
                      </li>
                    )}
                  </ul>
                )}
              </div>
            </div>
          </div>
        </div>
      )}
      
      {/* DAG Anchor Info */}
      {isAuthenticated && latestAnchor && (
        <div className="mb-8 bg-white shadow overflow-hidden sm:rounded-md p-4">
          <div className="flex items-center border-b border-gray-200 pb-3">
            <LinkIcon className="h-5 w-5 text-agora-blue mr-2" />
            <span className="font-medium text-gray-700">Current DAG Anchor</span>
            <span className="ml-auto text-xs text-gray-500">Last synced: {getTimeAgo()}</span>
          </div>
          <div className="mt-3 grid grid-cols-1 md:grid-cols-3 gap-4">
            <div>
              <div className="text-xs text-gray-500">CID</div>
              <div className="font-mono text-sm truncate">{latestAnchor.cid}</div>
            </div>
            <div>
              <div className="text-xs text-gray-500">Timestamp</div>
              <div className="text-sm">
                {latestAnchor.timestamp ? new Date(latestAnchor.timestamp).toLocaleString() : 'Unknown'}
              </div>
            </div>
            <div>
              <div className="text-xs text-gray-500">Height</div>
              <div className="text-sm">{latestAnchor.height || 'Unknown'}</div>
            </div>
          </div>
        </div>
      )}
      
      {/* Stats cards */}
      <div className="grid grid-cols-1 gap-5 sm:grid-cols-2 lg:grid-cols-4 mb-8">
        {/* Total Proposals */}
        <div className="bg-white overflow-hidden shadow rounded-lg">
          <div className="p-5">
            <div className="flex items-center">
              <div className="flex-shrink-0">
                <DocumentTextIcon className="h-6 w-6 text-gray-400" />
              </div>
              <div className="ml-5 w-0 flex-1">
                <dl>
                  <dt className="text-sm font-medium text-gray-500 truncate">Total Proposals</dt>
                  <dd>
                    <div className="text-lg font-medium text-gray-900">{stats.totalProposals}</div>
                  </dd>
                </dl>
              </div>
            </div>
          </div>
        </div>
        
        {/* Executed Proposals */}
        <div className="bg-white overflow-hidden shadow rounded-lg">
          <div className="p-5">
            <div className="flex items-center">
              <div className="flex-shrink-0">
                <CheckCircleIcon className="h-6 w-6 text-green-400" />
              </div>
              <div className="ml-5 w-0 flex-1">
                <dl>
                  <dt className="text-sm font-medium text-gray-500 truncate">Executed Proposals</dt>
                  <dd>
                    <div className="text-lg font-medium text-gray-900">{stats.executedProposals}</div>
                  </dd>
                </dl>
              </div>
            </div>
          </div>
        </div>
        
        {/* Active Proposals */}
        <div className="bg-white overflow-hidden shadow rounded-lg">
          <div className="p-5">
            <div className="flex items-center">
              <div className="flex-shrink-0">
                <ClockIcon className="h-6 w-6 text-blue-400" />
              </div>
              <div className="ml-5 w-0 flex-1">
                <dl>
                  <dt className="text-sm font-medium text-gray-500 truncate">Active Proposals</dt>
                  <dd>
                    <div className="text-lg font-medium text-gray-900">{stats.activeProposals}</div>
                  </dd>
                </dl>
              </div>
            </div>
          </div>
        </div>
        
        {/* Linked Threads */}
        <div className="bg-white overflow-hidden shadow rounded-lg">
          <div className="p-5">
            <div className="flex items-center">
              <div className="flex-shrink-0">
                <ChatBubbleLeftRightIcon className="h-6 w-6 text-yellow-400" />
              </div>
              <div className="ml-5 w-0 flex-1">
                <dl>
                  <dt className="text-sm font-medium text-gray-500 truncate">Linked Threads</dt>
                  <dd>
                    <div className="text-lg font-medium text-gray-900">{stats.linkedThreads}</div>
                  </dd>
                </dl>
              </div>
            </div>
          </div>
        </div>
      </div>
      
      {/* Recent Proposals */}
      <div className="bg-white shadow overflow-hidden sm:rounded-md">
        <div className="px-4 py-5 sm:px-6 flex justify-between items-center">
          <h3 className="text-lg leading-6 font-medium text-gray-900">Recent Proposals</h3>
          <Link to="/proposals" className="text-sm font-medium text-agora-blue hover:text-blue-700">
            View all
          </Link>
        </div>
        <ul className="divide-y divide-gray-200">
          {recentProposals.length === 0 ? (
            <li className="px-4 py-4 sm:px-6">
              <div className="text-center py-6">
                <p className="text-gray-500">No proposals found</p>
                <Link to="/proposals/new" className="mt-2 inline-block text-agora-blue hover:text-blue-700">
                  Create a new proposal
                </Link>
              </div>
            </li>
          ) : (
            recentProposals.map(proposal => {
              const isUpdated = updatedProposals.some(p => p.id === proposal.id);
              return (
                <li key={proposal.id} className={isUpdated ? "hover:bg-gray-50 bg-yellow-50" : "hover:bg-gray-50"}>
                  <Link to={`/proposals/${proposal.id}`} className="block">
                    <div className="px-4 py-4 sm:px-6">
                      <div className="flex items-center justify-between">
                        <div className="truncate">
                          <div className="flex">
                            <p className="text-sm font-medium text-agora-blue truncate">{proposal.title}</p>
                            {isUpdated && (
                              <span className="ml-2 flex items-center text-yellow-600 text-xs">
                                <BellAlertIcon className="h-4 w-4 mr-1" />
                                Updated
                              </span>
                            )}
                          </div>
                          <div className="mt-2 flex">
                            <div className="flex items-center text-sm text-gray-500">
                              <p>
                                {proposal.createdAt 
                                  ? new Date(proposal.createdAt).toLocaleDateString() 
                                  : 'Unknown date'}
                              </p>
                            </div>
                          </div>
                        </div>
                        <div>
                          {proposal.status === 'executed' && (
                            <span className="inline-flex items-center px-2.5 py-0.5 rounded-md text-sm font-medium bg-green-100 text-green-800">
                              Executed
                            </span>
                          )}
                          {proposal.status === 'active' && (
                            <span className="inline-flex items-center px-2.5 py-0.5 rounded-md text-sm font-medium bg-blue-100 text-blue-800">
                              Active
                            </span>
                          )}
                          {proposal.status === 'rejected' && (
                            <span className="inline-flex items-center px-2.5 py-0.5 rounded-md text-sm font-medium bg-red-100 text-red-800">
                              Rejected
                            </span>
                          )}
                        </div>
                      </div>
                    </div>
                  </Link>
                </li>
              );
            })
          )}
        </ul>
      </div>
    </div>
  );
} 