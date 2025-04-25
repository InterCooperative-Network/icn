import { WalletCredential } from '../types';

/**
 * Search options for finding credentials
 */
export interface CredentialSearchOptions {
  /** Full text search query across all fields */
  query?: string;
  
  /** Filter by credential type */
  type?: string | string[];
  
  /** Filter by federation ID */
  federationId?: string;
  
  /** Filter by proposal ID */
  proposalId?: string;
  
  /** Filter by thread ID */
  threadId?: string;
  
  /** Filter by issuance date range */
  dateRange?: {
    from?: Date;
    to?: Date;
  };
  
  /** Filter by issuer DID */
  issuerDid?: string;
  
  /** Filter by specific tag */
  tag?: string;
  
  /** Only include credentials with valid proofs */
  onlyVerified?: boolean;
  
  /** Custom filter function for complex criteria */
  customFilter?: (credential: WalletCredential) => boolean;
}

/**
 * Search credentials with flexible criteria
 * @param credentials Array of credentials to search
 * @param options Search options
 * @returns Filtered array of credentials matching the search criteria
 */
export function searchCredentials(
  credentials: WalletCredential[],
  options: CredentialSearchOptions
): WalletCredential[] {
  if (!credentials || credentials.length === 0) {
    return [];
  }
  
  return credentials.filter(credential => {
    // Check all filter criteria
    
    // Full text search
    if (options.query && !matchesFullTextSearch(credential, options.query)) {
      return false;
    }
    
    // Type filter
    if (options.type) {
      const types = Array.isArray(options.type) ? options.type : [options.type];
      if (!types.includes(credential.type)) {
        return false;
      }
    }
    
    // Federation ID filter
    if (options.federationId) {
      const credentialFederationId = 
        credential.metadata?.federation?.id || 
        credential.metadata?.agoranet?.federation_id;
      
      if (credentialFederationId !== options.federationId) {
        return false;
      }
    }
    
    // Proposal ID filter
    if (options.proposalId && credential.credentialSubject.proposalId !== options.proposalId) {
      return false;
    }
    
    // Thread ID filter
    if (options.threadId && credential.metadata?.agoranet?.threadId !== options.threadId) {
      return false;
    }
    
    // Date range filter
    if (options.dateRange) {
      const issuanceDate = new Date(credential.issuanceDate);
      
      if (options.dateRange.from && issuanceDate < options.dateRange.from) {
        return false;
      }
      
      if (options.dateRange.to && issuanceDate > options.dateRange.to) {
        return false;
      }
    }
    
    // Issuer DID filter
    if (options.issuerDid && credential.issuer.did !== options.issuerDid) {
      return false;
    }
    
    // Tag filter
    if (options.tag && (!credential.tags || !credential.tags.includes(options.tag))) {
      return false;
    }
    
    // Verified filter
    if (options.onlyVerified && (!credential.proof || credential.trustLevel === 'Low')) {
      return false;
    }
    
    // Custom filter
    if (options.customFilter && !options.customFilter(credential)) {
      return false;
    }
    
    // All filters passed
    return true;
  });
}

/**
 * Check if a credential matches a full-text search query
 * @param credential The credential to check
 * @param query The search query
 * @returns True if the credential matches the query
 */
function matchesFullTextSearch(credential: WalletCredential, query: string): boolean {
  if (!query) return true;
  
  // Normalize query for case-insensitive search
  const normalizedQuery = query.toLowerCase();
  
  // Check various fields for matches
  
  // Check title
  if (credential.title.toLowerCase().includes(normalizedQuery)) {
    return true;
  }
  
  // Check type
  if (credential.type.toLowerCase().includes(normalizedQuery)) {
    return true;
  }
  
  // Check issuer
  if (credential.issuer.did.toLowerCase().includes(normalizedQuery) ||
      (credential.issuer.name && credential.issuer.name.toLowerCase().includes(normalizedQuery))) {
    return true;
  }
  
  // Check tags
  if (credential.tags && 
      credential.tags.some(tag => tag.toLowerCase().includes(normalizedQuery))) {
    return true;
  }
  
  // Check federation metadata
  if (credential.metadata?.federation?.name && 
      credential.metadata.federation.name.toLowerCase().includes(normalizedQuery)) {
    return true;
  }
  
  // Check credential subject fields (recursive)
  return searchInObject(credential.credentialSubject, normalizedQuery);
}

/**
 * Recursively search for text in an object
 * @param obj The object to search in
 * @param query The search query
 * @returns True if the query is found in the object
 */
function searchInObject(obj: any, query: string): boolean {
  if (!obj || typeof obj !== 'object') {
    return false;
  }
  
  for (const key in obj) {
    const value = obj[key];
    
    // Check the key name
    if (key.toLowerCase().includes(query)) {
      return true;
    }
    
    // Check the value based on type
    if (typeof value === 'string' && value.toLowerCase().includes(query)) {
      return true;
    } else if (typeof value === 'number' && value.toString().includes(query)) {
      return true;
    } else if (typeof value === 'boolean' && value.toString().toLowerCase().includes(query)) {
      return true;
    } else if (value && typeof value === 'object') {
      // Recursively search in nested objects
      if (searchInObject(value, query)) {
        return true;
      }
    }
  }
  
  return false;
} 