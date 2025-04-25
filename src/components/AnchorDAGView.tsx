import React, { useState, useEffect, useMemo } from 'react';
import { CredentialDAGView } from './CredentialDAGView';
import { AnchorNode } from './AnchorNode';
import { WalletCredential } from '../../packages/credential-utils/types';
import { isAnchorCredential } from '../../packages/credential-utils/types/AnchorCredential';
import { groupCredentialsByAnchor } from '../../packages/credential-utils/utils/groupByAnchor';
import { FederationManifest } from '../../packages/credential-utils/types/federation';

export interface AnchorDAGViewProps {
  credentials: WalletCredential[];
  selectedCredentialId?: string;
  onCredentialSelect?: (id: string) => void;
  federationManifests?: Record<string, FederationManifest>;
  width?: number;
  height?: number;
}

/**
 * Component for visualizing anchor credentials and their connected execution receipts
 */
export const AnchorDAGView: React.FC<AnchorDAGViewProps> = ({
  credentials,
  selectedCredentialId,
  onCredentialSelect,
  federationManifests = {},
  width = 800,
  height = 600,
}) => {
  const [selectedAnchor, setSelectedAnchor] = useState<string | null>(null);
  
  // Group credentials by anchor
  const anchorGroups = useMemo(() => {
    return groupCredentialsByAnchor(credentials);
  }, [credentials]);
  
  // Extract all anchor credentials
  const anchorCredentials = useMemo(() => {
    return credentials.filter(cred => isAnchorCredential(cred));
  }, [credentials]);

  // Handle selection of anchor
  const handleAnchorSelect = (credential: WalletCredential) => {
    setSelectedAnchor(credential.id);
    if (onCredentialSelect) {
      onCredentialSelect(credential.id);
    }
  };

  // Update selected anchor when selectedCredentialId changes externally
  useEffect(() => {
    if (selectedCredentialId && anchorCredentials.some(c => c.id === selectedCredentialId)) {
      setSelectedAnchor(selectedCredentialId);
    }
  }, [selectedCredentialId, anchorCredentials]);

  // Get relevant credentials for the current view
  const relevantCredentials = useMemo(() => {
    if (!selectedAnchor) {
      return credentials;
    }
    
    const selectedCred = credentials.find(c => c.id === selectedAnchor);
    if (!selectedCred || !isAnchorCredential(selectedCred)) {
      return credentials;
    }
    
    // Get the dag root hash
    const dagRoot = selectedCred.credentialSubject?.dag_root_hash || 
                    selectedCred.metadata?.dag?.root_hash;
    
    if (!dagRoot || !anchorGroups[dagRoot]) {
      return [selectedCred];
    }
    
    // Return the anchor and its receipts
    return [selectedCred, ...anchorGroups[dagRoot].receipts];
  }, [selectedAnchor, credentials, anchorGroups]);

  return (
    <div className="flex flex-col h-full">
      {/* Anchor selection bar */}
      <div className="flex overflow-x-auto gap-3 p-3 bg-gray-100 dark:bg-gray-800 rounded-lg mb-4">
        {anchorCredentials.length === 0 ? (
          <div className="text-gray-500 italic p-2">No anchor credentials found</div>
        ) : (
          anchorCredentials.map(cred => (
            <div key={cred.id} className="flex-shrink-0 w-48">
              <AnchorNode 
                credential={cred}
                selected={selectedAnchor === cred.id}
                onClick={handleAnchorSelect}
              />
            </div>
          ))
        )}
      </div>
      
      {/* DAG visualization */}
      <div className="flex-grow">
        <CredentialDAGView 
          credentials={relevantCredentials}
          selectedCredentialId={selectedCredentialId}
          onCredentialSelect={onCredentialSelect}
          federationManifests={federationManifests}
          showAnchorNodes={true}
          width={width}
          height={height - 120} // Adjust for the anchor bar
        />
      </div>
    </div>
  );
}; 