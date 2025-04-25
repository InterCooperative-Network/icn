// Main index file for the credential-utils package
// Export all types and utils

// Re-export types
export * from './types';

// Re-export utilities
export * from './utils';
export * from './utils/receiptToVC';
export * from './utils/credentialExport';
export * from './utils/selectiveDisclosure';
export * from './utils/proposalLinking';
export * from './utils/groupByAnchor';

// Export types
export * from './types/credentials';
export * from './types/federation';
export * from './types/AnchorCredential';

// Export utilities
export * from './utils/anchorCredential';
export * from './utils/selectiveDisclosure';
export * from './utils/zkDisclosure';
// export * from './utils/verificationUtils'; // Commented out as module doesn't exist yet

// Export custom types
export interface ZKProofOptions {
  fields: string[];
  proofType: 'hash' | 'bulletproofs' | 'groth16';
  validityPeriod?: number;
  reason?: string;
} 