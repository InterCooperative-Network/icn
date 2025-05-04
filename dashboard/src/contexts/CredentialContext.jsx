import React, { createContext, useContext, useState, useEffect } from 'react';
import { verifyCredential } from '../utils/credentialVerifier';
import { db } from '../utils/db';

const CredentialContext = createContext();

export function useCredentials() {
  return useContext(CredentialContext);
}

export function CredentialProvider({ children }) {
  const [credentials, setCredentials] = useState([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState(null);
  const [federationId, setFederationId] = useState(null);
  const [userDid, setUserDid] = useState(null);

  // Load credentials from local storage or IndexedDB
  useEffect(() => {
    async function loadCredentials() {
      try {
        // Load from IndexedDB
        const storedCredentials = await db.credentials.toArray();
        setCredentials(storedCredentials);
        
        // Set user DID from the first credential
        if (storedCredentials.length > 0) {
          setUserDid(storedCredentials[0].subject);
          
          // Find federation scope if available
          const federationCred = storedCredentials.find(
            cred => cred.type === 'FederationMembership'
          );
          
          if (federationCred) {
            setFederationId(federationCred.metadata.federationId);
          }
        }
      } catch (err) {
        console.error('Failed to load credentials:', err);
        setError(err);
      } finally {
        setLoading(false);
      }
    }
    
    loadCredentials();
  }, []);

  // Add a new credential and verify it
  const addCredential = async (credentialJwt) => {
    try {
      const verifiedCred = await verifyCredential(credentialJwt);
      
      if (verifiedCred) {
        await db.credentials.add({
          id: verifiedCred.jti || Date.now().toString(),
          jwt: credentialJwt,
          subject: verifiedCred.sub,
          issuer: verifiedCred.iss,
          type: verifiedCred.vc.type[1] || verifiedCred.vc.type[0],
          issuanceDate: verifiedCred.vc.issuanceDate,
          expirationDate: verifiedCred.exp ? new Date(verifiedCred.exp * 1000).toISOString() : null,
          metadata: verifiedCred.vc.credentialSubject
        });
        
        // Reload credentials
        const updatedCreds = await db.credentials.toArray();
        setCredentials(updatedCreds);
        
        // Update user DID if not set
        if (!userDid) {
          setUserDid(verifiedCred.sub);
        }
        
        return true;
      }
      return false;
    } catch (err) {
      console.error('Failed to add credential:', err);
      setError(err);
      return false;
    }
  };

  // Check if user has a specific credential type
  const hasCredential = (type) => {
    return credentials.some(cred => cred.type === type);
  };

  // Check permission based on credential scope
  const hasPermission = (action, scope) => {
    if (!userDid) return false;
    
    // Find credentials that grant this permission
    return credentials.some(cred => {
      // Check for action-specific credentials
      if (cred.type === 'ActionPermission' && 
          cred.metadata.actions && 
          cred.metadata.actions.includes(action)) {
        // If scope is provided, check if credential covers this scope
        if (scope) {
          return cred.metadata.scope === scope;
        }
        return true;
      }
      
      // Federation admins have all permissions within their federation
      if (cred.type === 'FederationAdmin' && 
          (!scope || cred.metadata.federationId === scope)) {
        return true;
      }
      
      return false;
    });
  };

  // Clear all credentials (logout)
  const clearCredentials = async () => {
    await db.credentials.clear();
    setCredentials([]);
    setUserDid(null);
    setFederationId(null);
  };

  const value = {
    credentials,
    loading,
    error,
    isAuthenticated: !!userDid,
    userDid,
    federationId,
    addCredential,
    hasCredential,
    hasPermission,
    clearCredentials
  };

  return (
    <CredentialContext.Provider value={value}>
      {children}
    </CredentialContext.Provider>
  );
} 