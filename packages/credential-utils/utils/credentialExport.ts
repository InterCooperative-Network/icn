import { WalletCredential } from '../types';

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
    ...(credential.proof && { proof: credential.proof })
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