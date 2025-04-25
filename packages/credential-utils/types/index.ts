// Import VerifiableCredential and AmendmentCredential types
import { VerifiableCredential } from './credential';
import { AmendmentCredential } from './amendment';
// Remove duplicate FederationTrust export if needed

// Export all types from the types directory
export * from './credentials';
export * from './federation';
export * from './wallet';
export * from './identity';

/**
 * Options for creating a selective disclosure from a credential
 */
export interface SelectiveDisclosureOptions {
  /**
   * ID of the credential to create a selective disclosure from
   */
  credentialId: string;
  
  /**
   * Fields to include in the disclosure. If empty, all fields are included.
   * Supports dot notation for nested fields (e.g., 'credentialSubject.role')
   */
  includeFields?: string[];
  
  /**
   * Fields to explicitly exclude from the disclosure.
   * Takes precedence over includeFields.
   */
  excludeFields?: string[];
  
  /**
   * Type of disclosure mechanism
   * - 'redaction': Simple field removal
   * - 'zk': Zero-knowledge proofs (if supported)
   */
  proofType: 'redaction' | 'zk';
  
  /**
   * Optional reason for creating this selective disclosure
   */
  reason?: string;
  
  /**
   * DID of the holder presenting this disclosure
   */
  holderId?: string;
}

/**
 * Represents a selective disclosure derived from a credential
 */
export interface SelectiveDisclosure {
  /**
   * Original credential ID this disclosure is derived from
   */
  originalCredentialId: string;
  
  /**
   * The selectively disclosed credential (uses the imported type to avoid conflicts)
   */
  credential: any; // Using any to avoid circular references
  
  /**
   * Fields that were disclosed
   */
  disclosedFields: string[];
  
  /**
   * Fields that were redacted/hidden
   */
  redactedFields: string[];
  
  /**
   * Type of disclosure mechanism used
   */
  proofType: 'redaction' | 'zk';
  
  /**
   * Information about the disclosure
   */
  metadata: {
    createdAt: string;
    reason?: string;
    expiresAt?: string;
  };
}

/**
 * Restorative Action Credential
 * Used to represent completion of harm accountability processes
 */
export interface RestorativeActionCredential extends Omit<VerifiableCredential, 'type'> {
  type: ['VerifiableCredential', 'RestorativeActionCredential'];
  credentialSubject: {
    id: string;
    incident_id?: string;
    participant_did: string;
    restorative_steps?: string[];
    resolution_summary: string;
    status: 'proposed' | 'in_progress' | 'complete' | 'incomplete';
    guardian_circle?: {
      id: string;
      name: string;
      members: string[];
    };
  };
  metadata?: {
    federation?: {
      id: string;
      name: string;
    };
    agoranet?: {
      threadId: string;
      threadUrl: string;
    };
  };
}

/**
 * Union type for all credential types supported by the wallet
 */
export type WalletCredential = 
  | VerifiableCredential 
  | AmendmentCredential 
  | RestorativeActionCredential; 

// Re-export shared types for convenience
import { WalletCredential } from './wallet';
import { FederationManifest, TrustScoreResult } from './federation';
import { ScopedIdentity, FederationMember, Guardian, KeyMaterial, KeyType } from './identity';

/**
 * Federation information object
 */
export interface FederationInfo {
  /** Federation ID */
  id: string;
  /** Federation name */
  name: string;
  /** Federation DID */
  did: string;
}

/**
 * Options for creating an anchor credential
 */
export interface AnchorCredentialOptions {
  /** Type of anchor (amendment, epoch, etc.) */
  anchorType: string;
  /** Federation information */
  federation: FederationInfo;
  /** Subject DID */
  subjectDid: string;
  /** DAG root hash */
  dagRootHash: string;
  /** When this anchor becomes effective */
  effectiveFrom: string;
  /** When this anchor expires (optional) */
  effectiveUntil?: string;
  /** Referenced credentials */
  referencedCredentials: string[];
  /** Amendment ID (for amendment anchors) */
  amendmentId?: string;
  /** Previous amendment ID (for amendment chains) */
  previousAmendmentId?: string;
  /** Text hash of amendment document */
  textHash?: string;
  /** Epoch this was ratified in */
  ratifiedInEpoch?: string;
  /** Human-readable description */
  description?: string;
  /** DID of the signer */
  signerDid?: string;
}

/**
 * Options for exporting credentials
 */
export interface CredentialExportOptions {
  /** Export format */
  format: 'json' | 'vc' | 'qr' | 'selective';
  /** Whether to include metadata */
  includeMetadata?: boolean;
  /** Destination path */
  destination?: string;
  /** QR code options */
  qrOptions?: {
    /** QR code format */
    format: 'svg' | 'png' | 'terminal';
    /** QR code size */
    size?: number;
    /** QR code color */
    color?: string;
  };
  /** Selective disclosure options */
  selectiveDisclosure?: {
    /** Fields to include */
    includeFields?: string[];
    /** Fields to exclude */
    excludeFields?: string[];
    /** Proof type */
    proofType: 'redaction' | 'zk';
    /** Reason */
    reason?: string;
  };
}

export interface VerifiableCredential {
  '@context': string[];
  id: string;
  type: string[];
  issuer: string | { id: string; [key: string]: any };
  issuanceDate: string;
  expirationDate?: string;
  credentialSubject: {
    id: string;
    [key: string]: any;
  };
  proof?: {
    type: string;
    created: string;
    verificationMethod: string;
    proofPurpose: string;
    proofValue?: string;
    jws?: string;
    [key: string]: any;
  };
} 