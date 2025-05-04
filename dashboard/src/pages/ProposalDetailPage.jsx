import React, { useState, useEffect } from 'react';
import { useParams, Link } from 'react-router-dom';
import { 
  ArrowLeftIcon, 
  DocumentTextIcon, 
  ChatBubbleLeftRightIcon,
  ClockIcon,
  PlayIcon,
  BellAlertIcon
} from '@heroicons/react/24/outline';
import { proposalApi, credentialApi, dagApi } from '../services/runtimeApi';
import { threadApi } from '../services/agoranetApi';
import ReceiptViewer from '../components/ReceiptViewer';
import ThreadLinker from '../components/ThreadLinker';
import VotePanel from '../components/VotePanel';
import ReceiptMonitor from '../components/ReceiptMonitor';
import { useCredentials } from '../contexts/CredentialContext';
import { useDagSync } from '../contexts/DagSyncContext';

export default function ProposalDetailPage() {
  const { id } = useParams();
  const { hasPermission } = useCredentials();
  const { updatedProposals, clearUpdatedProposal } = useDagSync();
  
  const [proposal, setProposal] = useState(null);
  const [thread, setThread] = useState(null);
  const [receipt, setReceipt] = useState(null);
  const [dagHistory, setDagHistory] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const [activeTab, setActiveTab] = useState('details');
  const [isHighlighted, setIsHighlighted] = useState(false);

  // Check if the proposal has been updated via DAG
  useEffect(() => {
    const isUpdated = updatedProposals.some(p => p.id === id);
    if (isUpdated) {
      setIsHighlighted(true);
      // Clear the highlight after 5 seconds
      const timer = setTimeout(() => {
        setIsHighlighted(false);
        clearUpdatedProposal(id);
      }, 5000);
      
      return () => clearTimeout(timer);
    }
  }, [updatedProposals, id, clearUpdatedProposal]);

  // Load proposal details
  useEffect(() => {
    async function fetchProposalDetails() {
      try {
        setLoading(true);
        
        // Fetch proposal
        const proposalData = await proposalApi.getProposal(id);
        setProposal(proposalData);
        
        // If proposal has a thread ID, fetch the thread
        if (proposalData.threadId) {
          try {
            const threadData = await threadApi.getThread(proposalData.threadId);
            setThread(threadData);
          } catch (threadErr) {
            console.error('Error fetching thread:', threadErr);
            // Non-critical error, don't set main error state
          }
        }
        
        // Fetch execution receipt if available
        try {
          const receipts = await credentialApi.getReceiptsForProposal(id);
          if (receipts && receipts.length > 0) {
            setReceipt(receipts[0]);
          }
        } catch (receiptErr) {
          console.error('Error fetching receipt:', receiptErr);
          // Non-critical error, don't set main error state
        }
        
        // Fetch DAG history
        try {
          const history = await dagApi.getProposalDagHistory(id);
          setDagHistory(history);
        } catch (dagErr) {
          console.error('Error fetching DAG history:', dagErr);
          // Non-critical error, don't set main error state
        }
      } catch (err) {
        console.error('Error fetching proposal details:', err);
        setError('Failed to load proposal details');
      } finally {
        setLoading(false);
      }
    }
    
    fetchProposalDetails();
  }, [id]);

  // Execute proposal
  const handleExecuteProposal = async () => {
    try {
      setLoading(true);
      await proposalApi.executeProposal(id);
      
      // Refetch proposal to get updated status
      const updatedProposal = await proposalApi.getProposal(id);
      setProposal(updatedProposal);
      
      // Show the receipt tab
      setActiveTab('receipt');
    } catch (err) {
      console.error('Error executing proposal:', err);
      setError('Failed to execute proposal');
    } finally {
      setLoading(false);
    }
  };

  // Handle thread linking
  const handleThreadLinked = async (threadId) => {
    try {
      // Refetch proposal to get updated thread ID
      const updatedProposal = await proposalApi.getProposal(id);
      setProposal({
        ...updatedProposal,
        threadId
      });
      
      // Fetch the thread details
      const threadData = await threadApi.getThread(threadId);
      setThread(threadData);
    } catch (err) {
      console.error('Error updating after thread link:', err);
    }
  };
  
  // Handle receipt found
  const handleReceiptFound = (newReceipt) => {
    setReceipt(newReceipt);
    
    // Switch to receipt tab
    setActiveTab('receipt');
  };
  
  // Handle vote submitted
  const handleVoteSubmitted = async (vote) => {
    // Refetch proposal after voting
    try {
      const updatedProposal = await proposalApi.getProposal(id);
      setProposal(updatedProposal);
    } catch (err) {
      console.error('Error fetching updated proposal after vote:', err);
    }
  };

  // Loading state
  if (loading && !proposal) {
    return (
      <div className="flex justify-center py-12">
        <div className="animate-spin rounded-full h-12 w-12 border-t-2 border-b-2 border-agora-blue"></div>
      </div>
    );
  }

  // Error state
  if (error && !proposal) {
    return (
      <div className="text-center py-12">
        <DocumentTextIcon className="h-12 w-12 text-red-500 mx-auto mb-4" />
        <h3 className="text-lg font-medium text-gray-900">Error loading proposal</h3>
        <p className="mt-1 text-sm text-gray-500">{error}</p>
        <Link to="/proposals" className="mt-4 text-agora-blue hover:text-blue-700">
          Back to proposals
        </Link>
      </div>
    );
  }

  return (
    <div className={isHighlighted ? "bg-yellow-50 p-4 rounded-lg transition-colors duration-1000" : ""}>
      {/* Header */}
      <div className="mb-5">
        <div className="flex items-center mb-4">
          <Link to="/proposals" className="mr-4 text-gray-500 hover:text-gray-700">
            <ArrowLeftIcon className="h-5 w-5" />
          </Link>
          <h1 className="text-2xl font-bold text-gray-900">{proposal?.title || 'Proposal Details'}</h1>
          
          {isHighlighted && (
            <div className="ml-3 flex items-center text-yellow-700 text-sm">
              <BellAlertIcon className="h-5 w-5 mr-1" />
              Updated via DAG
            </div>
          )}
        </div>
        
        <div className="flex flex-wrap items-center text-sm text-gray-500 space-x-4">
          <div className="flex items-center">
            <DocumentTextIcon className="h-4 w-4 mr-1" />
            <span>ID: {id}</span>
          </div>
          
          {proposal?.creatorDid && (
            <div className="flex items-center">
              <span>Creator: {proposal.creatorDid.substring(0, 16)}...</span>
            </div>
          )}
          
          {proposal?.status && (
            <div className="flex items-center">
              <ClockIcon className="h-4 w-4 mr-1" />
              <span>Status: {proposal.status}</span>
            </div>
          )}
          
          {proposal?.threadId && (
            <div className="flex items-center">
              <ChatBubbleLeftRightIcon className="h-4 w-4 mr-1" />
              <Link to={`/threads/${proposal.threadId}`} className="text-agora-blue hover:underline">
                Thread: {proposal.threadId.substring(0, 8)}...
              </Link>
            </div>
          )}
        </div>
      </div>
      
      {/* Actions */}
      <div className="mb-6 flex space-x-4">
        {proposal?.status === 'active' && hasPermission('execute_proposal') && (
          <button
            onClick={handleExecuteProposal}
            disabled={loading}
            className="btn btn-primary flex items-center"
          >
            {loading ? (
              <div className="animate-spin rounded-full h-4 w-4 border-t-2 border-b-2 border-white mr-2"></div>
            ) : (
              <PlayIcon className="h-5 w-5 mr-2" />
            )}
            Execute Proposal
          </button>
        )}
      </div>
      
      {/* Receipt Monitor */}
      {proposal?.status === 'active' && !receipt && (
        <div className="mb-6">
          <ReceiptMonitor 
            proposalId={id} 
            onReceiptFound={handleReceiptFound}
          />
        </div>
      )}
      
      {/* Two column layout for voting and tabs */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-6 mb-6">
        {/* Left column for tabs */}
        <div className="md:col-span-2">
          {/* Tabs */}
          <div className="border-b border-gray-200">
            <nav className="-mb-px flex">
              <button
                className={`${
                  activeTab === 'details'
                    ? 'border-agora-blue text-agora-blue'
                    : 'border-transparent text-gray-500 hover:border-gray-300 hover:text-gray-700'
                } whitespace-nowrap py-4 px-1 border-b-2 font-medium text-sm mr-8`}
                onClick={() => setActiveTab('details')}
              >
                Proposal Details
              </button>
              
              <button
                className={`${
                  activeTab === 'ccl'
                    ? 'border-agora-blue text-agora-blue'
                    : 'border-transparent text-gray-500 hover:border-gray-300 hover:text-gray-700'
                } whitespace-nowrap py-4 px-1 border-b-2 font-medium text-sm mr-8`}
                onClick={() => setActiveTab('ccl')}
              >
                CCL Code
              </button>
              
              <button
                className={`${
                  activeTab === 'thread'
                    ? 'border-agora-blue text-agora-blue'
                    : 'border-transparent text-gray-500 hover:border-gray-300 hover:text-gray-700'
                } whitespace-nowrap py-4 px-1 border-b-2 font-medium text-sm mr-8`}
                onClick={() => setActiveTab('thread')}
              >
                Thread
              </button>
              
              <button
                className={`${
                  activeTab === 'receipt'
                    ? 'border-agora-blue text-agora-blue'
                    : 'border-transparent text-gray-500 hover:border-gray-300 hover:text-gray-700'
                } whitespace-nowrap py-4 px-1 border-b-2 font-medium text-sm`}
                onClick={() => setActiveTab('receipt')}
              >
                Receipt
              </button>
            </nav>
          </div>
          
          {/* Tab content */}
          <div className="mt-6">
            {/* Details tab */}
            {activeTab === 'details' && (
              <div className="bg-white shadow overflow-hidden sm:rounded-md">
                <div className="px-4 py-5 sm:px-6">
                  <h3 className="text-lg leading-6 font-medium text-gray-900">
                    Proposal Information
                  </h3>
                  <p className="mt-1 max-w-2xl text-sm text-gray-500">
                    Details about this governance proposal
                  </p>
                </div>
                <div className="border-t border-gray-200">
                  <dl>
                    <div className="bg-gray-50 px-4 py-5 sm:grid sm:grid-cols-3 sm:gap-4 sm:px-6">
                      <dt className="text-sm font-medium text-gray-500">Title</dt>
                      <dd className="mt-1 text-sm text-gray-900 sm:mt-0 sm:col-span-2">
                        {proposal?.title}
                      </dd>
                    </div>
                    <div className="bg-white px-4 py-5 sm:grid sm:grid-cols-3 sm:gap-4 sm:px-6">
                      <dt className="text-sm font-medium text-gray-500">Description</dt>
                      <dd className="mt-1 text-sm text-gray-900 sm:mt-0 sm:col-span-2">
                        {proposal?.description || 'No description available'}
                      </dd>
                    </div>
                    <div className="bg-gray-50 px-4 py-5 sm:grid sm:grid-cols-3 sm:gap-4 sm:px-6">
                      <dt className="text-sm font-medium text-gray-500">Status</dt>
                      <dd className="mt-1 text-sm text-gray-900 sm:mt-0 sm:col-span-2">
                        {proposal?.status || 'Unknown'}
                      </dd>
                    </div>
                    <div className="bg-white px-4 py-5 sm:grid sm:grid-cols-3 sm:gap-4 sm:px-6">
                      <dt className="text-sm font-medium text-gray-500">Creator</dt>
                      <dd className="mt-1 text-sm text-gray-900 sm:mt-0 sm:col-span-2">
                        {proposal?.creatorDid || 'Unknown'}
                      </dd>
                    </div>
                    <div className="bg-gray-50 px-4 py-5 sm:grid sm:grid-cols-3 sm:gap-4 sm:px-6">
                      <dt className="text-sm font-medium text-gray-500">Federation</dt>
                      <dd className="mt-1 text-sm text-gray-900 sm:mt-0 sm:col-span-2">
                        {proposal?.federationId || 'Not specified'}
                      </dd>
                    </div>
                    <div className="bg-white px-4 py-5 sm:grid sm:grid-cols-3 sm:gap-4 sm:px-6">
                      <dt className="text-sm font-medium text-gray-500">Voting Results</dt>
                      <dd className="mt-1 text-sm text-gray-900 sm:mt-0 sm:col-span-2">
                        {proposal?.votesFor !== undefined ? (
                          <div className="flex space-x-4">
                            <div>✅ For: {proposal.votesFor}</div>
                            <div>❌ Against: {proposal.votesAgainst}</div>
                            <div>⚪ Abstain: {proposal.votesAbstain}</div>
                          </div>
                        ) : (
                          'No voting data available'
                        )}
                      </dd>
                    </div>
                    
                    {/* DAG History */}
                    {dagHistory && dagHistory.length > 0 && (
                      <div className="bg-gray-50 px-4 py-5 sm:grid sm:grid-cols-3 sm:gap-4 sm:px-6">
                        <dt className="text-sm font-medium text-gray-500">DAG History</dt>
                        <dd className="mt-1 text-sm text-gray-900 sm:mt-0 sm:col-span-2">
                          <ul className="space-y-2">
                            {dagHistory.map((event, idx) => (
                              <li key={idx} className="text-xs p-2 bg-white rounded-md flex justify-between">
                                <span>{event.type}</span>
                                <span className="text-gray-500">{new Date(event.timestamp).toLocaleString()}</span>
                                {event.cid && (
                                  <span className="font-mono">{event.cid.substring(0, 8)}...</span>
                                )}
                              </li>
                            ))}
                          </ul>
                        </dd>
                      </div>
                    )}
                  </dl>
                </div>
              </div>
            )}
            
            {/* CCL Code tab */}
            {activeTab === 'ccl' && (
              <div className="bg-white shadow overflow-hidden sm:rounded-md">
                <div className="px-4 py-5 sm:px-6">
                  <h3 className="text-lg leading-6 font-medium text-gray-900">
                    CCL Source Code
                  </h3>
                  <p className="mt-1 max-w-2xl text-sm text-gray-500">
                    Civic Code Language defining this proposal's execution
                  </p>
                </div>
                <div className="border-t border-gray-200 p-4">
                  {proposal?.cclCode ? (
                    <pre className="p-4 bg-gray-50 rounded-md text-sm overflow-auto max-h-96 font-mono">
                      {proposal.cclCode}
                    </pre>
                  ) : (
                    <div className="text-center py-8 text-gray-500">
                      No CCL code available for this proposal
                    </div>
                  )}
                </div>
              </div>
            )}
            
            {/* Thread tab */}
            {activeTab === 'thread' && (
              <div>
                {proposal?.threadId && thread ? (
                  <div className="bg-white shadow overflow-hidden sm:rounded-md">
                    <div className="px-4 py-5 sm:px-6 flex justify-between items-center">
                      <div>
                        <h3 className="text-lg leading-6 font-medium text-gray-900">
                          {thread.title}
                        </h3>
                        <p className="mt-1 max-w-2xl text-sm text-gray-500">
                          Deliberation thread for this proposal
                        </p>
                      </div>
                      <Link
                        to={`/threads/${proposal.threadId}`}
                        target="_blank"
                        rel="noopener noreferrer"
                        className="btn btn-secondary"
                      >
                        Open Thread
                      </Link>
                    </div>
                    <div className="border-t border-gray-200 p-4">
                      <div className="bg-gray-50 p-4 rounded-md">
                        <p className="text-sm text-gray-500">
                          Thread preview not available in this view. Click the button above to open the full thread.
                        </p>
                      </div>
                    </div>
                  </div>
                ) : (
                  <ThreadLinker
                    proposalId={id}
                    currentThreadId={proposal?.threadId}
                    onLink={handleThreadLinked}
                  />
                )}
              </div>
            )}
            
            {/* Receipt tab */}
            {activeTab === 'receipt' && (
              <div>
                {receipt ? (
                  <ReceiptViewer receipt={receipt} />
                ) : (
                  <div className="bg-white shadow overflow-hidden sm:rounded-md p-8 text-center">
                    <DocumentTextIcon className="h-12 w-12 text-gray-400 mx-auto mb-4" />
                    <h3 className="text-lg font-medium text-gray-900">No execution receipt available</h3>
                    <p className="mt-1 text-sm text-gray-500">
                      This proposal hasn't been executed yet or the receipt hasn't been generated.
                    </p>
                    {proposal?.status === 'active' && hasPermission('execute_proposal') && (
                      <button
                        onClick={handleExecuteProposal}
                        className="mt-4 btn btn-primary"
                      >
                        Execute Proposal
                      </button>
                    )}
                  </div>
                )}
              </div>
            )}
          </div>
        </div>
        
        {/* Right column for voting */}
        <div className="md:col-span-1">
          {/* Voting Panel */}
          {!receipt && proposal?.status === 'active' && (
            <VotePanel 
              proposalId={id} 
              onVoteSuccess={handleVoteSubmitted}
              disabled={loading} 
            />
          )}
          
          {/* Execution status when complete */}
          {receipt && (
            <div className="bg-green-50 p-4 rounded-md border border-green-200 mb-4">
              <div className="flex items-center">
                <CheckCircleIcon className="h-5 w-5 text-green-500 mr-2" />
                <span className="text-green-700 font-medium">Proposal Executed</span>
              </div>
              <p className="mt-2 text-sm text-green-600">
                This proposal has been executed and a receipt has been verified.
                View the full receipt details in the Receipt tab.
              </p>
            </div>
          )}
        </div>
      </div>
    </div>
  );
} 