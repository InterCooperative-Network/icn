import { WalletCredential, SelectiveDisclosureOptions, SelectiveDisclosure } from '../types';
import { v4 as uuidv4 } from 'uuid';
import { set, get, unset, cloneDeep } from 'lodash';

/**
 * Get all fields from an object using dot notation
 * @param obj Object to extract fields from
 * @param prefix Current path prefix
 * @returns Array of field paths in dot notation
 */
function getAllFields(obj: any, prefix = ''): string[] {
  if (!obj || typeof obj !== 'object') return [];
  
  return Object.keys(obj).reduce((fields: string[], key) => {
    const newPrefix = prefix ? `${prefix}.${key}` : key;
    
    if (obj[key] && typeof obj[key] === 'object' && !Array.isArray(obj[key])) {
      return [...fields, newPrefix, ...getAllFields(obj[key], newPrefix)];
    }
    
    return [...fields, newPrefix];
  }, []);
}

/**
 * Create a selective disclosure of a credential
 * @param credential The original credential
 * @param options Selective disclosure options
 * @returns A selective disclosure with redacted fields
 */
export function createSelectiveDisclosure(
  credential: WalletCredential,
  options: SelectiveDisclosureOptions
): SelectiveDisclosure {
  // Clone the credential to avoid modifying the original
  const disclosedCredential = cloneDeep(credential);
  
  // Generate a new ID for the disclosed credential
  disclosedCredential.id = `${credential.id}-sd-${uuidv4().substring(0, 8)}`;
  
  // Get all available fields in the credential
  const allFields = getAllFields(credential);
  
  // Determine which fields to include
  let fieldsToInclude = options.includeFields?.length 
    ? options.includeFields 
    : allFields;
  
  // Remove excluded fields
  if (options.excludeFields?.length) {
    fieldsToInclude = fieldsToInclude.filter(
      field => !options.excludeFields?.some(exclude => 
        field === exclude || field.startsWith(`${exclude}.`)
      )
    );
  }
  
  // Find fields to redact
  const fieldsToRedact = allFields.filter(
    field => !fieldsToInclude.some(include => 
      field === include || field.startsWith(`${include}.`)
    )
  );
  
  // Handle different proof types
  if (options.proofType === 'redaction') {
    // For redaction, simply remove fields
    fieldsToRedact.forEach(field => {
      // Skip @context, type, id, issuer, and issuanceDate which are required for VC validity
      if (field === '@context' || field === 'type' || field === 'id' || 
          field === 'issuer' || field === 'issuanceDate') {
        return;
      }
      
      try {
        unset(disclosedCredential, field);
      } catch (e) {
        console.warn(`Couldn't unset field: ${field}`, e);
      }
    });
    
    // Add redaction metadata to proof
    disclosedCredential.proof = {
      ...disclosedCredential.proof,
      type: `${disclosedCredential.proof.type}WithRedaction`,
      redactedFields: fieldsToRedact,
    };
  } else if (options.proofType === 'zk') {
    // For ZK proofs, we'd implement zero-knowledge proof generation
    // This is a placeholder for future ZK implementation
    disclosedCredential.proof = {
      ...disclosedCredential.proof,
      type: `${disclosedCredential.proof.type}WithZKP`,
      // Add ZK-specific proof properties here
    };
    
    // Mark redacted fields with '***' instead of removing them
    fieldsToRedact.forEach(field => {
      try {
        set(disclosedCredential, field, '[REDACTED]');
      } catch (e) {
        console.warn(`Couldn't modify field: ${field}`, e);
      }
    });
  }
  
  // Add selective disclosure marker
  disclosedCredential.type = [...disclosedCredential.type, 'SelectiveDisclosure'];
  
  // Return the selective disclosure
  return {
    originalCredentialId: credential.id,
    credential: disclosedCredential,
    disclosedFields: fieldsToInclude,
    redactedFields: fieldsToRedact,
    proofType: options.proofType,
    metadata: {
      createdAt: new Date().toISOString(),
      reason: options.reason,
      // Default to 24 hour expiry if not specified
      expiresAt: new Date(Date.now() + 24 * 60 * 60 * 1000).toISOString(),
    }
  };
}

/**
 * Verify a selective disclosure against its original credential
 * @param disclosure The selective disclosure to verify
 * @param originalCredential The original credential
 * @returns Whether the disclosure is valid
 */
export function verifySelectiveDisclosure(
  disclosure: SelectiveDisclosure,
  originalCredential: WalletCredential
): boolean {
  // Verify the original credential ID matches
  if (disclosure.originalCredentialId !== originalCredential.id) {
    return false;
  }
  
  // For redaction proofs, verify that the disclosed fields match
  if (disclosure.proofType === 'redaction') {
    // Check that each included field has the same value
    return disclosure.disclosedFields.every(field => {
      try {
        const originalValue = get(originalCredential, field);
        const disclosedValue = get(disclosure.credential, field);
        
        // Compare values, considering arrays and objects
        if (typeof originalValue === 'object' && originalValue !== null) {
          return JSON.stringify(originalValue) === JSON.stringify(disclosedValue);
        }
        
        return originalValue === disclosedValue;
      } catch (e) {
        return false;
      }
    });
  } else if (disclosure.proofType === 'zk') {
    // For ZK proofs, we would implement zero-knowledge verification
    // This is a placeholder for future ZK implementation
    console.warn('ZK proof verification not yet implemented');
    return false;
  }
  
  return false;
}

/**
 * Export a selective disclosure as a JSON-LD document
 * @param disclosure The selective disclosure to export
 * @returns A JSON-LD document
 */
export function exportSelectiveDisclosure(disclosure: SelectiveDisclosure): any {
  // Convert to standard VC format with selective disclosure metadata
  return {
    '@context': [
      'https://www.w3.org/2018/credentials/v1',
      'https://identity.foundation/presentation-exchange/submission/v1',
      'https://icn.xyz/contexts/selective-disclosure/v1'
    ],
    ...disclosure.credential,
    selectiveDisclosureMetadata: {
      originalCredentialId: disclosure.originalCredentialId,
      disclosedFields: disclosure.disclosedFields,
      redactedFields: disclosure.redactedFields,
      proofType: disclosure.proofType,
      createdAt: disclosure.metadata.createdAt,
      expiresAt: disclosure.metadata.expiresAt,
      reason: disclosure.metadata.reason
    }
  };
} 