import { v4 as uuidv4 } from 'uuid';
import { WalletCredential, AnchorCredential } from '../types/credentials';
import { FederationManifest } from '../types/federation';
// Use type-only import for Node.js Buffer
// @ts-ignore
import type { Buffer } from 'buffer';
import * as crypto from 'crypto';

// Add Node.js type declarations for require
declare function require(moduleName: string): any;

/**
 * Options for creating an anchor credential
 */
export interface AnchorCredentialOptions {
  /** Type of anchor credential */
  anchorType: 'epoch' | 'mandate' | 'role_assignment' | 'membership' | 'amendment';
  
  /** Federation information */
  federation: {
    id: string;
    did: string;
    name: string;
  };
  
  /** DID of the subject */
  subjectDid: string;
  
  /** Private key of the signer */
  privateKey: string;
  
  /** Root hash of the DAG */
  dagRootHash: string;
  
  /** When the credential becomes effective */
  effectiveFrom: string;
  
  /** When the credential expires (optional) */
  effectiveUntil?: string;
  
  /** Epoch ID (for epoch credentials) */
  epochId?: string;
  
  /** Mandate details (for mandate credentials) */
  mandate?: string;
  
  /** Role (for role_assignment credentials) */
  role?: string;
  
  /** Height of the DAG block */
  dagBlockHeight?: number;
  
  /** List of credentials referenced by this anchor */
  referencedCredentials?: string[];
  
  /** Keys of federation members for multi-signature (optional) */
  memberKeys?: Record<string, string>;
  
  /** Minimum required signatures (optional) */
  minSignatures?: number;
  
  /** Federation manifest (optional) */
  federationManifest?: FederationManifest;
  
  /** Amendment ID (for amendment credentials) */
  amendmentId?: string;
  
  /** Previous amendment ID (for amendment credentials) */
  previousAmendmentId?: string;
  
  /** Text hash (for amendment credentials) */
  textHash?: string;
  
  /** Ratified in epoch (for amendment credentials) */
  ratifiedInEpoch?: string;
}

// Define the structure of metadata to avoid type errors
export interface AnchorCredentialMetadata {
  federation: {
    id: string;
    name: string;
  };
  dag: {
    root_hash: string;
    timestamp: string;
    block_height?: number;
  };
}

// Update the credential type to include properly typed metadata
export interface EnhancedAnchorCredential extends Omit<AnchorCredential, 'metadata'> {
  metadata: AnchorCredentialMetadata;
}

/**
 * Create a new anchor credential
 * 
 * @param options Options for creating the anchor
 * @returns A new anchor credential
 */
export async function createAnchorCredential(
  options: AnchorCredentialOptions
): Promise<AnchorCredential> {
  const now = new Date();
  const id = `urn:anchor:${options.anchorType}:${uuidv4()}`;
  
  // Basic credential structure
  const credential: EnhancedAnchorCredential = {
    id,
    title: getAnchorTitle(options),
    type: 'anchor_credential',
    anchorType: options.anchorType,
    issuer: {
      did: options.federation.did,
      name: options.federation.name,
    },
    subjectDid: options.subjectDid,
    issuanceDate: now.toISOString(),
    credentialSubject: {
      id: options.subjectDid,
      effective_from: options.effectiveFrom,
      dag_root_hash: options.dagRootHash,
    },
    metadata: {
      federation: {
        id: options.federation.id,
        name: options.federation.name,
      },
      dag: {
        root_hash: options.dagRootHash,
        timestamp: now.toISOString(),
      }
    },
    proof: {
      type: 'Ed25519Signature2020',
      created: now.toISOString(),
      verificationMethod: `${options.federation.did}#controller`,
      proofPurpose: 'assertionMethod',
      jws: '', // Will be set below
    }
  };
  
  // Add optional fields
  if (options.epochId) {
    credential.credentialSubject.epoch_id = options.epochId;
  }
  
  if (options.mandate) {
    credential.credentialSubject.mandate = options.mandate;
  }
  
  if (options.role) {
    credential.credentialSubject.role = options.role;
  }
  
  if (options.effectiveUntil) {
    credential.credentialSubject.effective_until = options.effectiveUntil;
    credential.expirationDate = options.effectiveUntil;
  }
  
  if (options.referencedCredentials && options.referencedCredentials.length > 0) {
    credential.credentialSubject.referenced_credentials = options.referencedCredentials;
  }
  
  if (options.dagBlockHeight) {
    credential.metadata.dag.block_height = options.dagBlockHeight;
  }
  
  // Add amendment-specific fields
  if (options.anchorType === 'amendment') {
    if (options.amendmentId) {
      credential.credentialSubject.amendment_id = options.amendmentId;
    }
    
    if (options.previousAmendmentId) {
      credential.credentialSubject.previous_amendment_id = options.previousAmendmentId;
    }
    
    if (options.textHash) {
      credential.credentialSubject.text_hash = options.textHash;
    }
    
    if (options.ratifiedInEpoch) {
      credential.credentialSubject.ratified_in_epoch = options.ratifiedInEpoch;
    }
  }
  
  // Create the signature
  const signature = await signAnchorCredential(
    credential,
    options.privateKey,
    options.memberKeys,
    options.minSignatures,
    options.federationManifest
  );
  
  // Add the signature to the credential
  if (credential.proof) {
    credential.proof.jws = signature.jws;
  }
  
  // Add multi-signature proof if available
  if (signature.multiSignatures && signature.multiSignatures.length > 0) {
    (credential as any).multiSignatureProof = {
      type: 'Ed25519MultisignatureQuorum2023',
      created: now.toISOString(),
      proofPurpose: 'assertionMethod',
      signatures: signature.multiSignatures
    };
  }
  
  return credential;
}

/**
 * Generate a human-readable title for an anchor credential
 */
function getAnchorTitle(options: AnchorCredentialOptions): string {
  switch (options.anchorType) {
    case 'epoch':
      return options.epochId 
        ? `Epoch Transition: ${options.epochId}`
        : 'Epoch Transition';
    
    case 'mandate':
      return options.mandate
        ? `Federation Mandate: ${options.mandate.substring(0, 30)}${options.mandate.length > 30 ? '...' : ''}`
        : 'Federation Mandate';
    
    case 'role_assignment':
      return options.role
        ? `Role Assignment: ${options.role}`
        : 'Role Assignment';
    
    case 'membership':
      return `Federation Membership: ${options.federation.name}`;
    
    default:
      return `Anchor Credential: ${options.anchorType}`;
  }
}

/**
 * Sign an anchor credential
 */
async function signAnchorCredential(
  credential: AnchorCredential,
  privateKey: string,
  memberKeys?: Record<string, string>,
  minSignatures?: number,
  manifest?: FederationManifest
): Promise<{
  jws: string;
  multiSignatures?: Array<{
    verificationMethod: string;
    created: string;
    jws: string;
  }>;
}> {
  // Data to sign (credential without proof)
  const { proof, ...credentialWithoutProof } = credential;
  const dataToSign = JSON.stringify(credentialWithoutProof);
  
  // Create the primary signature
  let jws = '';
  try {
    // In a real implementation, this would use the private key to create a proper JWS
    // For this example, we'll create a mock JWS
    const header = {
      alg: 'EdDSA',
      typ: 'JWT',
      kid: `${credential.issuer.did}#controller`
    };
    
    // Use a Node.js and browser compatible Base64 encoding
    const encodedHeader = base64Encode(JSON.stringify(header));
    const encodedPayload = base64Encode(dataToSign);
    const mockSignature = base64Encode(`sig-${credential.issuer.did}-${Date.now()}`);
    
    jws = `${encodedHeader}.${encodedPayload}.${mockSignature}`;
  } catch (error) {
    console.error('Error signing anchor credential:', error);
    throw new Error(`Failed to sign anchor credential: ${error}`);
  }
  
  // Create multi-signatures if requested
  const multiSignatures: Array<{
    verificationMethod: string;
    created: string;
    jws: string;
  }> = [];
  
  if (memberKeys && Object.keys(memberKeys).length > 0 && manifest) {
    // Add the primary signature first
    multiSignatures.push({
      verificationMethod: `${credential.issuer.did}#controller`,
      created: credential.issuanceDate,
      jws
    });
    
    // Determine minimum signatures required
    const signaturesNeeded = minSignatures || 
                            (manifest.quorum_rules?.min_approvals || 3);
    
    // Create signatures for each available member key
    for (const [memberDid, memberKey] of Object.entries(memberKeys)) {
      // Skip if not a valid federation member
      if (!manifest.members || !manifest.members[memberDid]) {
        console.warn(`Skipping signature for non-member: ${memberDid}`);
        continue;
      }
      
      try {
        // Create a mock signature for this member
        const header = {
          alg: 'EdDSA',
          typ: 'JWT',
          kid: `${memberDid}#keys-1`
        };
        
        // Use a Node.js and browser compatible Base64 encoding
        const encodedHeader = base64Encode(JSON.stringify(header));
        const encodedPayload = base64Encode(dataToSign);
        const mockSignature = base64Encode(`sig-${memberDid}-${Date.now()}`);
        
        multiSignatures.push({
          verificationMethod: `${memberDid}#keys-1`,
          created: credential.issuanceDate,
          jws: `${encodedHeader}.${encodedPayload}.${mockSignature}`
        });
        
        // Stop if we have enough signatures
        if (multiSignatures.length >= signaturesNeeded) {
          break;
        }
      } catch (error) {
        console.warn(`Error creating signature for ${memberDid}:`, error);
      }
    }
  }
  
  return {
    jws,
    ...(multiSignatures.length > 0 && { multiSignatures })
  };
}

/**
 * Base64 encode a string (works in both Node.js and browser)
 */
function base64Encode(str: string): string {
  // Use a method that works in both browser and Node.js environments
  try {
    // Browser environment
    return btoa(str);
  } catch (err) {
    // Node.js environment
    try {
      // Safely access Buffer in a way that works in both environments
      const Buffer = typeof window === 'undefined' 
        ? require('buffer').Buffer 
        : null;
      
      if (Buffer) {
        return Buffer.from(str).toString('base64');
      }
      throw new Error('Buffer not available');
    } catch (e) {
      // Fallback for environments without Buffer
      console.warn('Failed to encode using Buffer, using fallback method', e);
      return str;
    }
  }
}

/**
 * Verify an anchor credential
 * 
 * @param credential The anchor credential to verify
 * @param publicKey The federation's public key
 * @param manifest The federation manifest for multi-signature verification
 * @returns Whether the anchor credential is valid
 */
export async function verifyAnchorCredential(
  credential: AnchorCredential,
  publicKey: string,
  manifest?: FederationManifest
): Promise<{
  valid: boolean;
  multiSigValid?: boolean;
  errors?: string[];
}> {
  const errors: string[] = [];
  let valid = false;
  let multiSigValid = false;
  
  try {
    // Verify the primary signature
    if (credential.proof && credential.proof.jws) {
      // In a real implementation, this would use the public key to verify the JWS
      // For this example, we'll assume it's valid if it follows the expected format
      const jwsParts = credential.proof.jws.split('.');
      
      if (jwsParts.length !== 3) {
        errors.push('Invalid JWS format');
      } else {
        // In a real implementation, we would verify the signature against the public key
        // For now, we'll just check if it exists and has the right format
        valid = true;
      }
    } else {
      errors.push('Missing proof or JWS');
    }
    
    // Verify multi-signatures if they exist
    const multiSigProof = (credential as any).multiSignatureProof;
    
    if (multiSigProof && multiSigProof.signatures && multiSigProof.signatures.length > 0) {
      if (!manifest || !manifest.quorum_rules || !manifest.members) {
        errors.push('Federation manifest required for multi-signature verification');
      } else {
        const minSignatures = manifest.quorum_rules.min_approvals || 3;
        const validSignatures = multiSigProof.signatures.filter((sig: any) => {
          // Extract the did from the verification method
          const did = sig.verificationMethod.split('#')[0];
          
          // Check if the signer is a valid federation member
          if (!manifest.members[did]) {
            return false;
          }
          
          // In a real implementation, we would verify each signature
          // For now, we'll just check if it follows the expected format
          const jwsParts = sig.jws.split('.');
          return jwsParts.length === 3;
        });
        
        if (validSignatures.length >= minSignatures) {
          multiSigValid = true;
        } else {
          errors.push(`Insufficient valid signatures: ${validSignatures.length}/${minSignatures}`);
        }
      }
    }
    
    return {
      valid,
      ...(multiSigProof && { multiSigValid }),
      ...(errors.length > 0 && { errors })
    };
  } catch (error) {
    return {
      valid: false,
      errors: [`Error verifying anchor credential: ${error}`]
    };
  }
}

/**
 * Get all credentials referenced by an anchor credential
 * 
 * @param credential The anchor credential
 * @param getCredentialFn Function to retrieve a credential by ID
 * @returns The referenced credentials
 */
export async function getReferencedCredentials(
  credential: AnchorCredential,
  getCredentialFn: (id: string) => Promise<WalletCredential | null>
): Promise<WalletCredential[]> {
  if (!credential.credentialSubject.referenced_credentials || 
      credential.credentialSubject.referenced_credentials.length === 0) {
    return [];
  }
  
  const result: WalletCredential[] = [];
  
  for (const id of credential.credentialSubject.referenced_credentials) {
    try {
      const cred = await getCredentialFn(id);
      if (cred) {
        result.push(cred);
      }
    } catch (error) {
      console.warn(`Failed to retrieve referenced credential ${id}:`, error);
    }
  }
  
  return result;
}

/**
 * Get anchor credentials for a specific DAG root
 * 
 * @param dagRootHash The DAG root hash to search for
 * @param credentials Array of credentials to search
 * @returns Anchor credentials matching the DAG root
 */
export function getAnchorCredentialsForDagRoot(
  dagRootHash: string,
  credentials: WalletCredential[]
): AnchorCredential[] {
  return credentials
    .filter((cred): cred is EnhancedAnchorCredential => {
      // Type guard to ensure we only return anchor credentials with matching DAG root
      const isAnchorCredential = cred.type === 'anchor_credential';
      if (!isAnchorCredential) return false;
      
      const hasMetadata = cred.metadata !== undefined && typeof cred.metadata === 'object';
      if (!hasMetadata) return false;
      
      // Since we're using a type guard, we need to check if the metadata has the expected structure
      const metadata = cred.metadata as any;
      const hasDag = metadata && 'dag' in metadata;
      if (!hasDag) return false;
      
      const dag = metadata.dag;
      const dagIsObject = typeof dag === 'object' && dag !== null;
      if (!dagIsObject) return false;
      
      const hasRootHash = 'root_hash' in dag;
      if (!hasRootHash) return false;
      
      return dag.root_hash === dagRootHash;
    })
    .sort((a, b) => {
      // Sort by issuance date (newest first)
      return new Date(b.issuanceDate).getTime() - new Date(a.issuanceDate).getTime();
    });
}

/**
 * Find all anchor credentials in a collection
 * 
 * @param credentials Collection of credentials to search
 * @returns All anchor credentials found
 */
export function findAnchorCredentials(
  credentials: WalletCredential[]
): AnchorCredential[] {
  return credentials.filter(cred => 
    cred.type === 'anchor_credential' || 
    (Array.isArray(cred.type) && cred.type.includes('anchor_credential'))
  ) as AnchorCredential[];
}

/**
 * Find all anchor credentials for a specific federation
 * 
 * @param credentials Collection of credentials to search
 * @param federationId ID of the federation
 * @returns All anchor credentials for the specified federation
 */
export function findFederationAnchors(
  credentials: WalletCredential[],
  federationId: string
): AnchorCredential[] {
  return findAnchorCredentials(credentials).filter(anchor => 
    anchor.metadata?.federation?.id === federationId
  );
}

/**
 * Find all anchor credentials of a specific type
 * 
 * @param credentials Collection of credentials to search
 * @param anchorType Type of anchor to find
 * @returns All anchor credentials of the specified type
 */
export function findAnchorsByType(
  credentials: WalletCredential[],
  anchorType: 'epoch' | 'mandate' | 'role_assignment' | 'membership'
): AnchorCredential[] {
  return findAnchorCredentials(credentials).filter(anchor => 
    (anchor as AnchorCredential).anchorType === anchorType
  );
}

/**
 * Find anchor credentials by DAG root hash
 * 
 * @param credentials Collection of credentials to search
 * @param dagRootHash DAG root hash to find
 * @returns All anchor credentials with the specified DAG root hash
 */
export function findAnchorsByDagRoot(
  credentials: WalletCredential[],
  dagRootHash: string
): AnchorCredential[] {
  return findAnchorCredentials(credentials).filter(anchor => 
    anchor.metadata?.dag?.root_hash === dagRootHash
  );
}

/**
 * Create an amendment credential
 * 
 * @param federationId Federation ID
 * @param amendmentId Amendment ID
 * @param amendmentText Amendment text content
 * @param ratifiedInEpoch Epoch ID in which the amendment was ratified
 * @param previousAmendmentId Previous amendment ID (optional)
 * @param options Additional options
 * @returns An amendment anchor credential
 */
export async function createAmendmentCredential(
  federationId: string,
  federationDid: string,
  amendmentId: string,
  amendmentText: string,
  ratifiedInEpoch: string,
  previousAmendmentId?: string,
  options: Partial<AnchorCredentialOptions> = {}
): Promise<AnchorCredential> {
  // Calculate hash of the amendment text
  const textHash = crypto.createHash('sha256')
    .update(amendmentText)
    .digest('hex');
  
  const now = new Date().toISOString();
  
  return createAnchorCredential({
    anchorType: 'amendment',
    federation: {
      id: federationId,
      did: federationDid,
      name: options.federation?.name || `Federation ${federationId}`,
    },
    subjectDid: federationDid,
    privateKey: options.privateKey || '',
    dagRootHash: options.dagRootHash || '',
    effectiveFrom: now,
    amendmentId,
    textHash,
    ratifiedInEpoch,
    previousAmendmentId,
    ...options
  });
}

/**
 * Find all amendment credentials in a collection
 * 
 * @param credentials Collection of credentials to search
 * @returns All amendment credentials found
 */
export function findAmendmentCredentials(
  credentials: WalletCredential[]
): AnchorCredential[] {
  return findAnchorCredentials(credentials).filter(anchor => 
    (anchor as AnchorCredential).anchorType === 'amendment'
  );
}

/**
 * Get the complete amendment history for a federation
 * 
 * @param credentials Collection of credentials
 * @param federationId ID of the federation
 * @returns A sorted array of amendment credentials
 */
export function getAmendmentHistory(
  credentials: WalletCredential[],
  federationId: string
): AnchorCredential[] {
  const amendments = findAmendmentCredentials(credentials)
    .filter(amendment => amendment.metadata?.federation?.id === federationId);
  
  // Sort by timestamp
  return amendments.sort((a, b) => {
    const timestampA = a.credentialSubject.effective_from;
    const timestampB = b.credentialSubject.effective_from;
    return timestampA.localeCompare(timestampB);
  });
} 