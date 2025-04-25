// Wallet-related types

export interface UserWallet {
  did: string;
  displayName: string;
  balances: {
    [key: string]: number;
  };
  credentials: WalletCredential[];
}

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
  federationTrust?: FederationTrust;
}

export interface FederationTrust {
  score: number;
  status: string;
  breakdown: {
    valid_signature: boolean;
    registered_member: boolean;
    quorum_threshold_met: boolean;
    sufficient_signer_weight: boolean;
    federation_health: number;
  };
  summary: string;
  details: string[];
}

export interface IssuerInfo {
  did: string;
  name?: string;
  logo?: string;
  website?: string;
  federation?: string;
  verified: boolean;
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