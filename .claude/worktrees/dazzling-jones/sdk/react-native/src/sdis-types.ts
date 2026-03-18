/**
 * SDIS Types for React Native SDK
 *
 * Types for Sovereign Digital Identity System operations.
 */

/**
 * Type of proof to generate or verify
 */
export interface ProofType {
  /** The proof category */
  type: 'age' | 'citizenship' | 'membership' | 'non_revocation' | 'custom';
  /** Age threshold for age proofs (e.g., 18, 21) */
  threshold?: number;
  /** ISO 3166-1 alpha-2 country code for citizenship proofs */
  country_code?: string;
  /** Organization DID for membership proofs */
  org_did?: string;
  /** Custom circuit ID for custom proofs */
  circuit_id?: string;
}

/**
 * Available channels for Level 2+ verification
 */
export type UpgradeChannel = 'nfc' | 'ble' | 'wifi' | 'http';

/**
 * Request to generate an ephemeral proof
 */
export interface GenerateProofRequest {
  /** Type of proof to generate */
  proof_type: ProofType;
  /** Validity duration in seconds (default: 3600, max: 86400) */
  validity_secs?: number;
  /** Available upgrade channels for Level 2+ verification */
  channels?: UpgradeChannel[];
}

/**
 * Generated ephemeral proof
 */
export interface EphemeralProof {
  /** Base64-encoded QR data (137 bytes binary) */
  qr_data: string;
  /** Unix timestamp when proof expires */
  expires_at: number;
  /** Session ID for binding retrieval */
  session_id: string;
  /** QR code size estimate */
  qr_estimate: QrEstimate;
}

/**
 * QR code size estimate
 */
export interface QrEstimate {
  /** Raw data size in bytes */
  data_size: number;
  /** QR version required (1-40) */
  qr_version: number;
  /** QR modules (pixels per side) */
  modules: number;
}

/**
 * Verification levels
 * - Level 1: QR scan only (fast, offline, classical crypto)
 * - Level 2: With binding (NFC/BLE, hybrid signature)
 * - Level 3: Full STARK proof (network required)
 */
export type VerificationLevel = 1 | 2 | 3;

/**
 * Result of proof verification
 */
export interface VerifyResult {
  /** Whether verification passed */
  valid: boolean;
  /** Verification level achieved */
  level: VerificationLevel;
  /** Type of proof that was verified (if valid) */
  proof_type?: string;
  /** Warnings or additional info */
  warnings: string[];
  /** Unix timestamp of verification */
  verified_at: number;
}

/**
 * Request for Level 1 verification
 */
export interface VerifyLevel1Request {
  /** Base64-encoded QR data */
  qr_data: string;
}

/**
 * Request for Level 2 verification
 */
export interface VerifyLevel2Request {
  /** Base64-encoded QR data */
  qr_data: string;
  /** Base64-encoded binding data (optional if cached on server) */
  binding?: string;
}

/**
 * SDIS health status
 */
export interface SdisHealth {
  status: 'ok' | 'degraded' | 'down';
  service: string;
  version: string;
}

/**
 * Local verification history entry
 */
export interface VerificationHistoryEntry {
  /** Unique ID for this entry */
  id: string;
  /** Verification result */
  result: VerifyResult;
  /** Proof type that was verified */
  proof_type: ProofType;
  /** Timestamp of verification */
  timestamp: number;
  /** Level used for verification */
  level: VerificationLevel;
  /** Optional notes */
  notes?: string;
}

/**
 * Local proof generation history entry
 */
export interface ProofHistoryEntry {
  /** Unique ID for this entry */
  id: string;
  /** Proof type that was generated */
  proof_type: ProofType;
  /** Timestamp of generation */
  timestamp: number;
  /** When the proof expired/expires */
  expires_at: number;
}

/**
 * Helper to create age proof type
 */
export function ageProof(threshold: number): ProofType {
  return { type: 'age', threshold };
}

/**
 * Helper to create citizenship proof type
 */
export function citizenshipProof(countryCode: string): ProofType {
  return { type: 'citizenship', country_code: countryCode };
}

/**
 * Helper to create membership proof type
 */
export function membershipProof(orgDid: string): ProofType {
  return { type: 'membership', org_did: orgDid };
}

/**
 * Helper to create non-revocation proof type
 */
export function nonRevocationProof(): ProofType {
  return { type: 'non_revocation' };
}

/**
 * Format proof type for display
 */
export function formatProofType(proofType: ProofType): string {
  switch (proofType.type) {
    case 'age':
      return `Age ${proofType.threshold}+`;
    case 'citizenship':
      return `Citizenship: ${proofType.country_code}`;
    case 'membership':
      return `Membership`;
    case 'non_revocation':
      return `Not Revoked`;
    case 'custom':
      return `Custom Proof`;
    default:
      return 'Unknown';
  }
}

/**
 * Common proof type presets
 */
export const PROOF_PRESETS = {
  AGE_18: ageProof(18),
  AGE_21: ageProof(21),
  AGE_25: ageProof(25),
  AGE_65: ageProof(65),
  CITIZENSHIP_US: citizenshipProof('US'),
  CITIZENSHIP_CA: citizenshipProof('CA'),
  CITIZENSHIP_UK: citizenshipProof('GB'),
  NON_REVOCATION: nonRevocationProof(),
} as const;
