import React, { useState } from 'react';
import { WalletCredential } from '../../packages/credential-utils/types';
import { extractDagAnchorHash } from '../../packages/credential-utils/utils/groupByAnchor';

interface AnchorImportPromptProps {
  credential: WalletCredential;
  onImportAnchor: (dagAnchor: string) => void;
  onCancel: () => void;
}

/**
 * Component that prompts users to import an anchor credential
 * when viewing a receipt that references a DAG anchor
 */
export const AnchorImportPrompt: React.FC<AnchorImportPromptProps> = ({
  credential,
  onImportAnchor,
  onCancel,
}) => {
  const [isExpanded, setIsExpanded] = useState(false);
  
  // Extract the DAG anchor hash from the credential
  const dagAnchorHash = extractDagAnchorHash(credential);
  
  if (!dagAnchorHash) {
    return null;
  }
  
  const truncatedHash = dagAnchorHash.length > 10 
    ? `${dagAnchorHash.substring(0, 6)}...${dagAnchorHash.substring(dagAnchorHash.length - 4)}`
    : dagAnchorHash;
  
  return (
    <div className="rounded-lg bg-amber-50 dark:bg-amber-900/30 p-4 mb-4 border border-amber-200 dark:border-amber-700">
      <div className="flex items-start">
        <div className="flex-shrink-0 mt-0.5">
          <svg className="h-5 w-5 text-amber-500" viewBox="0 0 20 20" fill="currentColor">
            <path fillRule="evenodd" d="M8.257 3.099c.765-1.36 2.722-1.36 3.486 0l5.58 9.92c.75 1.334-.213 2.98-1.742 2.98H4.42c-1.53 0-2.493-1.646-1.743-2.98l5.58-9.92zM11 13a1 1 0 11-2 0 1 1 0 012 0zm-1-8a1 1 0 00-1 1v3a1 1 0 002 0V6a1 1 0 00-1-1z" clipRule="evenodd" />
          </svg>
        </div>
        <div className="ml-3 flex-1">
          <h3 className="text-sm font-medium text-amber-800 dark:text-amber-200">
            AnchorCredential not found – import now?
          </h3>
          <div className="mt-2 text-sm text-amber-700 dark:text-amber-300">
            <p>
              This credential references DAG anchor <span className="font-mono">{truncatedHash}</span> but 
              the corresponding anchor credential is not in your wallet.
            </p>
            
            {isExpanded && (
              <div className="mt-3">
                <p className="mb-2">
                  Anchor credentials provide important federation context and verification 
                  for receipts, including epoch information and quorum signatures.
                </p>
                
                <p>
                  Importing the anchor will enable enhanced visualization and trust verification.
                </p>
              </div>
            )}
          </div>
          
          <div className="mt-3 flex items-center">
            <button
              type="button"
              className="inline-flex items-center px-4 py-2 border border-transparent text-sm font-medium rounded-md shadow-sm text-white bg-indigo-600 hover:bg-indigo-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-indigo-500"
              onClick={() => onImportAnchor(dagAnchorHash)}
            >
              Import Anchor
            </button>
            <button
              type="button"
              className="ml-3 inline-flex items-center px-4 py-2 border border-gray-300 shadow-sm text-sm font-medium rounded-md text-gray-700 dark:text-gray-300 bg-white dark:bg-gray-800 hover:bg-gray-50 dark:hover:bg-gray-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-indigo-500"
              onClick={onCancel}
            >
              Not Now
            </button>
            <button 
              className="ml-4 text-amber-600 dark:text-amber-400 text-sm underline"
              onClick={() => setIsExpanded(!isExpanded)}
            >
              {isExpanded ? 'Show Less' : 'Learn More'}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}; 