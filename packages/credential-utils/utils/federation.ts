import { WalletCredential } from '../types/wallet';

/**
 * Groups credentials by federation ID
 * @param credentials Array of wallet credentials
 * @returns Object with federation IDs as keys and arrays of credentials as values
 */
export function groupCredentialsByFederation(
  credentials: WalletCredential[]
): Record<string, WalletCredential[]> {
  const result: Record<string, WalletCredential[]> = {};
  
  credentials.forEach(credential => {
    // Try to get federation ID from metadata
    let federationId = 'unfederated';
    
    if (credential.metadata?.federation?.id) {
      federationId = credential.metadata.federation.id;
    } else if (credential.metadata?.agoranet?.federation_id) {
      federationId = credential.metadata.agoranet.federation_id;
    }
    
    // Initialize array if it doesn't exist
    if (!result[federationId]) {
      result[federationId] = [];
    }
    
    // Add credential to the appropriate federation group
    result[federationId].push(credential);
  });
  
  return result;
}

/**
 * Filters credentials by federation ID
 * @param credentials Array of wallet credentials
 * @param federationId Federation ID to filter by
 * @returns Array of credentials belonging to the specified federation
 */
export function filterCredentialsByFederation(
  credentials: WalletCredential[],
  federationId: string
): WalletCredential[] {
  return credentials.filter(credential => {
    // Get federation ID from metadata
    const credentialFederationId = 
      credential.metadata?.federation?.id || 
      credential.metadata?.agoranet?.federation_id || 
      'unfederated';
    
    return credentialFederationId === federationId;
  });
} 