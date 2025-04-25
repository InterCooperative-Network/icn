import { WalletCredential } from '../types/wallet';
import { FederationManifest } from '../types/federation';
import { filterCredentialsByFederation } from './federation';
import { v4 as uuidv4 } from 'uuid';
import * as jose from 'jose';

/**
 * Options for generating a federation report
 */
export interface FederationReportOptions {
  /**
   * ID of the federation to generate the report for
   */
  federationId: string;
  
  /**
   * Federation manifest containing member information and keys
   */
  federationManifest: FederationManifest;
  
  /**
   * DID of the user to generate the report for
   */
  userDid: string;
  
  /**
   * Private key of the federation (or admin) to sign the report
   */
  privateKey: string;
  
  /**
   * Optional summary statistics to include in the report
   */
  summaryStats?: {
    participationScore?: number;
    totalContributions?: number;
    activeSince?: string;
    roles?: string[];
    [key: string]: any;
  };
  
  /**
   * Optional expiration date for the report
   */
  expirationDate?: string;
  
  /**
   * Optional additional metadata to include
   */
  additionalMetadata?: Record<string, any>;
}

/**
 * A federation-signed report bundling multiple credentials
 */
export interface FederationReport {
  '@context': string[];
  id: string;
  type: string[];
  holder: string;
  verifiableCredential: WalletCredential[];
  issuanceDate: string;
  expirationDate?: string;
  federationMetadata: {
    federation_id: string;
    name: string;
    issuanceDate: string;
    summaryStats?: Record<string, any>;
    [key: string]: any;
  };
  proof: {
    type: string;
    created: string;
    verificationMethod: string;
    proofPurpose: string;
    jws: string;
  };
}

/**
 * Options for generating a federation report with multiple signatures
 */
export interface FederationMultiSignReportOptions extends FederationReportOptions {
  /**
   * Additional member private keys for multi-signature support
   * Each key should be associated with the member's DID
   */
  memberKeys?: Record<string, string>;
  
  /**
   * Minimum number of signatures to collect (defaults to min_approvals from quorum_rules)
   */
  minSignatures?: number;
}

/**
 * Enhanced federation report proof with support for multiple signatures
 */
export interface FederationMultiSignatureProof {
  type: string;
  created: string;
  proofPurpose: string;
  signatures: Array<{
    verificationMethod: string;
    created: string;
    jws: string;
  }>;
}

/**
 * Generate a federation-signed report for a user's credentials within a federation
 * 
 * @param credentials All wallet credentials to filter for this federation
 * @param options Options for generating the report
 * @returns The federation report as a verifiable presentation
 */
export async function generateFederationReport(
  credentials: WalletCredential[],
  options: FederationReportOptions
): Promise<FederationReport> {
  // Filter credentials by federation
  const filteredCredentials = filterCredentialsByFederation(
    credentials,
    options.federationId
  );
  
  if (filteredCredentials.length === 0) {
    throw new Error(`No credentials found for federation ${options.federationId}`);
  }
  
  // Create the unsigned report
  const now = new Date().toISOString();
  const reportId = `urn:uuid:${uuidv4()}`;
  
  const unsignedReport: Omit<FederationReport, 'proof'> = {
    '@context': [
      'https://www.w3.org/2018/credentials/v1',
      'https://identity.foundation/presentation-exchange/submission/v1',
      'https://www.icn.network/context/v1'
    ],
    id: reportId,
    type: ['VerifiablePresentation', 'FederationReport'],
    holder: options.userDid,
    verifiableCredential: filteredCredentials,
    issuanceDate: now,
    federationMetadata: {
      federation_id: options.federationId,
      name: options.federationManifest.name,
      issuanceDate: now,
      totalCredentials: filteredCredentials.length,
      ...options.summaryStats && { summaryStats: options.summaryStats },
      ...options.additionalMetadata
    }
  };
  
  // Add expiration date if provided
  if (options.expirationDate) {
    unsignedReport.expirationDate = options.expirationDate;
  }
  
  // Sign the report using the federation's private key
  const signature = await signFederationReport(
    unsignedReport,
    options.federationManifest,
    options.privateKey
  );
  
  // Return the signed report
  return {
    ...unsignedReport,
    proof: signature
  };
}

/**
 * Signs a federation report using the provided private key
 * 
 * @param report The unsigned federation report
 * @param manifest The federation manifest
 * @param privateKey The private key to sign with
 * @returns The signature proof
 */
async function signFederationReport(
  report: Omit<FederationReport, 'proof'>,
  manifest: FederationManifest,
  privateKey: string
): Promise<FederationReport['proof']> {
  // In a real implementation, this would use the private key to sign the report
  // For this example, we'll create a placeholder signature
  
  // Serialize the report to a string that can be signed
  const reportToSign = JSON.stringify({
    '@context': report['@context'],
    id: report.id,
    type: report.type,
    holder: report.holder,
    federationMetadata: report.federationMetadata,
    credentialIds: report.verifiableCredential.map(vc => vc.id)
  });
  
  // Create a mock signature (in a real implementation, this would use the private key)
  const mockJws = Buffer.from(reportToSign).toString('base64');
  
  // Get the federation DID to use as the verification method
  const federationDid = `did:icn:federation/${manifest.federation_id}`;
  
  return {
    type: 'Ed25519Signature2020',
    created: new Date().toISOString(),
    verificationMethod: `${federationDid}#controller`,
    proofPurpose: 'assertionMethod',
    jws: mockJws
  };
}

/**
 * Verify a federation report signature
 * 
 * @param report The federation report to verify
 * @param publicKey The federation's public key
 * @returns Whether the signature is valid
 */
export async function verifyFederationReport(
  report: FederationReport,
  publicKey: string
): Promise<boolean> {
  // In a real implementation, this would verify the signature using the public key
  // For this example, we'll just return true
  
  // Check that the report has a valid structure
  if (!report.proof || !report.proof.jws) {
    return false;
  }
  
  // In a real implementation, verify the signature against the report
  return true;
}

/**
 * Extract lineage information from a federation report
 * 
 * @param report The federation report to analyze
 * @returns A map of credential IDs to their parent credential IDs
 */
export function extractCredentialLineage(
  report: FederationReport
): Record<string, string[]> {
  const lineage: Record<string, string[]> = {};
  
  // Extract relationships from credentials
  report.verifiableCredential.forEach(credential => {
    // Initialize empty array for this credential
    lineage[credential.id] = [];
    
    // Extract parent IDs from credential subject if they exist
    if (credential.credentialSubject.parentCredentialId) {
      if (Array.isArray(credential.credentialSubject.parentCredentialId)) {
        lineage[credential.id] = credential.credentialSubject.parentCredentialId;
      } else {
        lineage[credential.id] = [credential.credentialSubject.parentCredentialId];
      }
    }
    
    // Extract proposal relationships
    if (credential.credentialSubject.proposalId) {
      // Find other credentials with the same proposal ID
      report.verifiableCredential.forEach(relatedCred => {
        if (relatedCred.id !== credential.id && 
            relatedCred.credentialSubject.proposalId === credential.credentialSubject.proposalId) {
          // Add as related, but not necessarily a parent
          if (!lineage[credential.id].includes(relatedCred.id)) {
            if (isParentCredential(relatedCred, credential)) {
              lineage[credential.id].push(relatedCred.id);
            }
          }
        }
      });
    }
  });
  
  return lineage;
}

/**
 * Determines if one credential is a parent of another based on type and timestamps
 * 
 * @param potentialParent The potential parent credential
 * @param child The child credential
 * @returns Whether the potential parent is actually a parent
 */
function isParentCredential(
  potentialParent: WalletCredential,
  child: WalletCredential
): boolean {
  // Check issuance dates - parent must be issued before child
  if (new Date(potentialParent.issuanceDate) >= new Date(child.issuanceDate)) {
    return false;
  }
  
  // Check relationships based on credential types
  // For example, a proposal credential is parent to vote credentials
  // A vote credential is parent to finalization credentials
  const typeHierarchy: Record<string, string[]> = {
    'proposal': ['vote', 'appeal'],
    'vote': ['finalization'],
    'appeal': ['appeal_vote'],
    'appeal_vote': ['appeal_finalization'],
    'finalization': ['execution'],
    'appeal_finalization': ['execution']
  };
  
  // Check if the child type is in the hierarchy of the potential parent
  return typeHierarchy[potentialParent.type]?.includes(child.type) || false;
}

/**
 * Represents a federation manifest containing identity and signing information
 */
export interface FederationManifest {
  id: string;
  name: string;
  description?: string;
  did: string;
  publicKey: string;
  url?: string;
  logo?: string;
}

/**
 * Summary statistics for a credential collection
 */
export interface CredentialStats {
  totalCredentials: number;
  credentialsByType: Record<string, number>;
  credentialsByIssuer: Record<string, number>;
  oldestCredential: string;
  newestCredential: string;
  uniqueProposals: number;
  uniqueThreads: number;
}

/**
 * A signed credential report from a federation
 */
export interface FederationReport {
  id: string;
  federationId: string;
  federationName: string;
  federationDid: string;
  userDid: string;
  createdAt: string;
  expiresAt?: string;
  credentials: WalletCredential[];
  stats?: CredentialStats;
  proof: {
    type: string;
    created: string;
    verificationMethod: string;
    jws: string;
  };
}

/**
 * Extract the lineage relationships between credentials
 * @param credentials Collection of credentials to analyze
 * @returns A map of credential IDs to their parent credential IDs
 */
export function extractCredentialLineage(
  credentials: WalletCredential[]
): Record<string, string[]> {
  const lineage: Record<string, string[]> = {};
  
  // Initialize empty arrays for all credentials
  credentials.forEach(cred => {
    lineage[cred.id] = [];
  });
  
  // Process explicit parent references
  credentials.forEach(cred => {
    if (cred.credentialSubject.parentCredentialId) {
      if (Array.isArray(cred.credentialSubject.parentCredentialId)) {
        lineage[cred.id] = [...cred.credentialSubject.parentCredentialId];
      } else {
        lineage[cred.id] = [cred.credentialSubject.parentCredentialId];
      }
    }
  });
  
  // Infer relationships based on proposal and credential type
  credentials.forEach(cred => {
    if (cred.credentialSubject.proposalId) {
      credentials.forEach(potentialParent => {
        if (potentialParent.id !== cred.id &&
            potentialParent.credentialSubject.proposalId === cred.credentialSubject.proposalId &&
            isParentType(potentialParent.type, cred.type) &&
            new Date(potentialParent.issuanceDate) < new Date(cred.issuanceDate)) {
          if (!lineage[cred.id].includes(potentialParent.id)) {
            lineage[cred.id].push(potentialParent.id);
          }
        }
      });
    }
  });
  
  return lineage;
}

/**
 * Determine if one credential type is a parent of another based on the governance flow
 */
function isParentType(parentType: string, childType: string): boolean {
  const hierarchy: Record<string, string[]> = {
    'proposal': ['vote', 'appeal'],
    'vote': ['finalization'],
    'appeal': ['appeal_vote'],
    'appeal_vote': ['appeal_finalization'],
    'finalization': ['execution'],
    'appeal_finalization': ['execution']
  };
  
  return hierarchy[parentType]?.includes(childType) || false;
}

/**
 * Generate summary statistics for a collection of credentials
 * @param credentials Collection of credentials to analyze
 * @returns Statistical summary of the credentials
 */
export function generateSummaryStats(credentials: WalletCredential[]): CredentialStats {
  // Count credentials by type
  const credentialsByType: Record<string, number> = {};
  credentials.forEach(cred => {
    credentialsByType[cred.type] = (credentialsByType[cred.type] || 0) + 1;
  });
  
  // Count credentials by issuer
  const credentialsByIssuer: Record<string, number> = {};
  credentials.forEach(cred => {
    const issuerKey = cred.issuer.did;
    credentialsByIssuer[issuerKey] = (credentialsByIssuer[issuerKey] || 0) + 1;
  });
  
  // Find date range
  let oldestDate = new Date();
  let newestDate = new Date(0);
  
  credentials.forEach(cred => {
    const issueDate = new Date(cred.issuanceDate);
    if (issueDate < oldestDate) {
      oldestDate = issueDate;
    }
    if (issueDate > newestDate) {
      newestDate = issueDate;
    }
  });
  
  // Count unique proposals and threads
  const uniqueProposals = new Set<string>();
  const uniqueThreads = new Set<string>();
  
  credentials.forEach(cred => {
    if (cred.credentialSubject.proposalId) {
      uniqueProposals.add(cred.credentialSubject.proposalId);
    }
    
    if (cred.metadata?.agoranet?.threadId) {
      uniqueThreads.add(cred.metadata.agoranet.threadId);
    }
  });
  
  return {
    totalCredentials: credentials.length,
    credentialsByType,
    credentialsByIssuer,
    oldestCredential: oldestDate.toISOString(),
    newestCredential: newestDate.toISOString(),
    uniqueProposals: uniqueProposals.size,
    uniqueThreads: uniqueThreads.size
  };
}

/**
 * Generate a federation-signed report containing a bundle of credentials
 * @param options Options for the report generation
 * @returns A signed federation report
 */
export async function generateFederationReport(
  options: FederationReportOptions
): Promise<FederationReport> {
  const {
    federationManifest,
    userDid,
    credentials,
    privateKey,
    includeStats = false,
    expiresInDays = 30
  } = options;
  
  const now = new Date();
  const expiresAt = expiresInDays ? new Date(now.getTime() + expiresInDays * 24 * 60 * 60 * 1000) : undefined;
  
  // Create report payload
  const reportPayload = {
    id: `urn:federation:report:${uuidv4()}`,
    federationId: federationManifest.id,
    federationName: federationManifest.name,
    federationDid: federationManifest.did,
    userDid,
    createdAt: now.toISOString(),
    expiresAt: expiresAt?.toISOString(),
    credentials,
    stats: includeStats ? generateSummaryStats(credentials) : undefined
  };
  
  // Import private key for signing
  const privateKeyObj = await jose.importPKCS8(privateKey, 'ES256');
  
  // Sign the report
  const jws = await new jose.CompactSign(new TextEncoder().encode(JSON.stringify(reportPayload)))
    .setProtectedHeader({
      alg: 'ES256',
      kid: `${federationManifest.did}#keys-1`,
      typ: 'application/federation-report+jwt'
    })
    .sign(privateKeyObj);
  
  // Construct final report with proof
  const report: FederationReport = {
    ...reportPayload,
    proof: {
      type: 'EcdsaSecp256k1Signature2019',
      created: now.toISOString(),
      verificationMethod: `${federationManifest.did}#keys-1`,
      jws
    }
  };
  
  return report;
}

/**
 * Verify a federation report's signature
 * @param report The federation report to verify
 * @param publicKey The federation's public key
 * @returns Whether the report's signature is valid
 */
export async function verifyFederationReport(
  report: FederationReport,
  publicKey: string
): Promise<boolean> {
  try {
    // Create verification payload (everything except the proof)
    const { proof, ...reportPayload } = report;
    
    // Import public key for verification
    const publicKeyObj = await jose.importSPKI(publicKey, 'ES256');
    
    // Verify the JWS
    const { payload } = await jose.compactVerify(
      proof.jws,
      publicKeyObj
    );
    
    // Compare the payload with our report
    const decodedPayload = JSON.parse(new TextDecoder().decode(payload));
    
    // Check expiration
    if (report.expiresAt && new Date(report.expiresAt) < new Date()) {
      return false;
    }
    
    // Simple comparison of IDs is sufficient for basic validation
    return decodedPayload.id === report.id;
  } catch (error) {
    console.error('Error verifying federation report:', error);
    return false;
  }
}

/**
 * Export a federation report to a file
 * @param report The federation report to export
 * @param format The export format ('json' | 'pdf')
 * @returns The report data as a string or Buffer
 */
export function exportFederationReport(
  report: FederationReport,
  format: 'json' | 'pdf' = 'json'
): string | Buffer {
  if (format === 'json') {
    return JSON.stringify(report, null, 2);
  } else if (format === 'pdf') {
    // PDF generation would require a PDF library
    // This is a placeholder for future implementation
    throw new Error('PDF export not yet implemented');
  } else {
    throw new Error(`Unsupported export format: ${format}`);
  }
}

/**
 * Import a federation report from a file
 * @param data The report data as a string
 * @returns The parsed federation report
 */
export function importFederationReport(data: string): FederationReport {
  try {
    return JSON.parse(data) as FederationReport;
  } catch (error) {
    throw new Error(`Failed to parse federation report: ${error}`);
  }
}

/**
 * Generate a federation-signed report with multiple signatures
 * This is an enhanced version of generateFederationReport that supports quorum-based signatures
 * 
 * @param credentials All wallet credentials to filter for this federation
 * @param options Options for generating the report with multiple signatures
 * @returns The federation report as a verifiable presentation with multiple signatures
 */
export async function generateMultiSignFederationReport(
  credentials: WalletCredential[],
  options: FederationMultiSignReportOptions
): Promise<FederationReport> {
  // Filter credentials by federation
  const filteredCredentials = filterCredentialsByFederation(
    credentials,
    options.federationId
  );
  
  if (filteredCredentials.length === 0) {
    throw new Error(`No credentials found for federation ${options.federationId}`);
  }
  
  // Create the unsigned report
  const now = new Date().toISOString();
  const reportId = `urn:uuid:${uuidv4()}`;
  
  const unsignedReport: Omit<FederationReport, 'proof'> = {
    '@context': [
      'https://www.w3.org/2018/credentials/v1',
      'https://identity.foundation/presentation-exchange/submission/v1',
      'https://www.icn.network/context/v1'
    ],
    id: reportId,
    type: ['VerifiablePresentation', 'FederationReport', 'MultiSignedCredential'],
    holder: options.userDid,
    verifiableCredential: filteredCredentials,
    issuanceDate: now,
    federationMetadata: {
      federation_id: options.federationId,
      name: options.federationManifest.name,
      issuanceDate: now,
      totalCredentials: filteredCredentials.length,
      quorum_policy: options.federationManifest.quorum_rules.policy_type,
      ...options.summaryStats && { summaryStats: options.summaryStats },
      ...options.additionalMetadata
    }
  };
  
  // Add expiration date if provided
  if (options.expirationDate) {
    unsignedReport.expirationDate = options.expirationDate;
  }
  
  // Sign the report with multiple signatures
  const multiSignature = await createMultiSignatureProof(
    unsignedReport,
    options.federationManifest,
    options.privateKey,
    options.memberKeys || {},
    options.minSignatures
  );
  
  // Create the final report with multi-signature proof
  const finalReport: FederationReport = {
    ...unsignedReport,
    // Add the standard proof field for backward compatibility
    proof: {
      type: 'Ed25519Signature2020',
      created: now,
      verificationMethod: `did:icn:federation/${options.federationId}#controller`,
      proofPurpose: 'assertionMethod',
      jws: multiSignature.signatures[0].jws
    }
  };
  
  // Add the multi-signature proof as an extension
  (finalReport as any).multiSignatureProof = multiSignature;
  
  return finalReport;
}

/**
 * Create a multi-signature proof for a federation report
 * 
 * @param report The unsigned report
 * @param manifest The federation manifest
 * @param primaryKey The primary private key (usually the federation admin)
 * @param memberKeys Additional member private keys
 * @param minSignatures Minimum number of signatures to collect
 * @returns A multi-signature proof
 */
async function createMultiSignatureProof(
  report: Omit<FederationReport, 'proof'>,
  manifest: FederationManifest,
  primaryKey: string,
  memberKeys: Record<string, string> = {},
  minSignatures?: number
): Promise<FederationMultiSignatureProof> {
  // Determine the minimum signatures required
  const requiredSignatures = minSignatures || manifest.quorum_rules.min_approvals || 1;
  
  // Serialize the report to a string that can be signed
  const dataToSign = JSON.stringify({
    '@context': report['@context'],
    id: report.id,
    type: report.type,
    holder: report.holder,
    federationMetadata: report.federationMetadata,
    credentialIds: report.verifiableCredential.map(vc => vc.id)
  });
  
  // Create a list of all available keys
  const allKeys: Record<string, string> = {
    // Add the primary key (federation admin)
    [`did:icn:federation/${manifest.federation_id}`]: primaryKey,
    // Add all member keys
    ...memberKeys
  };
  
  // Create signatures for each available key
  const signatures: Array<{
    verificationMethod: string;
    created: string;
    jws: string;
  }> = [];
  
  // Sign with primary key first
  const adminDid = `did:icn:federation/${manifest.federation_id}`;
  signatures.push({
    verificationMethod: `${adminDid}#controller`,
    created: new Date().toISOString(),
    jws: createMockJws(dataToSign, adminDid)
  });
  
  // Sign with member keys
  for (const [memberDid, privateKey] of Object.entries(memberKeys)) {
    // Skip if not a valid federation member
    if (!manifest.members[memberDid]) {
      console.warn(`Skipping signature for non-member: ${memberDid}`);
      continue;
    }
    
    signatures.push({
      verificationMethod: `${memberDid}#keys-1`,
      created: new Date().toISOString(),
      jws: createMockJws(dataToSign, memberDid)
    });
    
    // Stop if we have enough signatures
    if (signatures.length >= requiredSignatures) {
      break;
    }
  }
  
  // Check if we have enough signatures
  if (signatures.length < requiredSignatures) {
    console.warn(`Warning: Could only collect ${signatures.length} signatures, but ${requiredSignatures} were requested`);
  }
  
  // Create and return the multi-signature proof
  return {
    type: 'Ed25519MultisignatureQuorum2023',
    created: new Date().toISOString(),
    proofPurpose: 'assertionMethod',
    signatures
  };
}

/**
 * Create a mock JWS signature
 * In a real implementation, this would use the private key to generate a proper JWS
 * 
 * @param data Data to sign
 * @param signer DID of the signer
 * @returns Mock JWS signature
 */
function createMockJws(data: string, signer: string): string {
  // In a real implementation, this would create a proper JWS
  // For this example, we'll create a mock JWS with a header that includes the signer
  
  // Create header
  const header = {
    alg: 'EdDSA',
    typ: 'JWT',
    kid: `${signer}#keys-1`
  };
  
  // Base64 encode parts
  const encodedHeader = Buffer.from(JSON.stringify(header)).toString('base64');
  const encodedPayload = Buffer.from(data).toString('base64');
  const mockSignature = Buffer.from(`sig-${signer}-${Date.now()}`).toString('base64');
  
  // Create JWS format: header.payload.signature
  return `${encodedHeader}.${encodedPayload}.${mockSignature}`;
} 