/**
 * SDIS QR Code Utilities
 *
 * Utilities for encoding and decoding SDIS ephemeral proofs in QR codes.
 *
 * QR Format: `icn://sdis?v=1&d=<base64-data>`
 *
 * The data is a 137-byte binary blob containing:
 * - Magic bytes "IC" (2 bytes)
 * - Version (1 byte)
 * - Proof type (1 byte)
 * - Truncated anchor (16 bytes)
 * - Ephemeral public key (32 bytes)
 * - Relative expiry (4 bytes)
 * - Nonce (16 bytes)
 * - Signature (64 bytes)
 * - Channels bitmap (1 byte)
 */

import { EphemeralProof, ProofType } from './sdis-types';

/** Prefix for SDIS QR codes */
const SDIS_QR_PREFIX = 'icn://sdis';

/** Current QR version */
const QR_VERSION = '1';

/**
 * Parsed SDIS QR data
 */
export interface ParsedSdisQR {
  /** Raw base64-encoded binary data */
  raw: string;
  /** QR version */
  version: string;
  /** Decoded proof info (if possible) */
  proofInfo?: {
    /** Proof type byte */
    proofTypeByte: number;
    /** Human-readable proof type */
    proofTypeLabel: string;
    /** Expiry timestamp (approximate) */
    expiresAt?: number;
  };
}

/**
 * Generate QR code data string for an SDIS proof
 *
 * @param proof - Ephemeral proof from the server
 * @returns String suitable for QR code generation
 *
 * @example
 * ```typescript
 * const proof = await client.generateEphemeralProof({ proof_type: { type: 'age', threshold: 21 } });
 * const qrData = generateSdisQR(proof);
 * // Use with QR library: <QRCode value={qrData} />
 * ```
 */
export function generateSdisQR(proof: EphemeralProof): string {
  const params = new URLSearchParams();
  params.set('v', QR_VERSION);
  params.set('d', proof.qr_data);

  return `${SDIS_QR_PREFIX}?${params.toString()}`;
}

/**
 * Parse QR code data string into SDIS proof data
 *
 * @param qrData - Scanned QR code string
 * @returns Parsed data or null if not an SDIS QR
 *
 * @example
 * ```typescript
 * const scanned = 'icn://sdis?v=1&d=SUM...';
 * const parsed = parseSdisQR(scanned);
 * if (parsed) {
 *   const result = await client.verifyLevel1(parsed.raw);
 * }
 * ```
 */
export function parseSdisQR(qrData: string): ParsedSdisQR | null {
  try {
    // Check prefix
    if (!qrData.startsWith(SDIS_QR_PREFIX)) {
      return null;
    }

    // Parse URL parameters
    const url = new URL(qrData);
    const params = url.searchParams;

    // Get version
    const version = params.get('v') || QR_VERSION;

    // Get raw data
    const raw = params.get('d');
    if (!raw) {
      return null;
    }

    // Try to decode proof info for display
    const proofInfo = decodeProofInfo(raw);

    return {
      raw,
      version,
      proofInfo,
    };
  } catch (error) {
    console.warn('Failed to parse SDIS QR code:', error);
    return null;
  }
}

/**
 * Check if a string is an SDIS QR code
 */
export function isSdisQR(qrData: string): boolean {
  return qrData.startsWith(SDIS_QR_PREFIX);
}

/**
 * Check if a string is any ICN QR code (payment or SDIS)
 */
export function isIcnQR(qrData: string): boolean {
  return qrData.startsWith('icn://');
}

/**
 * Determine the type of ICN QR code
 */
export function getIcnQRType(qrData: string): 'sdis' | 'payment' | 'unknown' {
  if (qrData.startsWith('icn://sdis')) {
    return 'sdis';
  }
  if (qrData.startsWith('icn://pay')) {
    return 'payment';
  }
  return 'unknown';
}

/**
 * Decode proof info from raw base64 data
 *
 * @param base64Data - Base64-encoded binary proof
 * @returns Decoded info or undefined
 */
function decodeProofInfo(base64Data: string): ParsedSdisQR['proofInfo'] | undefined {
  try {
    // Decode base64 to bytes
    const binaryString = atob(base64Data);
    const bytes = new Uint8Array(binaryString.length);
    for (let i = 0; i < binaryString.length; i++) {
      bytes[i] = binaryString.charCodeAt(i);
    }

    // Check minimum length (137 bytes)
    if (bytes.length < 137) {
      return undefined;
    }

    // Check magic bytes "IC"
    if (bytes[0] !== 0x49 || bytes[1] !== 0x43) {
      return undefined;
    }

    // Proof type is at byte 3
    const proofTypeByte = bytes[3];
    const proofTypeLabel = proofTypeByteToLabel(proofTypeByte);

    // Relative expiry is at bytes 52-55 (little-endian)
    const relativeExpiry =
      bytes[52] | (bytes[53] << 8) | (bytes[54] << 16) | (bytes[55] << 24);

    // EPOCH_2024 = 1704067200
    const EPOCH_2024 = 1704067200;
    const expiresAt = EPOCH_2024 + relativeExpiry;

    return {
      proofTypeByte,
      proofTypeLabel,
      expiresAt,
    };
  } catch {
    return undefined;
  }
}

/**
 * Convert proof type byte to human-readable label
 */
function proofTypeByteToLabel(byte: number): string {
  if (byte >= 0x00 && byte <= 0x7f) {
    return `Age ${byte}+`;
  }
  switch (byte) {
    case 0x80:
      return 'Citizenship';
    case 0x81:
      return 'Membership';
    case 0x82:
      return 'Not Revoked';
    case 0xff:
      return 'Custom';
    default:
      return 'Unknown';
  }
}

/**
 * Calculate time remaining until proof expires
 *
 * @param expiresAt - Unix timestamp
 * @returns Formatted time string or 'Expired'
 */
export function formatTimeRemaining(expiresAt: number): string {
  const now = Math.floor(Date.now() / 1000);
  const remaining = expiresAt - now;

  if (remaining <= 0) {
    return 'Expired';
  }

  const hours = Math.floor(remaining / 3600);
  const minutes = Math.floor((remaining % 3600) / 60);
  const seconds = remaining % 60;

  if (hours > 0) {
    return `${hours}h ${minutes}m`;
  }
  if (minutes > 0) {
    return `${minutes}:${seconds.toString().padStart(2, '0')}`;
  }
  return `${seconds}s`;
}

/**
 * Check if a proof is expired
 */
export function isProofExpired(expiresAt: number): boolean {
  return Date.now() / 1000 > expiresAt;
}

/**
 * Generate a compact display string for verification result
 */
export function formatVerificationResult(
  valid: boolean,
  level: number,
  proofType?: string,
): string {
  const status = valid ? 'Valid' : 'Invalid';
  const levelStr = `Level ${level}`;
  const typeStr = proofType || 'Unknown';

  return `${status} (${levelStr}) - ${typeStr}`;
}
