import React, { useState, useEffect } from 'react';
import { WalletCredential } from '../../packages/credential-utils/types';
import { CredentialSearchBar } from './CredentialSearchBar';
import { ThreadSearchView } from './ThreadSearchView';
import { groupCredentialsByFederation } from '../../packages/credential-utils/utils/federation';

interface FederationSearchPageProps {
  credentials: WalletCredential[];
  agoraNetEndpoint: string;
  userDid: string;
}

/**
 * A unified federation-aware search page that combines credential and thread search
 */
export const FederationSearchPage: React.FC<FederationSearchPageProps> = ({
  credentials,
  agoraNetEndpoint,
  userDid
}) => {
  const [activeTab, setActiveTab] = useState<'credentials' | 'threads'>('credentials');
  const [filteredCredentials, setFilteredCredentials] = useState<WalletCredential[]>(credentials);
  const [searchQuery, setSearchQuery] = useState<string>('');
  const [activeFederation, setActiveFederation] = useState<string>('all');
  
  // Extract federations from credentials
  const extractFederations = () => {
    const federations = new Map<string, string>();
    
    credentials.forEach(cred => {
      if (cred.metadata?.federation?.id) {
        federations.set(
          cred.metadata.federation.id,
          cred.metadata.federation.name || cred.metadata.federation.id
        );
      } else if (cred.metadata?.agoranet?.federation_id) {
        federations.set(
          cred.metadata.agoranet.federation_id,
          cred.metadata.agoranet.federation_id
        );
      }
    });
    
    return Array.from(federations).map(([id, name]) => ({ id, name }));
  };
  
  const federations = extractFederations();
  
  // Group credentials by federation for collapsible display
  const groupedCredentials = groupCredentialsByFederation(filteredCredentials);
  
  // Extract user credential IDs for highlighting related threads
  const userCredentialIds = credentials.map(cred => cred.id);
  
  // Handle search results from CredentialSearchBar
  const handleCredentialSearchResults = (
    results: WalletCredential[], 
    query: string, 
    fedFilter: string
  ) => {
    setFilteredCredentials(results);
    setSearchQuery(query);
    setActiveFederation(fedFilter);
  };
  
  // Initialize with all credentials
  useEffect(() => {
    setFilteredCredentials(credentials);
  }, [credentials]);
  
  return (
    <div className="federation-search-page">
      <div className="tabs">
        <button 
          className={`tab-button ${activeTab === 'credentials' ? 'active' : ''}`}
          onClick={() => setActiveTab('credentials')}
        >
          Credentials
        </button>
        <button 
          className={`tab-button ${activeTab === 'threads' ? 'active' : ''}`}
          onClick={() => setActiveTab('threads')}
        >
          Discussion Threads
        </button>
      </div>
      
      <div className="tab-content">
        {activeTab === 'credentials' ? (
          <div className="credentials-tab">
            <h2>Search Governance Credentials</h2>
            <CredentialSearchBar 
              credentials={credentials}
              federations={federations}
              onSearchResults={handleCredentialSearchResults}
            />
            
            {/* Display search results */}
            <div className="search-results">
              <h3>
                {searchQuery ? `Results for "${searchQuery}"` : 'All Credentials'}
                {activeFederation !== 'all' && ` in ${federations.find(f => f.id === activeFederation)?.name || activeFederation}`}
              </h3>
              
              {Object.entries(groupedCredentials).length === 0 ? (
                <div className="empty-results">
                  No credentials found matching your criteria.
                </div>
              ) : (
                <div className="grouped-results">
                  {Object.entries(groupedCredentials).map(([federationId, federationCredentials]) => (
                    <div key={federationId} className="federation-group">
                      <div className="federation-header">
                        <h4>
                          {federationId === 'unfederated' 
                            ? 'Unfederated Credentials' 
                            : `Federation: ${federations.find(f => f.id === federationId)?.name || federationId}`}
                        </h4>
                        <span className="credential-count">
                          {federationCredentials.length} credential{federationCredentials.length !== 1 ? 's' : ''}
                        </span>
                      </div>
                      
                      <div className="credentials-list">
                        {federationCredentials.map(credential => (
                          <div key={credential.id} className="credential-card">
                            <div className="card-header">
                              <h5>{credential.title}</h5>
                              <span className={`trust-badge ${credential.trustLevel?.toLowerCase()}`}>
                                {credential.trustLevel}
                              </span>
                            </div>
                            
                            <div className="card-body">
                              <p className="issuer">Issued by: {credential.issuer.name || credential.issuer.did}</p>
                              <p className="date">Date: {new Date(credential.issuanceDate).toLocaleDateString()}</p>
                              
                              {credential.credentialSubject.proposalId && (
                                <p className="proposal-id">
                                  Proposal: {credential.credentialSubject.proposalId}
                                </p>
                              )}
                              
                              <div className="tags">
                                {credential.tags?.map(tag => (
                                  <span key={tag} className="tag">{tag}</span>
                                ))}
                              </div>
                            </div>
                            
                            <div className="card-links">
                              {credential.metadata?.agoranet?.threadUrl && (
                                <a 
                                  href={credential.metadata.agoranet.threadUrl}
                                  target="_blank"
                                  rel="noopener noreferrer"
                                  className="thread-link"
                                >
                                  View Discussion
                                </a>
                              )}
                            </div>
                          </div>
                        ))}
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          </div>
        ) : (
          <div className="threads-tab">
            <h2>Search Federation Discussions</h2>
            <ThreadSearchView 
              agoraNetEndpoint={agoraNetEndpoint}
              federations={federations}
              userCredentialIds={userCredentialIds}
            />
          </div>
        )}
      </div>
      
      <style jsx>{`
        .federation-search-page {
          max-width: 1200px;
          margin: 0 auto;
          padding: 20px;
        }
        
        .tabs {
          display: flex;
          margin-bottom: 20px;
          border-bottom: 1px solid #dee2e6;
        }
        
        .tab-button {
          padding: 10px 20px;
          background-color: transparent;
          border: none;
          border-bottom: 2px solid transparent;
          cursor: pointer;
          font-size: 16px;
          font-weight: 500;
        }
        
        .tab-button.active {
          border-bottom-color: #007bff;
          color: #007bff;
        }
        
        .tab-content {
          padding: 20px 0;
        }
        
        h2 {
          margin-top: 0;
          margin-bottom: 20px;
          font-size: 24px;
        }
        
        h3 {
          margin-top: 30px;
          margin-bottom: 20px;
          font-size: 20px;
        }
        
        .search-results {
          margin-top: 30px;
        }
        
        .empty-results {
          padding: 20px;
          text-align: center;
          background-color: #f8f9fa;
          border-radius: 8px;
        }
        
        .federation-group {
          margin-bottom: 30px;
        }
        
        .federation-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          padding: 10px 16px;
          background-color: #e9ecef;
          border-radius: 8px 8px 0 0;
        }
        
        .federation-header h4 {
          margin: 0;
          font-size: 18px;
        }
        
        .credential-count {
          padding: 4px 8px;
          background-color: #007bff;
          color: white;
          border-radius: 4px;
          font-size: 12px;
        }
        
        .credentials-list {
          display: grid;
          grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
          gap: 16px;
          padding: 16px;
          background-color: #f8f9fa;
          border-radius: 0 0 8px 8px;
        }
        
        .credential-card {
          background-color: white;
          border-radius: 8px;
          box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1);
          padding: 16px;
        }
        
        .card-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          margin-bottom: 12px;
        }
        
        .card-header h5 {
          margin: 0;
          font-size: 16px;
        }
        
        .trust-badge {
          padding: 4px 8px;
          border-radius: 4px;
          font-size: 12px;
          font-weight: 500;
        }
        
        .trust-badge.high {
          background-color: #d4edda;
          color: #155724;
        }
        
        .trust-badge.medium {
          background-color: #fff3cd;
          color: #856404;
        }
        
        .trust-badge.low {
          background-color: #f8d7da;
          color: #721c24;
        }
        
        .card-body {
          margin-bottom: 12px;
        }
        
        .card-body p {
          margin: 4px 0;
          font-size: 14px;
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
        
        .card-links {
          display: flex;
          justify-content: flex-end;
        }
        
        .thread-link {
          padding: 6px 12px;
          background-color: #e1f5fe;
          color: #0277bd;
          border-radius: 4px;
          text-decoration: none;
          font-size: 14px;
        }
        
        .thread-link:hover {
          background-color: #039be5;
          color: white;
        }
        
        @media (max-width: 768px) {
          .credentials-list {
            grid-template-columns: 1fr;
          }
        }
      `}</style>
    </div>
  );
}; 