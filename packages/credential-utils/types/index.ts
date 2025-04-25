// Import VerifiableCredential and AmendmentCredential types
import { VerifiableCredential } from './credential';
import { AmendmentCredential } from './amendment';
// Remove duplicate FederationTrust export if needed

// Export all types from the types directory
export * from './credentials';
export * from './federation';
export * from './wallet';

/**
 * Options for creating a selective disclosure from a credential
 */
export interface SelectiveDisclosureOptions {
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
   * The selectively disclosed credential
   */
  credential: WalletCredential;
  
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