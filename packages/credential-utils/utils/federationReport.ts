import { 
  VerifiableCredential,
  VerifiablePresentation,
  createVerifiablePresentation,
  signVerifiablePresentation
} from './credentials';
import { KeyPair } from '../types/keys';

/**
 * Federation Report metadata
 */
export interface FederationReportMetadata {
  federationName: string;
  timestamp: number;
  reportId: string;
  reportType: string;
  description?: string;
  version?: string;
}

/**
 * Options for generating a federation report
 */
export interface FederationReportOptions {
  metadata: FederationReportMetadata;
  credentials: VerifiableCredential[];
  federationDid: string;
  keyPair: KeyPair;
  audience?: string; // Optional recipient of the report
  expirationDate?: string; // Optional ISO date string
}

/**
 * Creates a federation-signed report containing multiple credentials
 * bundled as a verifiable presentation
 * 
 * @param options - The options for generating the report
 * @returns A signed verifiable presentation containing all credentials
 */
export async function createFederationReport(
  options: FederationReportOptions
): Promise<VerifiablePresentation> {
  const {
    metadata,
    credentials,
    federationDid,
    keyPair,
    audience,
    expirationDate
  } = options;

  // Create the unsigned presentation
  const unsignedPresentation = createVerifiablePresentation({
    holder: federationDid,
    verifiableCredentials: credentials,
    id: `urn:federation:report:${metadata.reportId}`,
    // Add metadata as additional properties
    additionalProperties: {
      federationName: metadata.federationName,
      reportType: metadata.reportType,
      reportTimestamp: metadata.timestamp,
      description: metadata.description,
      version: metadata.version || '1.0'
    }
  });

  // Sign the presentation
  const signedPresentation = await signVerifiablePresentation(
    unsignedPresentation,
    {
      issuerDid: federationDid,
      privateKey: keyPair.privateKey,
      publicKey: keyPair.publicKey,
      audience,
      expirationDate
    }
  );

  return signedPresentation;
}

/**
 * Extracts the metadata from a federation report
 * 
 * @param report - The federation report to extract metadata from
 * @returns The metadata contained in the report
 */
export function extractFederationReportMetadata(
  report: VerifiablePresentation
): FederationReportMetadata {
  return {
    federationName: report.federationName as string,
    reportId: report.id.split(':').pop() as string,
    reportType: report.reportType as string,
    timestamp: report.reportTimestamp as number,
    description: report.description as string,
    version: report.version as string
  };
}

/**
 * Extracts credentials from a federation report
 * 
 * @param report - The federation report to extract credentials from
 * @returns Array of verifiable credentials contained in the report
 */
export function extractCredentialsFromReport(
  report: VerifiablePresentation
): VerifiableCredential[] {
  return report.verifiableCredential || [];
}

/**
 * Generates a unique report ID
 * 
 * @param prefix - Optional prefix for the report ID
 * @returns A unique report ID
 */
export function generateReportId(prefix: string = 'rep'): string {
  const timestamp = Date.now();
  const randomPart = Math.random().toString(36).substring(2, 10);
  return `${prefix}-${timestamp}-${randomPart}`;
} 