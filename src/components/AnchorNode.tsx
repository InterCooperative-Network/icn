import React from 'react';
import { AnchorCredential, isAnchorCredential } from '../../packages/credential-utils/types/AnchorCredential';
import { WalletCredential } from '../../packages/credential-utils/types';

// Styles for anchor node
const anchorNodeStyles = {
  node: 'rounded-xl shadow-md bg-gradient-to-br from-indigo-600 to-purple-700 text-white p-4 cursor-pointer hover:shadow-lg transition-all duration-200 border-2 border-indigo-300',
  title: 'font-semibold text-lg mb-1',
  subTitle: 'text-sm opacity-90',
  infoLine: 'text-xs flex items-center gap-2 mt-2',
  badge: 'bg-white/20 px-2 py-0.5 rounded-full text-xs',
};

export interface AnchorNodeProps {
  credential: WalletCredential;
  onClick?: (credential: WalletCredential) => void;
  selected?: boolean;
  compact?: boolean;
}

/**
 * Renders an anchor node with federation and epoch information
 */
export const AnchorNode: React.FC<AnchorNodeProps> = ({
  credential,
  onClick,
  selected = false,
  compact = false,
}) => {
  if (!isAnchorCredential(credential)) {
    return null;
  }

  // Extract federation name and epoch id
  const federationName = credential.metadata?.federation?.name || 'Unknown Federation';
  const epochId = credential.credentialSubject?.epochId || 
                  credential.credentialSubject?.epoch_id ||
                  'Unknown Epoch';
  const dagAnchor = credential.credentialSubject?.dagAnchor || 
                    credential.metadata?.dag?.root_hash || 
                    'Unknown Hash';
  
  // Format DAG anchor hash for display (first 6 characters)
  const shortDagAnchor = dagAnchor.substring(0, 6) + '...';
  
  // Format issuance date
  const issuanceDate = new Date(credential.issuanceDate).toLocaleDateString();
  
  // Get quorum status
  const hasQuorum = Boolean(
    credential.credentialSubject?.quorumInfo?.signers?.length >= 
    (credential.credentialSubject?.quorumInfo?.threshold || 0)
  );

  // Handle click event
  const handleClick = () => {
    if (onClick) {
      onClick(credential);
    }
  };

  return (
    <div 
      className={`${anchorNodeStyles.node} ${selected ? 'ring-2 ring-white' : ''}`}
      onClick={handleClick}
      data-testid="anchor-node"
    >
      {!compact && (
        <>
          <div className={anchorNodeStyles.title}>
            Epoch {epochId}
          </div>
          <div className={anchorNodeStyles.subTitle}>
            {federationName}
          </div>
          <div className={anchorNodeStyles.infoLine}>
            <span>DAG Root:</span>
            <span className={anchorNodeStyles.badge}>{shortDagAnchor}</span>
          </div>
          <div className={anchorNodeStyles.infoLine}>
            <span>Issued:</span>
            <span>{issuanceDate}</span>
          </div>
          <div className={anchorNodeStyles.infoLine}>
            <span>Quorum:</span>
            <span className={`${anchorNodeStyles.badge} ${hasQuorum ? 'bg-green-500/30' : 'bg-red-500/30'}`}>
              {hasQuorum ? 'Verified' : 'Incomplete'}
            </span>
          </div>
        </>
      )}
      
      {compact && (
        <div className="flex flex-col items-center justify-center p-1">
          <div className="text-sm font-medium">Epoch {epochId}</div>
          <div className="text-xs">{shortDagAnchor}</div>
        </div>
      )}
    </div>
  );
}; 