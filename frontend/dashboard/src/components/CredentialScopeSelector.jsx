import React, { useState, useEffect } from 'react';
import { useCredentials } from '../contexts/CredentialContext';

export default function CredentialScopeSelector({ value, onChange }) {
  const { credentials, federationId } = useCredentials();
  const [scopes, setScopes] = useState([]);
  
  // Extract scopes from credentials
  useEffect(() => {
    // Start with "all" option
    const scopeOptions = [{ id: 'all', name: 'All Scopes' }];
    
    // Add current federation if available
    if (federationId) {
      const federationName = credentials.find(
        cred => cred.type === 'FederationMembership' && cred.metadata.federationId === federationId
      )?.metadata.federationName || 'Current Federation';
      
      scopeOptions.push({ 
        id: federationId, 
        name: federationName
      });
    }
    
    // Add other federations from credentials
    credentials.forEach(cred => {
      if (
        (cred.type === 'FederationMembership' || cred.type === 'FederationAdmin') &&
        cred.metadata.federationId && 
        cred.metadata.federationId !== federationId &&
        !scopeOptions.some(scope => scope.id === cred.metadata.federationId)
      ) {
        scopeOptions.push({
          id: cred.metadata.federationId,
          name: cred.metadata.federationName || `Federation ${cred.metadata.federationId.substring(0, 8)}...`,
        });
      }
    });
    
    setScopes(scopeOptions);
  }, [credentials, federationId]);

  return (
    <select
      className="mt-1 block w-full rounded-md border-gray-300 shadow-sm focus:border-agora-blue focus:ring-agora-blue sm:text-sm"
      value={value}
      onChange={(e) => onChange(e.target.value)}
    >
      {scopes.map((scope) => (
        <option key={scope.id} value={scope.id}>
          {scope.name}
        </option>
      ))}
    </select>
  );
} 