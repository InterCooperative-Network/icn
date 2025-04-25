import React, { useState, useEffect } from 'react';
import axios from 'axios';
import { getCredentialLinkedThreads } from '../../packages/credential-utils/utils/proposalLinking';

interface Thread {
  id: string;
  title: string;
  content: string;
  author_did: string;
  created_at: string;
  updated_at: string;
  tags: string[];
  proposal_id?: string;
  federation_id?: string;
  status: 'Open' | 'Closed' | 'Archived' | 'Hidden';
}

interface ThreadSearchViewProps {
  agoraNetEndpoint: string;
  federations: { id: string; name: string }[];
  onThreadSelect?: (threadId: string) => void;
  userCredentialIds?: string[]; // Optional array of credential IDs to highlight related threads
}

/**
 * A component for searching and displaying AgoraNet discussion threads
 */
export const ThreadSearchView: React.FC<ThreadSearchViewProps> = ({
  agoraNetEndpoint,
  federations,
  onThreadSelect,
  userCredentialIds = []
}) => {
  const [searchQuery, setSearchQuery] = useState<string>('');
  const [selectedFederation, setSelectedFederation] = useState<string>('all');
  const [proposalFilter, setProposalFilter] = useState<string>('');
  const [threads, setThreads] = useState<Thread[]>([]);
  const [loading, setLoading] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [userRelatedThreads, setUserRelatedThreads] = useState<Set<string>>(new Set());
  
  // Fetch all threads
  const fetchThreads = async () => {
    setLoading(true);
    setError(null);
    try {
      // Build query parameters
      const params: Record<string, string> = {};
      
      if (searchQuery) {
        params.query = searchQuery;
      }
      
      if (selectedFederation && selectedFederation !== 'all') {
        params.federation_id = selectedFederation;
      }
      
      if (proposalFilter) {
        params.proposal_id = proposalFilter;
      }
      
      // Make API request
      const endpoint = `${agoraNetEndpoint.replace(/\/$/, '')}/api/threads`;
      const response = await axios.get(endpoint, { params });
      
      setThreads(response.data.threads || []);
    } catch (err) {
      console.error('Error fetching threads:', err);
      setError('Failed to fetch threads. Please try again.');
    } finally {
      setLoading(false);
    }
  };
  
  // Find threads linked to user's credentials
  const findUserRelatedThreads = async () => {
    if (!userCredentialIds.length) return;
    
    const relatedThreadIds = new Set<string>();
    
    try {
      for (const credId of userCredentialIds) {
        const linkedThreads = await getCredentialLinkedThreads(agoraNetEndpoint, credId);
        linkedThreads.forEach(thread => relatedThreadIds.add(thread.threadId));
      }
      
      setUserRelatedThreads(relatedThreadIds);
    } catch (error) {
      console.error('Error finding related threads:', error);
    }
  };
  
  // Fetch threads on component mount and when search parameters change
  useEffect(() => {
    fetchThreads();
  }, []);
  
  // Find user's related threads on mount
  useEffect(() => {
    findUserRelatedThreads();
  }, [userCredentialIds]);
  
  // Handler for search button click
  const handleSearch = () => {
    fetchThreads();
  };
  
  // Handle thread selection
  const handleThreadClick = (threadId: string) => {
    if (onThreadSelect) {
      onThreadSelect(threadId);
    } else {
      // Default behavior - open thread in new tab
      window.open(`${agoraNetEndpoint}/threads/${threadId}`, '_blank');
    }
  };
  
  return (
    <div className="thread-search-container">
      <div className="search-controls">
        <div className="search-bar">
          <input
            type="text"
            placeholder="Search threads..."
            value={searchQuery}
            onChange={(e) => setSearchQuery(e.target.value)}
            className="search-input"
          />
        </div>
        
        <div className="filter-controls">
          <div className="filter-group">
            <label>Federation:</label>
            <select 
              value={selectedFederation} 
              onChange={(e) => setSelectedFederation(e.target.value)}
            >
              <option value="all">All Federations</option>
              {federations.map(fed => (
                <option key={fed.id} value={fed.id}>
                  {fed.name || fed.id}
                </option>
              ))}
            </select>
          </div>
          
          <div className="filter-group">
            <label>Proposal ID:</label>
            <input
              type="text"
              placeholder="Filter by proposal ID"
              value={proposalFilter}
              onChange={(e) => setProposalFilter(e.target.value)}
            />
          </div>
          
          <button 
            className="search-button"
            onClick={handleSearch}
            disabled={loading}
          >
            {loading ? 'Searching...' : 'Search'}
          </button>
        </div>
      </div>
      
      {error && (
        <div className="error-message">
          {error}
        </div>
      )}
      
      <div className="threads-list">
        {threads.length === 0 ? (
          <div className="empty-message">
            {loading ? 'Loading threads...' : 'No threads found matching your criteria.'}
          </div>
        ) : (
          threads.map(thread => (
            <div 
              key={thread.id} 
              className={`thread-card ${userRelatedThreads.has(thread.id) ? 'user-related' : ''}`}
              onClick={() => handleThreadClick(thread.id)}
            >
              <div className="thread-header">
                <h3>{thread.title}</h3>
                <span className={`status-badge ${thread.status.toLowerCase()}`}>
                  {thread.status}
                </span>
              </div>
              
              <div className="thread-content">
                <p className="preview">
                  {thread.content.substring(0, 150)}
                  {thread.content.length > 150 ? '...' : ''}
                </p>
                
                {thread.proposal_id && (
                  <div className="proposal-tag">
                    Proposal: {thread.proposal_id}
                  </div>
                )}
                
                {thread.federation_id && (
                  <div className="federation-tag">
                    Federation: {
                      federations.find(f => f.id === thread.federation_id)?.name || 
                      thread.federation_id
                    }
                  </div>
                )}
                
                <div className="tags">
                  {thread.tags.map(tag => (
                    <span key={tag} className="tag">{tag}</span>
                  ))}
                </div>
              </div>
              
              <div className="thread-footer">
                <span className="created-at">
                  Created: {new Date(thread.created_at).toLocaleDateString()}
                </span>
                
                {userRelatedThreads.has(thread.id) && (
                  <span className="user-contributed-badge">
                    You contributed
                  </span>
                )}
              </div>
            </div>
          ))
        )}
      </div>
      
      <style jsx>{`
        .thread-search-container {
          max-width: 100%;
        }
        
        .search-controls {
          margin-bottom: 20px;
          padding: 16px;
          background-color: #f8f9fa;
          border-radius: 8px;
        }
        
        .search-bar {
          margin-bottom: 16px;
        }
        
        .search-input {
          width: 100%;
          padding: 10px 16px;
          border: 1px solid #ced4da;
          border-radius: 4px;
          font-size: 16px;
        }
        
        .filter-controls {
          display: flex;
          gap: 16px;
          flex-wrap: wrap;
          align-items: flex-end;
        }
        
        .filter-group {
          display: flex;
          flex-direction: column;
          gap: 4px;
          flex: 1;
        }
        
        select, input {
          padding: 8px 12px;
          border: 1px solid #ced4da;
          border-radius: 4px;
          background-color: white;
        }
        
        .search-button {
          padding: 8px 16px;
          background-color: #007bff;
          color: white;
          border: none;
          border-radius: 4px;
          cursor: pointer;
        }
        
        .search-button:disabled {
          background-color: #6c757d;
        }
        
        .error-message {
          padding: 12px;
          background-color: #f8d7da;
          color: #721c24;
          border-radius: 4px;
          margin-bottom: 16px;
        }
        
        .empty-message {
          padding: 20px;
          text-align: center;
          background-color: #f8f9fa;
          border-radius: 8px;
        }
        
        .threads-list {
          display: flex;
          flex-direction: column;
          gap: 16px;
        }
        
        .thread-card {
          padding: 16px;
          background-color: white;
          border-radius: 8px;
          box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1);
          cursor: pointer;
          transition: transform 0.2s;
        }
        
        .thread-card:hover {
          transform: translateY(-2px);
          box-shadow: 0 4px 8px rgba(0, 0, 0, 0.1);
        }
        
        .thread-card.user-related {
          border-left: 4px solid #28a745;
        }
        
        .thread-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          margin-bottom: 12px;
        }
        
        .thread-header h3 {
          margin: 0;
          font-size: 18px;
          font-weight: 600;
        }
        
        .status-badge {
          padding: 4px 8px;
          border-radius: 4px;
          font-size: 12px;
          font-weight: 500;
        }
        
        .status-badge.open {
          background-color: #d4edda;
          color: #155724;
        }
        
        .status-badge.closed {
          background-color: #f8d7da;
          color: #721c24;
        }
        
        .status-badge.archived {
          background-color: #e2e3e5;
          color: #383d41;
        }
        
        .thread-content {
          margin-bottom: 12px;
        }
        
        .preview {
          color: #495057;
          margin-bottom: 8px;
        }
        
        .proposal-tag, .federation-tag {
          display: inline-block;
          padding: 4px 8px;
          background-color: #e2e3e5;
          color: #383d41;
          border-radius: 4px;
          font-size: 12px;
          margin-right: 8px;
          margin-bottom: 8px;
        }
        
        .tags {
          display: flex;
          flex-wrap: wrap;
          gap: 6px;
          margin-top: 8px;
        }
        
        .tag {
          background-color: #f1f8ff;
          color: #0366d6;
          padding: 2px 8px;
          border-radius: 4px;
          font-size: 12px;
        }
        
        .thread-footer {
          display: flex;
          justify-content: space-between;
          font-size: 12px;
          color: #6c757d;
        }
        
        .user-contributed-badge {
          padding: 2px 6px;
          background-color: #dff0d8;
          color: #3c763d;
          border-radius: 4px;
          font-weight: 500;
        }
        
        @media (max-width: 768px) {
          .filter-controls {
            flex-direction: column;
            gap: 12px;
          }
          
          .filter-group {
            width: 100%;
          }
        }
      `}</style>
    </div>
  );
}; 