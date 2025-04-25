import React, { useState, useEffect } from 'react';
import { WalletCredential } from '../../packages/credential-utils/types';
import { CredentialService } from '../services/credential-service';

interface CredentialListViewProps {
  credentialService: CredentialService;
  userDid: string;
}

/**
 * Component to display and manage credentials in the wallet
 */
export const CredentialListView: React.FC<CredentialListViewProps> = ({ 
  credentialService, 
  userDid 
}) => {
  const [credentials, setCredentials] = useState<WalletCredential[]>([]);
  const [loading, setLoading] = useState<boolean>(true);
  const [selectedType, setSelectedType] = useState<string>('all');
  const [isVerifying, setIsVerifying] = useState<boolean>(false);
  const [selectedCredential, setSelectedCredential] = useState<string | null>(null);
  
  // Load credentials on component mount
  useEffect(() => {
    loadCredentials();
    // Start auto-sync
    credentialService.startSync();
    
    // Clean up on unmount
    return () => {
      credentialService.stopSync();
    };
  }, []);
  
  // Load credentials from service
  const loadCredentials = async () => {
    setLoading(true);
    try {
      await credentialService.syncCredentials();
      const allCredentials = credentialService.getCredentials();
      setCredentials(allCredentials);
    } catch (error) {
      console.error('Failed to load credentials:', error);
    } finally {
      setLoading(false);
    }
  };
  
  // Filter credentials by type
  const filteredCredentials = selectedType === 'all' 
    ? credentials 
    : credentials.filter(cred => cred.type === selectedType);
  
  // Get unique credential types
  const credentialTypes = ['all', ...new Set(credentials.map(cred => cred.type))];
  
  // Verify a credential
  const verifyCredential = async (id: string) => {
    setIsVerifying(true);
    setSelectedCredential(id);
    try {
      const result = await credentialService.verifyCredential(id);
      alert(result.message);
    } catch (error) {
      console.error('Verification failed:', error);
      alert('Verification failed: ' + (error instanceof Error ? error.message : 'Unknown error'));
    } finally {
      setIsVerifying(false);
      setSelectedCredential(null);
    }
  };
  
  // Export a credential
  const exportCredential = (id: string) => {
    try {
      credentialService.exportCredential(id);
    } catch (error) {
      console.error('Export failed:', error);
      alert('Export failed: ' + (error instanceof Error ? error.message : 'Unknown error'));
    }
  };
  
  // Export multiple credentials as a presentation
  const exportSelectedAsPresentation = (ids: string[]) => {
    try {
      const presentation = credentialService.exportPresentation(ids, userDid);
      
      // Create and download a JSON file
      const blob = new Blob([presentation], { type: 'application/json' });
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = 'governance-credentials.vp.json';
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      URL.revokeObjectURL(url);
    } catch (error) {
      console.error('Export failed:', error);
      alert('Export failed: ' + (error instanceof Error ? error.message : 'Unknown error'));
    }
  };
  
  return (
    <div className="credential-list-container">
      <div className="header">
        <h2>My Governance History</h2>
        <div className="actions">
          <button 
            className="sync-button" 
            onClick={loadCredentials} 
            disabled={loading}
          >
            {loading ? 'Syncing...' : 'Sync Credentials'}
          </button>
        </div>
      </div>
      
      <div className="filter-bar">
        <div className="type-filter">
          <label>Filter by type:</label>
          <select 
            value={selectedType} 
            onChange={(e) => setSelectedType(e.target.value)}
          >
            {credentialTypes.map(type => (
              <option key={type} value={type}>
                {type === 'all' ? 'All Types' : type}
              </option>
            ))}
          </select>
        </div>
      </div>
      
      {loading ? (
        <div className="loading">Loading credentials...</div>
      ) : filteredCredentials.length === 0 ? (
        <div className="empty-state">
          <p>No credentials found. Governance credentials will appear here after you participate in federation governance.</p>
        </div>
      ) : (
        <div className="credential-grid">
          {filteredCredentials.map((credential) => (
            <div 
              key={credential.id} 
              className={`credential-card ${credential.trustLevel?.toLowerCase()}`}
            >
              <div className="card-header">
                <h3>{credential.title}</h3>
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
              
              <div className="card-actions">
                <button 
                  className="verify-btn"
                  onClick={() => verifyCredential(credential.id)}
                  disabled={isVerifying && selectedCredential === credential.id}
                >
                  {isVerifying && selectedCredential === credential.id ? 'Verifying...' : 'Verify'}
                </button>
                <button 
                  className="export-btn"
                  onClick={() => exportCredential(credential.id)}
                >
                  Export
                </button>
                {credential.metadata?.agoranet?.threadUrl && (
                  <a 
                    href={credential.metadata.agoranet.threadUrl}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="view-thread-btn"
                  >
                    View Discussion
                  </a>
                )}
              </div>
            </div>
          ))}
        </div>
      )}
      
      <div className="bulk-actions">
        <button 
          onClick={() => exportSelectedAsPresentation(filteredCredentials.map(c => c.id))}
          disabled={filteredCredentials.length === 0}
        >
          Export All as Presentation
        </button>
      </div>
      
      <style jsx>{`
        .credential-list-container {
          max-width: 1200px;
          margin: 0 auto;
          padding: 20px;
        }
        
        .header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          margin-bottom: 20px;
        }
        
        .filter-bar {
          display: flex;
          margin-bottom: 20px;
        }
        
        .type-filter {
          display: flex;
          align-items: center;
          gap: 10px;
        }
        
        .credential-grid {
          display: grid;
          grid-template-columns: repeat(auto-fill, minmax(300px, 1fr));
          gap: 20px;
        }
        
        .credential-card {
          border-radius: 8px;
          box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1);
          padding: 16px;
          background-color: #fff;
          border-left: 4px solid #ccc;
        }
        
        .credential-card.high {
          border-left-color: #4caf50;
        }
        
        .credential-card.medium {
          border-left-color: #ff9800;
        }
        
        .credential-card.low {
          border-left-color: #f44336;
        }
        
        .card-header {
          display: flex;
          justify-content: space-between;
          align-items: center;
          margin-bottom: 10px;
        }
        
        .card-header h3 {
          margin: 0;
          font-size: 16px;
          font-weight: 600;
        }
        
        .trust-badge {
          padding: 4px 8px;
          border-radius: 4px;
          font-size: 12px;
          font-weight: 500;
        }
        
        .trust-badge.high {
          background-color: #e8f5e9;
          color: #2e7d32;
        }
        
        .trust-badge.medium {
          background-color: #fff3e0;
          color: #ef6c00;
        }
        
        .trust-badge.low {
          background-color: #ffebee;
          color: #c62828;
        }
        
        .card-body {
          margin-bottom: 16px;
        }
        
        .card-body p {
          margin: 4px 0;
          font-size: 14px;
        }
        
        .tags {
          display: flex;
          flex-wrap: wrap;
          gap: 6px;
          margin-top: 10px;
        }
        
        .tag {
          background-color: #eceff1;
          color: #455a64;
          padding: 2px 8px;
          border-radius: 4px;
          font-size: 12px;
        }
        
        .card-actions {
          display: flex;
          justify-content: space-between;
        }
        
        button {
          padding: 8px 16px;
          border-radius: 4px;
          font-weight: 500;
          cursor: pointer;
          border: none;
        }
        
        .verify-btn {
          background-color: #e3f2fd;
          color: #1565c0;
        }
        
        .export-btn {
          background-color: #f5f5f5;
          color: #424242;
        }
        
        .view-thread-btn {
          background-color: #e1f5fe;
          color: #0277bd;
          padding: 8px 16px;
          border-radius: 4px;
          font-weight: 500;
          cursor: pointer;
          border: none;
          text-decoration: none;
          display: inline-block;
          margin-left: auto;
        }
        
        .bulk-actions {
          margin-top: 20px;
          display: flex;
          justify-content: flex-end;
        }
        
        .empty-state {
          text-align: center;
          padding: 40px;
          background-color: #f5f5f5;
          border-radius: 8px;
        }
        
        .loading {
          text-align: center;
          padding: 20px;
        }
      `}</style>
    </div>
  );
}; 