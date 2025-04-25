/**
 * Zero-knowledge identity utilities for federation credentials
 */

import { ScopedIdentity, FederationMember } from '../types/identity';
import { createSignature } from './identity';
import { v4 as uuidv4 } from 'uuid';

// Types for ZK identity proofs

/**
 * A ZK identity claim (selective disclosure)
 */
export interface ZkIdentityClaim {
  /** Type of the ZK identity claim */
  type: 'ScopedIdentityClaim' | 'FederationMemberClaim' | 'GuardianClaim';
  
  /** Original DID the claim is about */
  subjectDid: string;
  
  /** Disclosed attributes from the original identity */
  disclosedAttributes: Record<string, any>;
  
  /** Commitment to hidden attributes (hashes) */
  attributeCommitments: Record<string, string>;
  
  /** Nonce used for the commitments */
  nonce: string;
  
  /** Timestamp */
  created: string;
  
  /** ZK proof data */
  proof: {
    /** Type of the proof */
    type: string;
    /** DID of the issuer */
    issuer: string;
    /** Proof value (depends on type) */
    proofValue: string;
    /** Created timestamp */
    created: string;
  };
}

/**
 * Presentation of a ZK identity claim
 */
export interface ZkIdentityPresentation {
  /** ID of the presentation */
  id: string;
  
  /** Type of the presentation */
  type: string[];
  
  /** DID of the holder (presenter) */
  holder: string;
  
  /** The ZK identity claim */
  claim: ZkIdentityClaim;
  
  /** Proof of the presentation */
  proof: {
    /** Type of the proof */
    type: string;
    /** Created timestamp */
    created: string;
    /** Challenge (if domain was provided) */
    challenge?: string;
    /** Domain (if specified) */
    domain?: string;
    /** Verification method ID */
    verificationMethod: string;
    /** Proof purpose */
    proofPurpose: string;
    /** Proof value (usually JWS signature) */
    proofValue: string;
  };
}

/**
 * Options for creating a ZK identity claim
 */
export interface ZkIdentityClaimOptions {
  /** The identity to create a claim from */
  identity: ScopedIdentity | FederationMember;
  
  /** Attributes to disclose (leave empty to decide based on disclosureMode) */
  discloseAttributes?: string[];
  
  /** Attributes to hide (leave empty to decide based on disclosureMode) */
  hideAttributes?: string[];
  
  /** Disclosure mode (e.g., 'minimal', 'standard', 'full') */
  disclosureMode?: 'minimal' | 'standard' | 'full';
  
  /** Private key for signing the claim */
  privateKeyBase64: string;
  
  /** Additional attributes to add to the claim */
  additionalAttributes?: Record<string, any>;
}

/**
 * Default attributes to disclose for each disclosure mode
 */
const DEFAULT_DISCLOSURE_ATTRIBUTES = {
  minimal: ['scope'],
  standard: ['scope', 'username', 'createdAt'],
  full: ['scope', 'username', 'createdAt', 'updatedAt', 'metadata']
};

/**
 * Create a ZK identity claim
 * @param options Options for creating the claim
 * @returns A ZK identity claim
 */
export async function createZkIdentityClaim(
  options: ZkIdentityClaimOptions
): Promise<ZkIdentityClaim> {
  const { identity, privateKeyBase64, additionalAttributes = {} } = options;
  const now = new Date().toISOString();
  
  // Determine which attributes to disclose based on the disclosure mode
  const disclosureMode = options.disclosureMode || 'standard';
  const defaultDisclosures = DEFAULT_DISCLOSURE_ATTRIBUTES[disclosureMode];
  
  // Use explicitly provided attributes if specified, otherwise use defaults
  const attributesToDisclose = options.discloseAttributes || defaultDisclosures;
  
  // Additional attributes to hide explicitly
  const attributesToHide = options.hideAttributes || [];
  
  // Extract all identity attributes as a record with string keys
  const allAttributes: Record<string, any> = {
    did: identity.did,
    scope: identity.scope,
    username: identity.username,
    createdAt: identity.createdAt,
    updatedAt: identity.updatedAt,
    ...identity.metadata,
    ...('federationMembership' in identity) ? {
      federationId: (identity as FederationMember).federationMembership.federationId,
      role: (identity as FederationMember).federationMembership.role.role,
      memberSince: (identity as FederationMember).federationMembership.memberSince,
    } : {},
    ...additionalAttributes
  };
  
  // Create disclosed and hidden attributes
  const disclosedAttributes: Record<string, any> = {};
  const attributeCommitments: Record<string, string> = {};
  
  // Generate a random nonce for commitments
  const nonce = uuidv4();
  
  // Process attributes for disclosure or commitment
  for (const attr of Object.keys(allAttributes)) {
    if (attributesToDisclose.includes(attr) && !attributesToHide.includes(attr)) {
      disclosedAttributes[attr] = allAttributes[attr];
    } else {
      // Create commitment (hash) for hidden attributes
      // In a real implementation, this would use a proper ZK scheme
      const value = JSON.stringify(allAttributes[attr]);
      const commitment = await sha256WithNonce(value, nonce);
      attributeCommitments[attr] = commitment;
    }
  }
  
  // Always disclose the subject DID
  disclosedAttributes.did = identity.did;
  
  // Determine the claim type
  let claimType = 'ScopedIdentityClaim';
  if ('federationMembership' in identity) {
    claimType = 'FederationMemberClaim';
  }
  
  // Create the basic claim without the proof
  const claim: Omit<ZkIdentityClaim, 'proof'> = {
    type: claimType as any,
    subjectDid: identity.did,
    disclosedAttributes,
    attributeCommitments,
    nonce,
    created: now
  };
  
  // Sign the claim to create the proof
  const dataToSign = JSON.stringify(claim);
  const signature = await createSignature(
    { 
      signerDid: identity.did, 
      data: dataToSign,
      signatureType: 'JWS',
      ...'federationMembership' in identity ? { 
        federationId: (identity as FederationMember).federationMembership.federationId 
      } : {}
    },
    privateKeyBase64
  );
  
  // Add the proof to the claim
  return {
    ...claim,
    proof: {
      type: 'JwsProof2020',
      issuer: identity.did,
      proofValue: signature.signature,
      created: signature.created
    }
  };
}

/**
 * Create a presentation of a ZK identity claim
 * @param claim The ZK identity claim
 * @param holderDid The DID of the holder (presenter)
 * @param privateKeyBase64 Private key of the holder for signing
 * @param domain Optional domain for the presentation
 * @param challenge Optional challenge for the presentation
 * @returns A ZK identity presentation
 */
export async function createZkIdentityPresentation(
  claim: ZkIdentityClaim,
  holderDid: string,
  privateKeyBase64: string,
  domain?: string,
  challenge?: string
): Promise<ZkIdentityPresentation> {
  const now = new Date().toISOString();
  const presentationId = `urn:zkp:${uuidv4()}`;
  
  // Create the presentation without proof
  const presentationWithoutProof: Omit<ZkIdentityPresentation, 'proof'> = {
    id: presentationId,
    type: ['VerifiablePresentation', 'ZkIdentityPresentation'],
    holder: holderDid,
    claim
  };
  
  // Sign the presentation
  const dataToSign = JSON.stringify({
    ...presentationWithoutProof,
    ...(challenge ? { challenge } : {}),
    ...(domain ? { domain } : {})
  });
  
  const signature = await createSignature(
    {
      signerDid: holderDid,
      data: dataToSign,
      signatureType: 'JWS'
    },
    privateKeyBase64
  );
  
  // Create the final presentation with proof
  return {
    ...presentationWithoutProof,
    proof: {
      type: 'JwsProof2020',
      created: now,
      ...(challenge ? { challenge } : {}),
      ...(domain ? { domain } : {}),
      verificationMethod: signature.verificationMethod,
      proofPurpose: 'authentication',
      proofValue: signature.signature
    }
  };
}

/**
 * Verify a ZK identity presentation
 * @param presentation The ZK identity presentation to verify
 * @returns Boolean indicating whether the presentation is valid
 */
export async function verifyZkIdentityPresentation(
  presentation: ZkIdentityPresentation
): Promise<boolean> {
  // In a real implementation, this would:
  // 1. Verify the presentation proof (signature)
  // 2. Verify the claim proof
  // 3. Verify the ZK proofs for the hidden attributes
  
  // This is just a placeholder showing the verification flow
  console.log('Verifying ZK identity presentation:', presentation.id);
  console.log('Holder:', presentation.holder);
  console.log('Claim type:', presentation.claim.type);
  console.log('Disclosed attributes:', Object.keys(presentation.claim.disclosedAttributes));
  console.log('Hidden attributes:', Object.keys(presentation.claim.attributeCommitments));
  
  // Return true for now - in a real implementation this would do actual crypto verification
  return true;
}

/**
 * Create a SHA-256 hash with a nonce
 * @param data Data to hash
 * @param nonce Nonce to use
 * @returns Hex-encoded hash
 */
async function sha256WithNonce(data: string, nonce: string): Promise<string> {
  const encoder = new TextEncoder();
  const dataWithNonce = encoder.encode(`${data}:${nonce}`);
  
  // Use the Web Crypto API to create a SHA-256 hash
  const hashBuffer = await crypto.subtle.digest('SHA-256', dataWithNonce);
  
  // Convert the hash to a hex string
  return Array.from(new Uint8Array(hashBuffer))
    .map(b => b.toString(16).padStart(2, '0'))
    .join('');
} 