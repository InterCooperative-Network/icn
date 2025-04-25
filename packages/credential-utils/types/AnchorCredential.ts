import { WalletCredential } from './credentials';

/**
 * Represents a Merkle-anchored epoch credential issued by a federation
 */
export interface AnchorCredential extends WalletCredential {
  type: string[] | 'AnchorCredential' | 'EpochAnchorCredential';
  
  credentialSubject: {
    id: string;
    epochId?: string;
    federationId?: string;
    dagAnchor: string; // Merkle root hash
    issuanceDate: string;
    quorumInfo?: {
      threshold: number;
      signers: string[];
    };
    [key: string]: any;
  };
  
  metadata?: {
    federation?: {
      id: string;
      name?: string;
    };
    dag?: {
      root_hash: string;
      timestamp: string;
      blockHeight?: number;
    };
    [key: string]: any;
  };
}

/**
 * Type guard to check if a credential is an AnchorCredential
 * @param credential The credential to check
 * @returns true if the credential is an AnchorCredential
 */
export function isAnchorCredential(credential: WalletCredential): credential is AnchorCredential {
  // Check type property - both array or string forms
  const typeCheck = 
    (Array.isArray(credential.type) && (
      credential.type.includes('AnchorCredential') || 
      credential.type.includes('EpochAnchorCredential')
    )) ||
    credential.type === 'AnchorCredential' ||
    credential.type === 'EpochAnchorCredential';
  
  // Check for DAG anchor properties
  const hasAnchorProps = 
    credential.credentialSubject?.dagAnchor !== undefined ||
    credential.metadata?.dag?.root_hash !== undefined;
  
  return typeCheck && hasAnchorProps;
}

/**
 * Type guard to check if a credential is anchored to a specific DAG
 * @param credential The credential to check
 * @param dagAnchorHash The DAG hash to check against
 * @returns true if the credential is anchored to the specified DAG
 */
export function isAnchoredToDAG(
  credential: WalletCredential, 
  dagAnchorHash: string
): boolean {
  const credDagAnchor = 
    credential.credentialSubject?.dagAnchor ||
    credential.metadata?.dag?.root_hash;
  
  return credDagAnchor === dagAnchorHash;
} 