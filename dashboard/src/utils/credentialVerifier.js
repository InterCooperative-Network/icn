import { verifyJWT } from 'did-jwt';
import { getResolver } from './didResolver';

/**
 * Verifies a JWT credential
 * @param {string} credentialJwt - The JWT credential to verify
 * @returns {Promise<Object|null>} The verified credential payload or null if invalid
 */
export async function verifyCredential(credentialJwt) {
  try {
    const resolver = getResolver();
    
    // Verify the JWT
    const { payload, issuer } = await verifyJWT(credentialJwt, { resolver });
    
    // Basic credential validation
    if (!payload.vc || !payload.vc.type || !payload.vc.credentialSubject) {
      console.error('Invalid credential format:', payload);
      return null;
    }
    
    // Check if credential has expired
    if (payload.exp && payload.exp * 1000 < Date.now()) {
      console.error('Credential has expired');
      return null;
    }
    
    // If valid, return the payload
    return payload;
  } catch (error) {
    console.error('Error verifying credential:', error);
    return null;
  }
}

/**
 * Verifies an ExecutionReceipt credential
 * @param {Object} credential - The credential to verify
 * @returns {boolean} Whether the credential is valid
 */
export function verifyExecutionReceipt(credential) {
  // Ensure it's an ExecutionReceipt type
  if (!credential.vc.type.includes('ExecutionReceipt')) {
    return false;
  }
  
  const subject = credential.vc.credentialSubject;
  
  // Required fields for an ExecutionReceipt
  if (!subject.proposalId || !subject.outcome || !subject.dagAnchor) {
    return false;
  }
  
  return true;
}

/**
 * Checks if a user has the right to perform an action in a federation
 * @param {Array} credentials - User's credentials
 * @param {string} action - The action to verify permission for
 * @param {string} federationId - The federation ID to check permissions against
 * @returns {boolean} Whether user has permission
 */
export function checkFederationPermission(credentials, action, federationId) {
  // Check for federation-specific permission credentials
  const permissionCred = credentials.find(cred => 
    cred.type === 'FederationPermission' && 
    cred.metadata.federationId === federationId &&
    cred.metadata.permissions.includes(action)
  );
  
  if (permissionCred) return true;
  
  // Check for admin credentials which provide all permissions
  const adminCred = credentials.find(cred => 
    cred.type === 'FederationAdmin' && 
    cred.metadata.federationId === federationId
  );
  
  return !!adminCred;
} 