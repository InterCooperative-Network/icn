/**
 * Core Identity Types for Federation Credential System
 */

import { FederationMemberRole } from './federation';

/**
 * Supported key types in the identity system
 */
export enum KeyType {
  Ed25519 = 'Ed25519',
  Ecdsa = 'Ecdsa',
  Secp256k1 = 'Secp256k1',
}

/**
 * Key material (public/private keypair)
 */
export interface KeyMaterial {
  /** Type of key */
  keyType: KeyType;
  /** Public key encoded as base64 */
  publicKeyBase64: string;
  /** Private key (available only for owned identities) */
  privateKeyBase64?: string;
  /** Key ID within the DID document */
  keyId?: string;
}

/**
 * A scoped identity with federation associations
 */
export interface ScopedIdentity {
  /** Decentralized Identifier (DID) */
  did: string;
  /** Scope (e.g., federation name) */
  scope: string;
  /** Username within the scope */
  username: string;
  /** Key material for this identity */
  keyMaterial: KeyMaterial;
  /** Federation metadata if this is a federation member */
  federationMembership?: {
    /** Federation ID this identity belongs to */
    federationId: string;
    /** Role within the federation */
    role: FederationMemberRole;
    /** When membership was established */
    memberSince: string;
    /** Optional expiration of membership */
    memberUntil?: string;
  };
  /** Creation timestamp */
  createdAt: string;
  /** Last updated timestamp */
  updatedAt: string;
  /** Optional guardian DIDs that can recover this identity */
  guardians?: string[];
  /** Additional metadata */
  metadata?: Record<string, any>;
}

/**
 * Special identity type for federation members
 */
export interface FederationMember extends ScopedIdentity {
  /** Must have federation membership */
  federationMembership: {
    federationId: string;
    role: FederationMemberRole;
    memberSince: string;
    memberUntil?: string;
  };
  /** Additional federation-specific data */
  federationMetadata?: {
    /** Member weight for voting */
    weight: number;
    /** Whether this member can sign credentials */
    canSignCredentials: boolean;
    /** Whether this member can approve amendments */
    canApproveAmendments: boolean;
    /** Can this member initiate recovery for others */
    canInitiateRecovery: boolean;
  };
}

/**
 * Special identity type for guardians
 */
export interface Guardian extends ScopedIdentity {
  /** Identities this guardian can recover */
  protectedIdentities: string[];
  /** Recovery authorization level */
  authorizationLevel: 'full' | 'limited';
  /** Whether this guardian is active */
  isActive: boolean;
}

/**
 * Interface for identity creation parameters
 */
export interface CreateIdentityParams {
  /** Scope for the identity */
  scope: string;
  /** Username within the scope */
  username: string;
  /** Key type to generate */
  keyType: KeyType;
  /** Federation ID if this is a federation member */
  federationId?: string;
  /** Role within the federation */
  role?: string;
  /** Additional metadata */
  metadata?: Record<string, any>;
}

/**
 * Interface for identity signature creation
 */
export interface SignatureParams {
  /** DID of the signer */
  signerDid: string;
  /** Federation ID if signing as federation member */
  federationId?: string;
  /** Data to sign */
  data: Uint8Array | string;
  /** Signature type */
  signatureType?: 'JWS' | 'Ed25519Signature2020' | 'EcdsaSecp256k1Signature2019';
  /** Whether to create a detached signature */
  detached?: boolean;
}

/**
 * Interface for a completed signature
 */
export interface SignatureResult {
  /** DID of the signer */
  signerDid: string;
  /** Signature value (JWS or raw signature) */
  signature: string;
  /** Verification method ID */
  verificationMethod: string;
  /** When the signature was created */
  created: string;
  /** Purpose of the signature */
  proofPurpose: string;
  /** Type of signature */
  signatureType: string;
} 