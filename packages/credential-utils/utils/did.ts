/**
 * Formats a DID for display by truncating the middle part
 * @param did The DID string to format
 * @returns Formatted DID string with middle part truncated
 */
export const formatDid = (did: string): string => {
  if (!did || did.length < 15) return did;
  const start = did.substring(0, 8);
  const end = did.substring(did.length - 8);
  return `${start}...${end}`;
};

/**
 * Extracts the scope from a DID
 * @param did The DID string (e.g. did:icn:scope:username)
 * @returns The scope part of the DID or undefined if not found
 */
export const extractDIDScope = (did: string): string | undefined => {
  if (!did || !did.startsWith('did:icn:')) return undefined;
  
  const parts = did.split(':');
  if (parts.length >= 3) {
    return parts[2];
  }
  
  return undefined;
};

/**
 * Extracts the username from a DID
 * @param did The DID string (e.g. did:icn:scope:username)
 * @returns The username part of the DID or undefined if not found
 */
export const extractDIDUsername = (did: string): string | undefined => {
  if (!did || !did.startsWith('did:icn:')) return undefined;
  
  const parts = did.split(':');
  if (parts.length >= 4) {
    return parts[3];
  }
  
  return undefined;
};

/**
 * Creates a DID from scope and username
 * @param scope The scope part of the DID
 * @param username The username part of the DID
 * @returns Formatted DID string
 */
export const createDid = (scope: string, username: string): string => {
  return `did:icn:${scope}:${username}`;
};

/**
 * Validates if a string is a valid ICN DID
 * @param did The DID string to validate
 * @returns True if the DID is valid, false otherwise
 */
export const isValidDid = (did: string): boolean => {
  if (!did || typeof did !== 'string') return false;
  return /^did:icn:[a-zA-Z0-9_-]+:[a-zA-Z0-9_-]+$/.test(did);
}; 