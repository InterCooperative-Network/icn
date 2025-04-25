// Wallet credential types
export type CredentialType = 
  | 'proposal' 
  | 'vote' 
  | 'finalization' 
  | 'appeal' 
  | 'appeal_vote' 
  | 'appeal_finalization'
  | 'execution'
  | 'federation_report'
  | 'anchor_credential';

// Verification status for a credential
export type VerificationStatus = 'verified' | 'unverified' | 'invalid';

// Trust level for a credential
export type TrustLevel = 'High' | 'Medium' | 'Low';

// A wallet credential representing a verifiable credential
export interface WalletCredential {
  id: string;
  title: string;
  type: string;
  issuer: {
    did: string;
    name?: string;
    logo?: string;
  };
  subjectDid: string;
  issuanceDate: string;
  expirationDate?: string;
  credentialStatus?: {
    type: string;
    status: string;
  };
  credentialSubject: Record<string, any>;
  proof?: {
    type: string;
    created: string;
    verificationMethod: string;
    proofPurpose: string;
    proofValue?: string;
    jws?: string;
  };
  trustLevel?: string;
  tags?: string[];
  metadata?: {
    icon?: string;
    color?: string;
    description?: string;
    agoranet?: {
      threadId: string;
      threadUrl: string;
      federation_id?: string;
    };
    federation?: {
      id: string;
      name?: string;
      logo?: string;
    };
  };
  receiptHash?: string;
}

export interface VerificationResult {
  valid: boolean;
  status: 'success' | 'warning' | 'error' | 'info';
  message: string;
  details?: any;
}

/**
 * Types of credentials in the system
 */
export enum CredentialType {
  Proposal = 'proposal',
  Vote = 'vote',
  Appeal = 'appeal',
  AppealVote = 'appeal_vote',
  Finalization = 'finalization',
  AppealFinalization = 'appeal_finalization',
  Execution = 'execution',
  FederationReport = 'federation_report',
  AnchorCredential = 'anchor_credential'
}

/**
 * Anchor credential represents a federation mandate, epoch transition, or other
 * important federation milestone that is attested and DAG-anchored
 */
export interface AnchorCredential extends WalletCredential {
  type: CredentialType.AnchorCredential | 'anchor_credential';
  anchorType: 'epoch' | 'mandate' | 'role_assignment' | 'membership';
  
  credentialSubject: {
    id: string;
    epoch_id?: string;
    mandate?: string;
    role?: string;
    effective_from: string;
    effective_until?: string;
    dag_root_hash: string;
    referenced_credentials?: string[];
    [key: string]: any;
  };
  
  metadata: {
    federation: {
      id: string;
      name?: string;
    };
    dag: {
      root_hash: string;
      timestamp: string;
      block_height?: number;
    };
    [key: string]: any;
  };
} 