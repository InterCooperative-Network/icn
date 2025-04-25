import React, { useState, useEffect } from 'react';
import { FederationSearchPage } from '../components/FederationSearchPage';
import { CredentialService } from '../services/credential-service';
import { SearchService } from '../services/search-service';
import { WalletCredential } from '../../packages/credential-utils/types';

/**
 * Demo component for the Federation Search UI
 */
export const FederationSearchDemo: React.FC = () => {
  const [credentials, setCredentials] = useState<WalletCredential[]>([]);
  const [loading, setLoading] = useState<boolean>(true);
  const [error, setError] = useState<string | null>(null);
  
  // Config values - in a real application, these would come from app config
  const agoraNetEndpoint = "https://agoranet.icn.zone";
  const userDid = "did:icn:user:123456";
  
  // Initialize services
  const credentialService = new CredentialService();
  
  // Load credentials on component mount
  useEffect(() => {
    const loadCredentials = async () => {
      setLoading(true);
      try {
        await credentialService.syncCredentials();
        const allCredentials = credentialService.getCredentials();
        setCredentials(allCredentials);
      } catch (err) {
        console.error('Error loading credentials:', err);
        setError('Failed to load credentials. Please try again.');
      } finally {
        setLoading(false);
      }
    };
    
    loadCredentials();
  }, []);
  
  if (loading) {
    return <div className="loading">Loading credentials...</div>;
  }
  
  if (error) {
    return <div className="error">{error}</div>;
  }
  
  return (
    <div className="federation-search-demo">
      <h1>Federation Search Demo</h1>
      
      <FederationSearchPage 
        credentials={credentials}
        agoraNetEndpoint={agoraNetEndpoint}
        userDid={userDid}
      />
      
      <style jsx>{`
        .federation-search-demo {
          max-width: 1200px;
          margin: 0 auto;
          padding: 20px;
        }
        
        h1 {
          margin-bottom: 30px;
          font-size: 28px;
          font-weight: 600;
          color: #333;
          border-bottom: 2px solid #e9ecef;
          padding-bottom: 10px;
        }
        
        .loading,
        .error {
          padding: 40px;
          text-align: center;
          background-color: #f8f9fa;
          border-radius: 8px;
          margin-top: 30px;
        }
        
        .error {
          color: #721c24;
          background-color: #f8d7da;
        }
      `}</style>
    </div>
  );
}; 