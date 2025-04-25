import React, { useState, useEffect } from 'react';
import { WalletCredential } from '../../packages/credential-utils/types';
import { filterCredentialsByFederation, groupCredentialsByFederation } from '../../packages/credential-utils/utils/federation';

interface CredentialSearchBarProps {
  credentials: WalletCredential[];
  federations: { id: string; name: string }[];
  onSearchResults: (results: WalletCredential[], searchQuery: string, federationFilter: string) => void;
}

/**
 * A search component for filtering credentials by various criteria
 */
export const CredentialSearchBar: React.FC<CredentialSearchBarProps> = ({
  credentials,
  federations,
  onSearchResults
}) => {
  const [searchQuery, setSearchQuery] = useState<string>('');
  const [selectedType, setSelectedType] = useState<string>('all');
  const [selectedFederation, setSelectedFederation] = useState<string>('all');
  const [selectedRole, setSelectedRole] = useState<string>('all');
  
  // Get unique credential types
  const credentialTypes = ['all', ...new Set(credentials.map(cred => cred.type))];
  
  // Get unique roles from credentials
  const extractRoles = (creds: WalletCredential[]) => {
    const roles = new Set<string>();
    
    creds.forEach(cred => {
      if (cred.credentialSubject.role) {
        roles.add(cred.credentialSubject.role);
      }
    });
    
    return ['all', ...Array.from(roles)];
  };
  
  const roles = extractRoles(credentials);
  
  // Perform search when any search criteria changes
  useEffect(() => {
    performSearch();
  }, [searchQuery, selectedType, selectedFederation, selectedRole]);
  
  const performSearch = () => {
    // 1. First filter by federation if selected
    let filteredCredentials = selectedFederation === 'all' 
      ? credentials 
      : filterCredentialsByFederation(credentials, selectedFederation);
    
    // 2. Filter by credential type if selected
    if (selectedType !== 'all') {
      filteredCredentials = filteredCredentials.filter(cred => 
        cred.type === selectedType
      );
    }
    
    // 3. Filter by role if selected
    if (selectedRole !== 'all') {
      filteredCredentials = filteredCredentials.filter(cred => 
        cred.credentialSubject.role === selectedRole
      );
    }
    
    // 4. Filter by search text if provided (check in multiple fields)
    if (searchQuery.trim()) {
      const query = searchQuery.toLowerCase().trim();
      
      filteredCredentials = filteredCredentials.filter(cred => {
        // Search in title
        if (cred.title.toLowerCase().includes(query)) return true;
        
        // Search in credential subject fields
        const subjectStr = JSON.stringify(cred.credentialSubject).toLowerCase();
        if (subjectStr.includes(query)) return true;
        
        // Search in proposal ID if any
        if (cred.credentialSubject.proposalId?.toLowerCase().includes(query)) return true;
        
        // Search in tags if any
        if (cred.tags?.some(tag => tag.toLowerCase().includes(query))) return true;
        
        // Search in issuer name/DID
        if (cred.issuer.name?.toLowerCase().includes(query) || 
            cred.issuer.did.toLowerCase().includes(query)) return true;
        
        return false;
      });
    }
    
    // Send results back to parent
    onSearchResults(filteredCredentials, searchQuery, selectedFederation);
  };
  
  return (
    <div className="credential-search-container">
      <div className="search-bar">
        <input
          type="text"
          placeholder="Search credentials..."
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
            <option value="unfederated">Unfederated</option>
          </select>
        </div>
        
        <div className="filter-group">
          <label>Type:</label>
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
        
        <div className="filter-group">
          <label>Role:</label>
          <select 
            value={selectedRole} 
            onChange={(e) => setSelectedRole(e.target.value)}
          >
            {roles.map(role => (
              <option key={role} value={role}>
                {role === 'all' ? 'All Roles' : role}
              </option>
            ))}
          </select>
        </div>
      </div>
      
      <style jsx>{`
        .credential-search-container {
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
        }
        
        .filter-group {
          display: flex;
          align-items: center;
          gap: 8px;
        }
        
        select {
          padding: 8px 12px;
          border: 1px solid #ced4da;
          border-radius: 4px;
          background-color: white;
        }
        
        @media (max-width: 768px) {
          .filter-controls {
            flex-direction: column;
            gap: 12px;
          }
          
          .filter-group {
            width: 100%;
          }
          
          select {
            flex-grow: 1;
          }
        }
      `}</style>
    </div>
  );
}; 