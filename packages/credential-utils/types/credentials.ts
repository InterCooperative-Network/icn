// Wallet credential types
export type CredentialType = 
  | 'proposal' 
  | 'vote' 
  | 'finalization' 
  | 'appeal' 
  | 'appeal_vote' 
  | 'appeal_finalization'
  | 'execution';

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
  };
}

export interface VerificationResult {
  valid: boolean;
  status: 'success' | 'warning' | 'error' | 'info';
  message: string;
  details?: any;
} 