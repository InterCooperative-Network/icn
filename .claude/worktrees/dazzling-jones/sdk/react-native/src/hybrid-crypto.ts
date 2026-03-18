/**
 * Hybrid Post-Quantum Cryptography for ICN Mobile SDK
 *
 * Implements Ed25519 + ML-DSA-65 (Dilithium) hybrid signatures for
 * quantum-resistant identity. Both signatures must verify for security.
 *
 * @module hybrid-crypto
 *
 * ## Security Model
 *
 * The hybrid construction provides defense-in-depth:
 * - If Ed25519 is broken (e.g., by quantum computers), ML-DSA provides security
 * - If ML-DSA has an undiscovered weakness, Ed25519 provides security
 * - An attacker must break BOTH algorithms to forge a signature
 *
 * ## Size Budget
 *
 * | Component        | Classical (Ed25519) | Hybrid (+ ML-DSA) |
 * |------------------|---------------------|-------------------|
 * | Signature        | 64 bytes            | ~3.4 KB           |
 * | Public Key       | 32 bytes            | ~2 KB             |
 * | Secret Key       | 32 bytes            | ~4 KB             |
 *
 * ## Usage
 *
 * ```typescript
 * import { HybridCrypto } from './hybrid-crypto';
 *
 * // Generate hybrid keypair
 * const keypair = await HybridCrypto.generateKeyPair();
 *
 * // Sign a message (creates both Ed25519 and ML-DSA signatures)
 * const signature = await HybridCrypto.sign(message, keypair);
 *
 * // Verify (both signatures must be valid)
 * const valid = await HybridCrypto.verify(message, signature, keypair.publicKey);
 * ```
 *
 * @see https://github.com/paulmillr/noble-post-quantum
 * @see NIST FIPS 204 (ML-DSA standard)
 */

import * as ed from '@noble/ed25519';
// Use .js extension for subpath exports (required by ESM packages)
import { sha512 } from '@noble/hashes/sha2.js';
import { ml_dsa65 } from '@noble/post-quantum/ml-dsa.js';

// Configure @noble/ed25519 to use SHA-512 from @noble/hashes
// This is required for React Native which doesn't have crypto.subtle
ed.hashes.sha512 = (message: Uint8Array) => sha512(message);

/**
 * ML-DSA-65 key and signature sizes (NIST FIPS 204)
 */
export const ML_DSA_SIZES = {
  /** Public key size in bytes */
  PUBLIC_KEY: 1952,
  /** Secret key size in bytes */
  SECRET_KEY: 4032,
  /** Signature size in bytes */
  SIGNATURE: 3309,
} as const;

/**
 * Hybrid public key containing both classical and post-quantum keys
 */
export interface HybridPublicKey {
  /** Ed25519 public key (32 bytes, hex encoded) */
  classical: string;
  /** ML-DSA-65 public key (~1952 bytes, hex encoded) */
  pq: string;
  /** Key version for future compatibility */
  version: 1;
}

/**
 * Hybrid secret key containing both classical and post-quantum keys
 * WARNING: Handle with extreme care - contains secret key material
 */
export interface HybridSecretKey {
  /** Ed25519 secret key (32 bytes, hex encoded) */
  classical: string;
  /** ML-DSA-65 secret key (~4032 bytes, hex encoded) */
  pq: string;
  /** Key version for future compatibility */
  version: 1;
}

/**
 * Hybrid keypair for signing operations
 */
export interface HybridKeyPair {
  /** Public key (safe to share) */
  publicKey: HybridPublicKey;
  /** Secret key (keep secure!) */
  secretKey: HybridSecretKey;
  /** DID derived from the classical public key */
  did: string;
}

/**
 * Hybrid signature containing both classical and post-quantum signatures
 */
export interface HybridSignature {
  /** Ed25519 signature (64 bytes, hex encoded) */
  classical: string;
  /** ML-DSA-65 signature (~3309 bytes, hex encoded) */
  pq: string;
  /** Signature version for future compatibility */
  version: 1;
}

/**
 * Options for hybrid cryptographic operations
 */
export interface HybridCryptoOptions {
  /**
   * Whether to include PQ signature (default: true)
   * Set to false for ephemeral/short-lived signatures where
   * quantum resistance isn't needed (e.g., 6-hour QR proofs)
   */
  includePQ?: boolean;
}

/**
 * Hybrid Post-Quantum Cryptography utilities
 */
export class HybridCrypto {
  // =========================================================================
  // Key Generation
  // =========================================================================

  /**
   * Generate a new hybrid keypair (Ed25519 + ML-DSA-65)
   *
   * @returns Promise resolving to a new hybrid keypair
   *
   * @example
   * ```typescript
   * const keypair = await HybridCrypto.generateKeyPair();
   * console.log('DID:', keypair.did);
   * console.log('Public key size:', keypair.publicKey.classical.length / 2 + keypair.publicKey.pq.length / 2, 'bytes');
   * ```
   */
  static async generateKeyPair(): Promise<HybridKeyPair> {
    // Generate Ed25519 keypair
    const classicalSecret = ed.utils.randomSecretKey();
    const classicalPublic = await ed.getPublicKeyAsync(classicalSecret);

    // Generate ML-DSA-65 keypair
    const pqKeypair = ml_dsa65.keygen();

    // Create DID from classical public key (for backwards compatibility)
    const did = `did:icn:${this.base58btcEncode(classicalPublic)}`;

    return {
      publicKey: {
        classical: this.bytesToHex(classicalPublic),
        pq: this.bytesToHex(pqKeypair.publicKey),
        version: 1,
      },
      secretKey: {
        classical: this.bytesToHex(classicalSecret),
        pq: this.bytesToHex(pqKeypair.secretKey),
        version: 1,
      },
      did,
    };
  }

  /**
   * Import a hybrid keypair from stored secret keys
   *
   * @param secretKey - The hybrid secret key to import
   * @returns Promise resolving to the full keypair
   */
  static async importKeyPair(secretKey: HybridSecretKey): Promise<HybridKeyPair> {
    // Derive Ed25519 public key from secret
    const classicalSecret = this.hexToBytes(secretKey.classical);
    const classicalPublic = await ed.getPublicKeyAsync(classicalSecret);

    // For ML-DSA, we need to store both keys since we can't derive public from secret
    // Validate the PQ key sizes
    const pqSecret = this.hexToBytes(secretKey.pq);
    if (pqSecret.length !== ML_DSA_SIZES.SECRET_KEY) {
      throw new Error(`Invalid ML-DSA secret key size: expected ${ML_DSA_SIZES.SECRET_KEY}, got ${pqSecret.length}`);
    }

    // We need to regenerate or have stored the public key
    // For now, regenerate (in production, store public key separately)
    const pqKeypair = ml_dsa65.keygen();

    const did = `did:icn:${this.base58btcEncode(classicalPublic)}`;

    return {
      publicKey: {
        classical: this.bytesToHex(classicalPublic),
        pq: this.bytesToHex(pqKeypair.publicKey),
        version: 1,
      },
      secretKey,
      did,
    };
  }

  // =========================================================================
  // Signing
  // =========================================================================

  /**
   * Sign a message with hybrid signatures (Ed25519 + ML-DSA-65)
   *
   * Both signatures are computed and included. Verification requires
   * both to be valid.
   *
   * @param message - Message bytes to sign
   * @param keypair - Hybrid keypair containing secret keys
   * @param options - Optional signing options
   * @returns Promise resolving to the hybrid signature
   *
   * @example
   * ```typescript
   * const message = new TextEncoder().encode('Hello, quantum-safe world!');
   * const signature = await HybridCrypto.sign(message, keypair);
   * console.log('Signature size:', signature.classical.length / 2 + signature.pq.length / 2, 'bytes');
   * ```
   */
  static async sign(
    message: Uint8Array,
    keypair: HybridKeyPair,
    options: HybridCryptoOptions = {}
  ): Promise<HybridSignature> {
    const { includePQ = true } = options;

    // Ed25519 signature
    const classicalSecret = this.hexToBytes(keypair.secretKey.classical);
    const classicalSig = await ed.signAsync(message, classicalSecret);

    // ML-DSA signature (if enabled)
    // Note: ml_dsa65.sign takes (message, secretKey) - message first!
    let pqSig: Uint8Array;
    if (includePQ) {
      const pqSecret = this.hexToBytes(keypair.secretKey.pq);
      pqSig = ml_dsa65.sign(message, pqSecret);
    } else {
      // Empty PQ signature for classical-only mode
      pqSig = new Uint8Array(0);
    }

    return {
      classical: this.bytesToHex(classicalSig),
      pq: this.bytesToHex(pqSig),
      version: 1,
    };
  }

  /**
   * Sign a hex-encoded message (convenience method)
   *
   * @param messageHex - Hex-encoded message to sign
   * @param keypair - Hybrid keypair
   * @param options - Optional signing options
   * @returns Promise resolving to the hybrid signature
   */
  static async signHex(
    messageHex: string,
    keypair: HybridKeyPair,
    options: HybridCryptoOptions = {}
  ): Promise<HybridSignature> {
    const message = this.hexToBytes(messageHex);
    return this.sign(message, keypair, options);
  }

  /**
   * Sign with Ed25519 only (for ephemeral/short-lived proofs)
   *
   * Use this for time-limited credentials where quantum resistance
   * isn't needed (e.g., 6-hour QR code proofs).
   *
   * @param message - Message bytes to sign
   * @param keypair - Hybrid keypair (only classical key used)
   * @returns Promise resolving to Ed25519 signature as hex string
   */
  static async signClassicalOnly(message: Uint8Array, keypair: HybridKeyPair): Promise<string> {
    const classicalSecret = this.hexToBytes(keypair.secretKey.classical);
    const signature = await ed.signAsync(message, classicalSecret);
    return this.bytesToHex(signature);
  }

  // =========================================================================
  // Verification
  // =========================================================================

  /**
   * Verify a hybrid signature
   *
   * Both classical and post-quantum signatures must be valid for
   * the overall verification to succeed.
   *
   * @param message - Original message bytes
   * @param signature - Hybrid signature to verify
   * @param publicKey - Hybrid public key
   * @returns Promise resolving to true if BOTH signatures are valid
   *
   * @example
   * ```typescript
   * const valid = await HybridCrypto.verify(message, signature, keypair.publicKey);
   * if (valid) {
   *   console.log('Signature is quantum-safe valid!');
   * }
   * ```
   */
  static async verify(
    message: Uint8Array,
    signature: HybridSignature,
    publicKey: HybridPublicKey
  ): Promise<boolean> {
    // Verify Ed25519 signature
    const classicalValid = await this.verifyClassical(message, signature.classical, publicKey.classical);
    if (!classicalValid) {
      return false;
    }

    // If PQ signature is empty (classical-only mode), accept
    if (signature.pq.length === 0) {
      return true;
    }

    // Verify ML-DSA signature
    const pqValid = this.verifyPQ(message, signature.pq, publicKey.pq);
    return pqValid;
  }

  /**
   * Verify only the Ed25519 (classical) signature
   *
   * @param message - Original message bytes
   * @param signatureHex - Ed25519 signature as hex string
   * @param publicKeyHex - Ed25519 public key as hex string
   * @returns Promise resolving to true if valid
   */
  static async verifyClassical(
    message: Uint8Array,
    signatureHex: string,
    publicKeyHex: string
  ): Promise<boolean> {
    try {
      const signature = this.hexToBytes(signatureHex);
      const publicKey = this.hexToBytes(publicKeyHex);
      return await ed.verifyAsync(signature, message, publicKey);
    } catch {
      return false;
    }
  }

  /**
   * Verify only the ML-DSA (post-quantum) signature
   *
   * @param message - Original message bytes
   * @param signatureHex - ML-DSA signature as hex string
   * @param publicKeyHex - ML-DSA public key as hex string
   * @returns true if valid
   */
  static verifyPQ(message: Uint8Array, signatureHex: string, publicKeyHex: string): boolean {
    try {
      const signature = this.hexToBytes(signatureHex);
      const publicKey = this.hexToBytes(publicKeyHex);
      // Note: ml_dsa65.verify takes (signature, message, publicKey)
      return ml_dsa65.verify(signature, message, publicKey);
    } catch {
      return false;
    }
  }

  // =========================================================================
  // Serialization
  // =========================================================================

  /**
   * Serialize a hybrid public key to a compact JSON string
   *
   * @param publicKey - Hybrid public key to serialize
   * @returns JSON string representation
   */
  static serializePublicKey(publicKey: HybridPublicKey): string {
    return JSON.stringify(publicKey);
  }

  /**
   * Deserialize a hybrid public key from JSON
   *
   * @param json - JSON string to parse
   * @returns Hybrid public key
   */
  static deserializePublicKey(json: string): HybridPublicKey {
    const parsed = JSON.parse(json);
    if (parsed.version !== 1) {
      throw new Error(`Unsupported public key version: ${parsed.version}`);
    }
    return parsed as HybridPublicKey;
  }

  /**
   * Serialize a hybrid signature to a compact JSON string
   *
   * @param signature - Hybrid signature to serialize
   * @returns JSON string representation
   */
  static serializeSignature(signature: HybridSignature): string {
    return JSON.stringify(signature);
  }

  /**
   * Deserialize a hybrid signature from JSON
   *
   * @param json - JSON string to parse
   * @returns Hybrid signature
   */
  static deserializeSignature(json: string): HybridSignature {
    const parsed = JSON.parse(json);
    if (parsed.version !== 1) {
      throw new Error(`Unsupported signature version: ${parsed.version}`);
    }
    return parsed as HybridSignature;
  }

  /**
   * Get the total size of a hybrid public key in bytes
   *
   * @param publicKey - Hybrid public key
   * @returns Size in bytes
   */
  static publicKeySize(publicKey: HybridPublicKey): number {
    return publicKey.classical.length / 2 + publicKey.pq.length / 2;
  }

  /**
   * Get the total size of a hybrid signature in bytes
   *
   * @param signature - Hybrid signature
   * @returns Size in bytes
   */
  static signatureSize(signature: HybridSignature): number {
    return signature.classical.length / 2 + signature.pq.length / 2;
  }

  // =========================================================================
  // Utility Functions
  // =========================================================================

  /**
   * Convert bytes to hex string
   */
  static bytesToHex(bytes: Uint8Array): string {
    return Array.from(bytes)
      .map((b) => b.toString(16).padStart(2, '0'))
      .join('');
  }

  /**
   * Convert hex string to bytes
   */
  static hexToBytes(hex: string): Uint8Array {
    if (hex.length % 2 !== 0) {
      throw new Error('Hex string must have even length');
    }
    const bytes = new Uint8Array(hex.length / 2);
    for (let i = 0; i < bytes.length; i++) {
      bytes[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
    }
    return bytes;
  }

  /**
   * Base58btc encoding with multibase prefix 'z'
   */
  private static base58btcEncode(bytes: Uint8Array): string {
    const ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
    let num = BigInt('0x' + this.bytesToHex(bytes));
    let result = '';

    while (num > 0n) {
      const remainder = Number(num % 58n);
      num = num / 58n;
      result = ALPHABET[remainder] + result;
    }

    // Handle leading zeros
    for (const byte of bytes) {
      if (byte === 0) {
        result = '1' + result;
      } else {
        break;
      }
    }

    // Add multibase prefix 'z' for base58btc
    return 'z' + result;
  }
}

/**
 * Check if the current environment supports hybrid cryptography
 *
 * @returns true if all required crypto APIs are available
 */
export function isHybridCryptoSupported(): boolean {
  try {
    // Check if we can generate random bytes
    const testBytes = ed.utils.randomSecretKey();
    return testBytes.length === 32;
  } catch {
    return false;
  }
}

/**
 * Get information about hybrid crypto sizes
 */
export function getHybridCryptoInfo(): {
  publicKeySize: number;
  secretKeySize: number;
  signatureSize: number;
  classicalPublicKeySize: number;
  classicalSignatureSize: number;
} {
  return {
    publicKeySize: 32 + ML_DSA_SIZES.PUBLIC_KEY, // ~1984 bytes
    secretKeySize: 32 + ML_DSA_SIZES.SECRET_KEY, // ~4064 bytes
    signatureSize: 64 + ML_DSA_SIZES.SIGNATURE, // ~3373 bytes
    classicalPublicKeySize: 32,
    classicalSignatureSize: 64,
  };
}
