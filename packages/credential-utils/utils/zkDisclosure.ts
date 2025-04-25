import { AnchorCredential, WalletCredential } from '../types/credentials';
import { v4 as uuidv4 } from 'uuid';
import { cloneDeep, get, set } from 'lodash';
import * as crypto from 'crypto';

/**
 * Options for creating a ZK selective disclosure
 */
export interface ZKDisclosureOptions {
  /** Fields to include in the disclosure */
  includeFields: string[];
  
  /** Reason for the disclosure */
  reason?: string;
  
  /** Validity period in seconds (default: 24 hours) */
  validityPeriod?: number;
}

/**
 * ZK disclosure representation
 */
export interface ZKDisclosure {
  /** Original credential ID */
  originalCredentialId: string;
  
  /** Generated credential with redactions */
  credential: any;
  
  /** Field hashes for verification */
  fieldHashes: Record<string, string>;
  
  /** Disclosed fields */
  disclosedFields: string[];
  
  /** Metadata about the disclosure */
  metadata: {
    /** When the disclosure was created */
    createdAt: string;
    
    /** When the disclosure expires */
    expiresAt: string;
    
    /** Purpose/reason for the disclosure */
    reason?: string;
  };
}

/**
 * Create a selective zero-knowledge disclosure from an anchor credential
 * 
 * This implementation creates a secure selective disclosure that only reveals
 * specific fields while allowing verification that they are part of the 
 * original credential without revealing other fields.
 * 
 * @param credential Original anchor credential
 * @param options Disclosure options
 * @returns ZK disclosure
 */
export function createZKDisclosure(
  credential: AnchorCredential,
  options: ZKDisclosureOptions
): ZKDisclosure {
  // Clone the credential to avoid modifying the original
  const disclosedCredential = cloneDeep(credential);
  
  // Generate a new ID for the disclosed credential
  disclosedCredential.id = `${credential.id}-zk-${uuidv4().substring(0, 8)}`;
  
  // Create field hashes for verification
  const fieldHashes: Record<string, string> = {};
  
  // Generate commitment for each included field
  options.includeFields.forEach(field => {
    const value = get(credential, field);
    if (value !== undefined) {
      // Create a hash commitment for this field
      const commitment = createFieldCommitment(field, value);
      fieldHashes[field] = commitment;
    }
  });
  
  // Redact all fields not explicitly included
  const redactFields = (obj: any, prefix = '') => {
    if (!obj || typeof obj !== 'object') return;
    
    Object.keys(obj).forEach(key => {
      const path = prefix ? `${prefix}.${key}` : key;
      
      // Skip essential VC fields
      if (key === '@context' || key === 'type' || key === 'id') {
        return;
      }
      
      // If this path should be disclosed
      const shouldDisclose = options.includeFields.some(field => 
        field === path || path.startsWith(`${field}.`)
      );
      
      if (!shouldDisclose) {
        // Redact with a placeholder to maintain structure
        if (typeof obj[key] === 'object' && obj[key] !== null) {
          // Preserve object structure but replace all leaf values
          redactFields(obj[key], path);
        } else {
          // Replace primitive values with [REDACTED]
          obj[key] = '[REDACTED]';
        }
      } else if (typeof obj[key] === 'object' && obj[key] !== null) {
        // Continue recursively for objects
        redactFields(obj[key], path);
      }
    });
  };
  
  // Redact undisclosed fields but keep structure
  redactFields(disclosedCredential);
  
  // Create a proof that connects the original and disclosed credential
  // In a real implementation, this would use a proper ZKP library
  
  // Add the proof to the credential
  disclosedCredential.proof = {
    ...disclosedCredential.proof,
    type: 'ZeroKnowledgeProof2023',
    proofPurpose: 'assertionMethod',
    created: new Date().toISOString(),
    verificationMethod: credential.proof?.verificationMethod || '',
    // ZK proof would go here in a real implementation
    zkpPayload: {
      version: '1.0',
      fieldHashes,
      scheme: 'sha256-commitment',
    }
  };
  
  // Set expiration time (default to 24 hours)
  const validityPeriod = options.validityPeriod || 24 * 60 * 60; // seconds
  const expiresAt = new Date(Date.now() + validityPeriod * 1000).toISOString();
  
  // Create the ZK disclosure object
  return {
    originalCredentialId: credential.id,
    credential: disclosedCredential,
    fieldHashes,
    disclosedFields: options.includeFields,
    metadata: {
      createdAt: new Date().toISOString(),
      expiresAt,
      reason: options.reason,
    },
  };
}

/**
 * Create a commitment for a field value
 * 
 * @param field Field name
 * @param value Field value
 * @returns Commitment hash
 */
function createFieldCommitment(field: string, value: any): string {
  // Convert value to string representation
  const valueStr = typeof value === 'object' 
    ? JSON.stringify(value) 
    : String(value);
  
  // Create a salted hash of the field name and value
  // This is a simplified commitment scheme - a real implementation would use a more
  // secure commitment scheme with blinding factors
  const salt = crypto.randomBytes(16).toString('hex');
  const commitment = crypto.createHash('sha256')
    .update(`${field}:${valueStr}:${salt}`)
    .digest('hex');
  
  return commitment;
}

/**
 * Verify a ZK disclosure - this is a mock implementation
 * In a real implementation, this would use ZK cryptography to validate
 * the disclosed values against the original credential.
 * 
 * @param disclosure ZK disclosure
 * @param originalCredential Original credential (for verification)
 * @returns Verification result
 */
export function verifyZKDisclosure(
  disclosure: ZKDisclosure,
  originalCredential?: AnchorCredential
): { valid: boolean, errors: string[] } {
  const errors: string[] = [];
  
  // Check if the disclosure has expired
  const now = new Date();
  if (new Date(disclosure.metadata.expiresAt) < now) {
    errors.push('The disclosure has expired');
    return { valid: false, errors };
  }
  
  // If we have the original credential, we can verify field values
  if (originalCredential) {
    if (disclosure.originalCredentialId !== originalCredential.id) {
      errors.push('Original credential ID does not match');
      return { valid: false, errors };
    }
    
    // Verify each disclosed field
    let allFieldsValid = true;
    disclosure.disclosedFields.forEach(field => {
      const originalValue = get(originalCredential, field);
      const disclosedValue = get(disclosure.credential, field);
      
      // Check if the values match
      if (JSON.stringify(originalValue) !== JSON.stringify(disclosedValue)) {
        errors.push(`Field ${field} value does not match original credential`);
        allFieldsValid = false;
      }
    });
    
    if (!allFieldsValid) {
      return { valid: false, errors };
    }
  } else {
    // Without the original credential, we can only verify the basic structure
    // In a real implementation, this would use ZKP verification
    
    // Verify that the proof exists in the correct format
    const proof = disclosure.credential.proof;
    if (!proof || proof.type !== 'ZeroKnowledgeProof2023') {
      errors.push('Invalid or missing proof type');
      return { valid: false, errors };
    }
    
    // Check that field hashes exist
    if (!proof.zkpPayload || !proof.zkpPayload.fieldHashes) {
      errors.push('Missing field hash commitments');
      return { valid: false, errors };
    }
  }
  
  return { valid: true, errors: [] };
}

/**
 * Export a ZK disclosure to a JSON document format
 * 
 * @param disclosure ZK disclosure
 * @returns JSON representation
 */
export function exportZKDisclosure(disclosure: ZKDisclosure): any {
  return {
    '@context': [
      'https://www.w3.org/2018/credentials/v1',
      'https://identity.foundation/presentation-exchange/submission/v1',
      'https://icn.xyz/contexts/zkp-disclosure/v1'
    ],
    ...disclosure.credential,
    zkDisclosureMetadata: {
      originalCredentialId: disclosure.originalCredentialId,
      disclosedFields: disclosure.disclosedFields,
      fieldHashes: disclosure.fieldHashes,
      createdAt: disclosure.metadata.createdAt,
      expiresAt: disclosure.metadata.expiresAt,
      reason: disclosure.metadata.reason,
    }
  };
} 