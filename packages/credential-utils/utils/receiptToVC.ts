import { WalletCredential } from '../types';

// Define the receipt structure from the API
interface Receipt {
  id: string;
  job_id: string;
  proposal_id?: string;
  created_at: string;
  node_id: string;
  signature: string;
  verifications?: number;
  verified_at?: string;
  verified?: boolean;
  execution_hash?: string;
  thread_id?: string;
}

/**
 * Transform a finalization receipt into a W3C Verifiable Credential format
 * @param receipt The runtime API receipt
 * @param userDid The DID of the user who owns this credential
 * @returns A formatted verifiable credential
 */
export function receiptToVC(receipt: Receipt, userDid: string): any {
  // Generate contexts based on receipt type
  const contexts = [
    'https://www.w3.org/2018/credentials/v1',
    'https://identity.foundation/presentation-exchange/submission/v1',
    'https://icn.xyz/contexts/governance/v1'
  ];
  
  // Generate credential type
  const type = receipt.node_id === userDid ? 'FederationFinalizationCredential' : 'FederationVoteCredential';
  
  // Generate VC format
  return {
    '@context': contexts,
    id: `urn:uuid:${receipt.id}`,
    type: ['VerifiableCredential', type],
    issuer: {
      id: receipt.node_id,
      // Federation data could be added here
    },
    issuanceDate: receipt.created_at,
    credentialSubject: {
      id: userDid,
      proposalId: receipt.proposal_id,
      executionHash: receipt.execution_hash,
      jobId: receipt.job_id,
      role: receipt.node_id === userDid ? 'finalizer' : 'voter',
      ...(receipt.thread_id && { threadId: receipt.thread_id }),
    },
    proof: {
      type: 'Ed25519Signature2020',
      created: receipt.created_at,
      verificationMethod: `${receipt.node_id}#keys-1`,
      proofPurpose: 'assertionMethod',
      proofValue: receipt.signature
    },
    ...(receipt.thread_id && { 
      metadata: {
        agoranet: {
          threadId: receipt.thread_id,
          threadUrl: `https://agoranet.icn.zone/threads/${receipt.thread_id}`
        }
      }
    })
  };
}

/**
 * Transform a wallet credential to W3C Verifiable Credential format
 * @param credential The wallet credential to transform
 * @returns A W3C formatted verifiable credential
 */
export function walletCredentialToVC(credential: WalletCredential): any {
  return {
    '@context': [
      'https://www.w3.org/2018/credentials/v1',
      'https://identity.foundation/presentation-exchange/submission/v1',
      'https://icn.xyz/contexts/governance/v1'
    ],
    id: `urn:uuid:${credential.id}`,
    type: ['VerifiableCredential', credential.type],
    issuer: {
      id: credential.issuer.did,
      name: credential.issuer.name,
    },
    issuanceDate: credential.issuanceDate,
    expirationDate: credential.expirationDate,
    credentialSubject: {
      id: credential.subjectDid,
      ...credential.credentialSubject
    },
    proof: credential.proof,
    ...(credential.metadata?.agoranet?.threadId && {
      metadata: {
        agoranet: credential.metadata.agoranet
      }
    })
  };
}

/**
 * Generate a presentation containing multiple credentials
 * @param credentials Array of wallet credentials to include
 * @param holderDid The DID of the holder creating this presentation
 * @returns A verifiable presentation containing the credentials
 */
export function generateVC(credentials: WalletCredential[], holderDid: string): any {
  return {
    '@context': [
      'https://www.w3.org/2018/credentials/v1',
      'https://identity.foundation/presentation-exchange/submission/v1'
    ],
    type: ['VerifiablePresentation'],
    holder: holderDid,
    verifiableCredential: credentials.map(walletCredentialToVC),
    // The presentation proof would be generated and added by the holder's wallet
  };
} 