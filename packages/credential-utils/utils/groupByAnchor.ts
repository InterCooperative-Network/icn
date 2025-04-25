import { WalletCredential } from '../types/credentials';
import { findAnchorCredentials } from './anchorCredential';

/**
 * Groups execution receipts by their dagAnchor hash and maps them to their parent anchor credentials
 * @param credentials All credentials in the wallet
 * @returns A map of dagAnchor hashes to {anchor, receipts} objects
 */
export function groupCredentialsByAnchor(credentials: WalletCredential[]): Record<string, {
  anchor: WalletCredential | null;
  receipts: WalletCredential[];
}> {
  // Get all anchor credentials
  const anchors = findAnchorCredentials(credentials);
  
  // Create a map of dag root hashes to anchor credentials
  const dagRootToAnchor: Record<string, WalletCredential> = {};
  for (const anchor of anchors) {
    const dagRoot = 
      anchor.credentialSubject?.dag_root_hash || 
      anchor.credentialSubject?.dagRoot ||
      anchor.metadata?.dag?.root_hash;
    
    if (dagRoot) {
      dagRootToAnchor[dagRoot] = anchor;
    }
  }
  
  // Group execution receipts by their dagAnchor hash
  const result: Record<string, {
    anchor: WalletCredential | null;
    receipts: WalletCredential[];
  }> = {};
  
  for (const cred of credentials) {
    // Skip anchor credentials (they're already processed)
    if (Array.isArray(cred.type) && (
      cred.type.includes('AnchorCredential') || 
      cred.type.includes('EpochAnchorCredential')
    )) {
      continue;
    }
    
    // Check if this credential has a dagAnchor reference
    const dagAnchor = 
      cred.credentialSubject?.dagAnchor ||
      cred.metadata?.dag?.root_hash;
    
    if (dagAnchor) {
      if (!result[dagAnchor]) {
        result[dagAnchor] = {
          anchor: dagRootToAnchor[dagAnchor] || null,
          receipts: []
        };
      }
      
      result[dagAnchor].receipts.push(cred);
    }
  }
  
  return result;
}

/**
 * Checks if a credential has a valid reference to a DAG anchor
 * @param credential The credential to check
 * @returns true if the credential is anchored to a DAG
 */
export function isAnchoredCredential(credential: WalletCredential): boolean {
  return Boolean(
    credential.credentialSubject?.dagAnchor ||
    credential.metadata?.dag?.root_hash
  );
}

/**
 * Extracts the DAG anchor hash from a credential
 * @param credential The credential to extract from
 * @returns The DAG anchor hash or null if not found
 */
export function extractDagAnchorHash(credential: WalletCredential): string | null {
  return credential.credentialSubject?.dagAnchor ||
         credential.metadata?.dag?.root_hash ||
         null;
} 