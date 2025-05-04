import React, { useState, useEffect } from 'react';
import { ChatBubbleLeftRightIcon, LinkIcon, XMarkIcon } from '@heroicons/react/24/outline';
import { threadApi } from '../services/agoranetApi';
import { useCredentials } from '../contexts/CredentialContext';

export default function ThreadLinker({ proposalId, currentThreadId, onLink }) {
  const { hasPermission } = useCredentials();
  const [threads, setThreads] = useState([]);
  const [selectedThreadId, setSelectedThreadId] = useState(currentThreadId || '');
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState(null);
  const [success, setSuccess] = useState(false);

  // Load threads when component mounts
  useEffect(() => {
    async function fetchThreads() {
      try {
        setLoading(true);
        const response = await threadApi.getThreads();
        // Filter out threads that already have proposals linked
        const availableThreads = response.filter(thread => !thread.proposal_cid || thread.id === currentThreadId);
        setThreads(availableThreads);
      } catch (err) {
        console.error('Error fetching threads:', err);
        setError('Failed to load deliberation threads');
      } finally {
        setLoading(false);
      }
    }

    fetchThreads();
  }, [currentThreadId]);

  // Link the proposal to the selected thread
  const handleLinkThread = async () => {
    if (!selectedThreadId) return;
    
    try {
      setLoading(true);
      await threadApi.linkProposal(selectedThreadId, proposalId);
      setSuccess(true);
      
      // Call onLink callback if provided
      if (onLink) {
        onLink(selectedThreadId);
      }
    } catch (err) {
      console.error('Error linking thread:', err);
      setError('Failed to link thread to proposal');
    } finally {
      setLoading(false);
    }
  };

  // Create a new thread and link to proposal
  const handleCreateThread = async (title) => {
    try {
      setLoading(true);
      // Create new thread
      const newThread = await threadApi.createThread({
        title: title || `Deliberation: ${proposalId}`,
        proposal_cid: proposalId
      });
      
      // Update state with new thread
      setThreads([...threads, newThread]);
      setSelectedThreadId(newThread.id);
      setSuccess(true);
      
      // Call onLink callback if provided
      if (onLink) {
        onLink(newThread.id);
      }
    } catch (err) {
      console.error('Error creating thread:', err);
      setError('Failed to create deliberation thread');
    } finally {
      setLoading(false);
    }
  };

  // Reset state when done
  const handleDone = () => {
    setSuccess(false);
    setError(null);
  };

  // If no permission to link threads
  if (!hasPermission('link_thread')) {
    return (
      <div className="bg-gray-50 p-4 rounded-md">
        <div className="flex items-center">
          <ChatBubbleLeftRightIcon className="h-6 w-6 text-gray-400 mr-2" />
          <span className="text-gray-500">You don't have permission to link deliberation threads</span>
        </div>
      </div>
    );
  }

  return (
    <div className="bg-white shadow sm:rounded-lg">
      <div className="px-4 py-5 sm:p-6">
        <h3 className="text-lg leading-6 font-medium text-gray-900">
          Link to Deliberation Thread
        </h3>
        <div className="mt-2 max-w-xl text-sm text-gray-500">
          <p>Connect this proposal to an AgoraNet deliberation thread</p>
        </div>
        
        {success ? (
          <div className="mt-4 bg-green-50 p-4 rounded-md">
            <div className="flex">
              <div className="flex-shrink-0">
                <LinkIcon className="h-5 w-5 text-green-400" aria-hidden="true" />
              </div>
              <div className="ml-3">
                <p className="text-sm font-medium text-green-800">
                  Successfully linked proposal to thread
                </p>
              </div>
              <div className="ml-auto pl-3">
                <div className="-mx-1.5 -my-1.5">
                  <button
                    type="button"
                    onClick={handleDone}
                    className="inline-flex rounded-md p-1.5 text-green-500 hover:bg-green-100 focus:outline-none"
                  >
                    <XMarkIcon className="h-5 w-5" />
                  </button>
                </div>
              </div>
            </div>
          </div>
        ) : error ? (
          <div className="mt-4 bg-red-50 p-4 rounded-md">
            <div className="flex">
              <div className="ml-3">
                <p className="text-sm font-medium text-red-800">{error}</p>
              </div>
              <div className="ml-auto pl-3">
                <div className="-mx-1.5 -my-1.5">
                  <button
                    type="button"
                    onClick={() => setError(null)}
                    className="inline-flex rounded-md p-1.5 text-red-500 hover:bg-red-100 focus:outline-none"
                  >
                    <XMarkIcon className="h-5 w-5" />
                  </button>
                </div>
              </div>
            </div>
          </div>
        ) : (
          <div className="mt-5">
            <div className="flex items-center">
              {loading ? (
                <div className="animate-spin rounded-full h-4 w-4 border-t-2 border-b-2 border-agora-blue mr-2"></div>
              ) : null}
              <select
                className="block w-full rounded-md border-gray-300 shadow-sm focus:border-indigo-500 focus:ring-indigo-500 sm:text-sm mr-4"
                value={selectedThreadId}
                onChange={(e) => setSelectedThreadId(e.target.value)}
                disabled={loading}
              >
                <option value="">Select a thread to link...</option>
                {threads.map((thread) => (
                  <option key={thread.id} value={thread.id}>
                    {thread.title}
                  </option>
                ))}
              </select>
              <button
                type="button"
                onClick={handleLinkThread}
                disabled={!selectedThreadId || loading}
                className="btn btn-primary disabled:opacity-50"
              >
                Link Thread
              </button>
            </div>
            <div className="mt-4">
              <button
                type="button"
                onClick={() => handleCreateThread()}
                disabled={loading}
                className="btn btn-secondary disabled:opacity-50"
              >
                Create New Thread
              </button>
            </div>
          </div>
        )}
      </div>
    </div>
  );
} 