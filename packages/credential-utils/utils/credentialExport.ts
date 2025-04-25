import { WalletCredential } from '../types';
import { filterCredentialsByFederation } from './federation';

/**
 * Export a wallet credential as a verifiable credential JSON file
 * @param credential The wallet credential to export
 */
export const exportCredentialAsVC = (credential: WalletCredential): void => {
  // Convert wallet credential to VC format
  const vc = {
    '@context': [
      'https://www.w3.org/2018/credentials/v1',
      'https://identity.foundation/presentation-exchange/submission/v1'
    ],
    type: ['VerifiableCredential', credential.type],
    issuer: credential.issuer.did,
    issuanceDate: credential.issuanceDate,
    credentialSubject: {
      id: credential.subjectDid,
      ...credential.credentialSubject
    },
    // Include other credential properties
    ...(credential.expirationDate && { expirationDate: credential.expirationDate }),
    ...(credential.proof && { proof: credential.proof }),
    // Include thread ID in the credential if available
    ...(credential.metadata?.agoranet && {
      metadata: {
        agoranet: credential.metadata.agoranet
      }
    })
  };

  // Create a JSON file and trigger download
  const blob = new Blob([JSON.stringify(vc, null, 2)], { type: 'application/json' });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = `${credential.title.replace(/\s+/g, '_')}.json`;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
};

/**
 * Generate a filename for downloading a credential
 * @param credential The wallet credential to generate a filename for
 * @returns A formatted filename for the credential
 */
export const getCredentialFilename = (credential: WalletCredential): string => {
  const shortHash = credential.receiptHash ? credential.receiptHash.substring(0, 8) : 'unknown';
  const formattedType = credential.type.replace(/[^a-z0-9]/gi, '-').toLowerCase();
  return `${formattedType}-receipt-${shortHash}.vc.json`;
};

/**
 * Get a human-readable type name for a credential type
 * @param type The credential type identifier
 * @returns A human-readable type name
 */
export const getCredentialTypeName = (type: string): string => {
  const typeMap: Record<string, string> = {
    'proposal': 'Proposal Submission',
    'vote': 'Guardian Vote',
    'finalization': 'Proposal Finalization',
    'appeal': 'Appeal Submission',
    'appeal_vote': 'Appeal Vote',
    'appeal_finalization': 'Appeal Finalization',
    'execution': 'Execution Receipt'
  };
  
  return typeMap[type] || 'Receipt';
};

/**
 * Creates a Verifiable Presentation containing all credentials from a specific federation
 * @param credentials Array of credentials to include in the presentation
 * @param federationId The federation ID to filter by
 * @param holderDid The DID of the holder/creator of the presentation
 * @param options Selective disclosure options
 * @returns JSON string of the Verifiable Presentation
 */
export function createFederationPresentation(
  credentials: WalletCredential[],
  federationId: string,
  holderDid: string,
  options?: {
    includeOnly?: string[];
    redactFields?: string[];
    reason?: string;
  }
): string {
  // Filter credentials by federation
  const federationCredentials = filterCredentialsByFederation(credentials, federationId);
  
  if (federationCredentials.length === 0) {
    throw new Error(`No credentials found for federation: ${federationId}`);
  }
  
  // Convert wallet credentials to VerifiableCredential format with selective disclosure if needed
  const verifiableCredentials = federationCredentials.map(credential => {
    // Apply selective disclosure if options are provided
    if (options?.includeOnly || options?.redactFields) {
      // Convert credential to VC first
      const vc = convertToVerifiableCredential(credential);
      
      // Apply selective disclosure
      return applySelectiveDisclosure(vc, {
        includeFields: options.includeOnly,
        excludeFields: options.redactFields,
        reason: options.reason || `Federation presentation with selective disclosure`,
      });
    }
    
    // No selective disclosure needed, just convert to VC
    return convertToVerifiableCredential(credential);
  });
  
  // Get federation details from the first credential
  const federationName = federationCredentials[0]?.metadata?.federation?.name || federationId;
  
  // Create the presentation
  const presentation = {
    '@context': [
      'https://www.w3.org/2018/credentials/v1',
      'https://www.w3.org/2018/credentials/examples/v1',
      'https://identity.foundation/presentation-exchange/submission/v1'
    ],
    'type': ['VerifiablePresentation', 'FederationCredentialsPresentation'],
    'id': `urn:uuid:${generateUUID()}`,
    'holder': holderDid,
    'verifiableCredential': verifiableCredentials,
    'metadata': {
      'federation': {
        'id': federationId,
        'name': federationName,
        'credentialCount': federationCredentials.length,
        'exportDate': new Date().toISOString()
      },
      'selectiveDisclosure': options ? {
        'includeOnly': options.includeOnly,
        'redactFields': options.redactFields,
        'reason': options.reason
      } : undefined
    }
  };
  
  return JSON.stringify(presentation, null, 2);
}

/**
 * Generate a UUID v4
 * @returns UUID string
 */
function generateUUID(): string {
  return 'xxxxxxxx-xxxx-4xxx-yxxx-xxxxxxxxxxxx'.replace(/[xy]/g, function(c) {
    const r = Math.random() * 16 | 0,
      v = c === 'x' ? r : (r & 0x3 | 0x8);
    return v.toString(16);
  });
}

/**
 * Converts a wallet credential to the standard VerifiableCredential format
 * @param credential The wallet credential to convert
 * @returns A VerifiableCredential object
 */
function convertToVerifiableCredential(credential: WalletCredential) {
  // Create the basic structure with proper typing
  const vc: any = {
    '@context': [
      'https://www.w3.org/2018/credentials/v1',
      'https://identity.foundation/linked-federation/v1'
    ],
    'id': credential.id,
    'type': ['VerifiableCredential', getCredentialTypeName(credential.type)],
    'issuer': {
      'id': credential.issuer.did,
      'name': credential.issuer.name
    },
    'issuanceDate': credential.issuanceDate,
    'credentialSubject': {
      'id': credential.subjectDid,
      ...credential.credentialSubject
    }
  };
  
  // Add proof if available
  if (credential.proof) {
    vc.proof = credential.proof;
  }
  
  // Add expiration date if available
  if (credential.expirationDate) {
    vc.expirationDate = credential.expirationDate;
  }
  
  // Add federation metadata if available
  if (credential.metadata?.federation || credential.metadata?.agoranet?.federation_id) {
    vc.credentialSubject.federation = {
      'id': credential.metadata?.federation?.id || 
           credential.metadata?.agoranet?.federation_id,
      'name': credential.metadata?.federation?.name
    };
  }
  
  // Add thread reference if available
  if (credential.metadata?.agoranet?.threadId) {
    vc.credentialSubject.thread = {
      'id': credential.metadata.agoranet.threadId,
      'url': credential.metadata.agoranet.threadUrl
    };
  }
  
  return vc;
}

/**
 * Apply selective disclosure to a verifiable credential
 * @param vc The verifiable credential to apply selective disclosure to
 * @param options Selective disclosure options
 * @returns The selectively disclosed verifiable credential
 */
function applySelectiveDisclosure(vc: any, options: {
  includeFields?: string[];
  excludeFields?: string[];
  reason?: string;
}): any {
  // Create a copy of the VC to avoid modifying the original
  const disclosedVC: any = JSON.parse(JSON.stringify(vc));
  
  // Fields that should never be redacted
  const protectedFields = ['@context', 'id', 'type', 'issuer', 'issuanceDate'];
  
  // Helper function to check if field should be included
  const shouldIncludeField = (field: string): boolean => {
    // Always include protected fields
    if (protectedFields.includes(field)) {
      return true;
    }
    
    // If includeFields is specified, only include fields in that list
    if (options.includeFields && options.includeFields.length > 0) {
      return options.includeFields.some(includePattern => {
        // Allow prefix matching with * wildcard (e.g. "credentialSubject.*")
        if (includePattern.endsWith('*')) {
          const prefix = includePattern.slice(0, -1);
          return field === prefix || field.startsWith(`${prefix}.`);
        }
        return field === includePattern;
      });
    }
    
    // If excludeFields is specified, exclude fields in that list
    if (options.excludeFields && options.excludeFields.length > 0) {
      return !options.excludeFields.some(excludePattern => {
        // Allow prefix matching with * wildcard
        if (excludePattern.endsWith('*')) {
          const prefix = excludePattern.slice(0, -1);
          return field === prefix || field.startsWith(`${prefix}.`);
        }
        return field === excludePattern;
      });
    }
    
    // By default, include all fields
    return true;
  };
  
  // Helper function to extract fields from an object
  const extractFields = (obj: any, prefix = ''): string[] => {
    if (!obj || typeof obj !== 'object') return [];
    
    let fields: string[] = [];
    for (const key in obj) {
      const newPrefix = prefix ? `${prefix}.${key}` : key;
      fields.push(newPrefix);
      
      if (obj[key] && typeof obj[key] === 'object' && !Array.isArray(obj[key])) {
        fields = fields.concat(extractFields(obj[key], newPrefix));
      }
    }
    
    return fields;
  };
  
  // Extract all fields from the VC
  const allFields = extractFields(vc);
  
  // Get fields to be redacted
  const fieldsToRedact = allFields.filter(field => !shouldIncludeField(field));
  
  // Function to redact a field in an object
  const redactField = (obj: any, fieldPath: string): void => {
    const parts = fieldPath.split('.');
    const lastPart = parts.pop()!;
    
    // Navigate to the parent object
    let current = obj;
    for (const part of parts) {
      if (!current[part]) return; // Path doesn't exist
      current = current[part];
    }
    
    // Redact the field if it exists
    if (current && current[lastPart] !== undefined) {
      current[lastPart] = '[REDACTED]';
    }
  };
  
  // Redact fields
  fieldsToRedact.forEach(field => redactField(disclosedVC, field));
  
  // Add selective disclosure metadata
  disclosedVC.selectiveDisclosureMetadata = {
    originalCredentialId: vc.id,
    redactedFields: fieldsToRedact,
    disclosedFields: allFields.filter(field => !fieldsToRedact.includes(field)),
    proofType: 'redaction',
    reason: options.reason || 'Selective disclosure for federation presentation',
    timestamp: new Date().toISOString()
  };
  
  return disclosedVC;
} 