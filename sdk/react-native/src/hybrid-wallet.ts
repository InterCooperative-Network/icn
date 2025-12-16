/**
 * ICN Hybrid Wallet for React Native
 *
 * Quantum-resistant wallet using Ed25519 + ML-DSA-65 hybrid signatures.
 * Provides migration path from classical Ed25519-only wallets.
 *
 * @module hybrid-wallet
 *
 * ## Size Considerations
 *
 * Hybrid keys are significantly larger than classical Ed25519:
 * - Public key: ~2KB (vs 32 bytes)
 * - Secret key: ~4KB (vs 32 bytes)
 * - Signature: ~3.4KB (vs 64 bytes)
 *
 * Ensure your secure storage can handle these sizes.
 *
 * ## Migration
 *
 * Existing Ed25519-only wallets can upgrade to hybrid:
 * ```typescript
 * const wallet = createHybridWallet(storage);
 * if (await wallet.hasClassicalOnlyKeys()) {
 *   await wallet.upgradeToHybrid();
 * }
 * ```
 */

import { SecureStorage, KeyPair as ClassicalKeyPair } from './types';
import {
  HybridCrypto,
  HybridKeyPair,
  HybridPublicKey,
  HybridSignature,
  HybridCryptoOptions,
  getHybridCryptoInfo,
} from './hybrid-crypto';

// Storage keys for hybrid wallet
const HYBRID_KEYPAIR_KEY = 'icn_hybrid_wallet_keypair';
const HYBRID_PUBLIC_KEY_KEY = 'icn_hybrid_wallet_public_key';
const DID_KEY = 'icn_wallet_did';
const WALLET_VERSION_KEY = 'icn_wallet_version';

// Classical wallet keys (for migration)
const CLASSICAL_PRIVATE_KEY = 'icn_wallet_private_key';
const CLASSICAL_PUBLIC_KEY = 'icn_wallet_public_key';

// Wallet versions
const WALLET_VERSION_CLASSICAL = '1';
const WALLET_VERSION_HYBRID = '2';

/**
 * Hybrid key pair info (safe to expose - no secrets)
 */
export interface HybridKeyPairInfo {
  /** DID derived from classical public key */
  did: string;
  /** Classical (Ed25519) public key hex */
  classicalPublicKey: string;
  /** Post-quantum (ML-DSA) public key hex */
  pqPublicKey: string;
  /** Whether this is a hybrid wallet (vs classical-only) */
  isHybrid: boolean;
  /** Total public key size in bytes */
  publicKeySize: number;
}

/**
 * Options for hybrid signing
 */
export interface HybridSignOptions extends HybridCryptoOptions {
  /**
   * Return signature as JSON object (default: false)
   * When false, returns hex-encoded concatenated signature
   */
  asJson?: boolean;
}

/**
 * Hybrid Wallet implementation
 */
export class HybridWallet {
  private storage: SecureStorage;
  private cachedKeyPair: HybridKeyPair | null = null;
  private isHybridMode: boolean = false;

  constructor(storage: SecureStorage) {
    this.storage = storage;
  }

  // ===========================================================================
  // Key Generation & Import
  // ===========================================================================

  /**
   * Generate a new hybrid keypair (Ed25519 + ML-DSA-65)
   *
   * @returns Hybrid key pair info
   */
  async generateKeyPair(): Promise<HybridKeyPairInfo> {
    const keypair = await HybridCrypto.generateKeyPair();

    // Serialize and store the full keypair
    const keypairJson = JSON.stringify({
      publicKey: keypair.publicKey,
      secretKey: keypair.secretKey,
      did: keypair.did,
    });

    await Promise.all([
      this.storage.setItem(HYBRID_KEYPAIR_KEY, keypairJson),
      this.storage.setItem(HYBRID_PUBLIC_KEY_KEY, HybridCrypto.serializePublicKey(keypair.publicKey)),
      this.storage.setItem(DID_KEY, keypair.did),
      this.storage.setItem(WALLET_VERSION_KEY, WALLET_VERSION_HYBRID),
    ]);

    this.cachedKeyPair = keypair;
    this.isHybridMode = true;

    return this.toKeyPairInfo(keypair);
  }

  /**
   * Import an existing hybrid keypair
   *
   * @param keypairJson - JSON-serialized hybrid keypair
   */
  async importKeyPair(keypairJson: string): Promise<HybridKeyPairInfo> {
    const parsed = JSON.parse(keypairJson);

    if (!parsed.secretKey || !parsed.publicKey) {
      throw new Error('Invalid hybrid keypair format');
    }

    const keypair: HybridKeyPair = {
      publicKey: parsed.publicKey,
      secretKey: parsed.secretKey,
      did: parsed.did || `did:icn:${this.deriveDidSuffix(parsed.publicKey.classical)}`,
    };

    await Promise.all([
      this.storage.setItem(HYBRID_KEYPAIR_KEY, keypairJson),
      this.storage.setItem(HYBRID_PUBLIC_KEY_KEY, HybridCrypto.serializePublicKey(keypair.publicKey)),
      this.storage.setItem(DID_KEY, keypair.did),
      this.storage.setItem(WALLET_VERSION_KEY, WALLET_VERSION_HYBRID),
    ]);

    this.cachedKeyPair = keypair;
    this.isHybridMode = true;

    return this.toKeyPairInfo(keypair);
  }

  // ===========================================================================
  // Key Retrieval
  // ===========================================================================

  /**
   * Get the current key pair info
   */
  async getKeyPairInfo(): Promise<HybridKeyPairInfo | null> {
    const keypair = await this.loadKeyPair();
    if (!keypair) return null;
    return this.toKeyPairInfo(keypair);
  }

  /**
   * Get the public key (safe to share)
   */
  async getPublicKey(): Promise<HybridPublicKey | null> {
    const publicKeyJson = await this.storage.getItem(HYBRID_PUBLIC_KEY_KEY);
    if (!publicKeyJson) return null;
    return HybridCrypto.deserializePublicKey(publicKeyJson);
  }

  /**
   * Get the DID
   */
  async getDid(): Promise<string | null> {
    return this.storage.getItem(DID_KEY);
  }

  /**
   * Check if any keypair exists
   */
  async hasKeyPair(): Promise<boolean> {
    return this.storage.hasItem(HYBRID_KEYPAIR_KEY) || this.storage.hasItem(CLASSICAL_PRIVATE_KEY);
  }

  /**
   * Check if this is a hybrid wallet (vs classical-only)
   */
  async isHybrid(): Promise<boolean> {
    const version = await this.storage.getItem(WALLET_VERSION_KEY);
    return version === WALLET_VERSION_HYBRID;
  }

  // ===========================================================================
  // Signing
  // ===========================================================================

  /**
   * Sign a message with hybrid signatures
   *
   * @param message - Hex-encoded message to sign
   * @param options - Signing options
   * @returns Signature (JSON object or hex string depending on options)
   */
  async sign(message: string, options: HybridSignOptions = {}): Promise<string> {
    const keypair = await this.loadKeyPair();
    if (!keypair) {
      throw new Error('No keypair stored. Generate or import a keypair first.');
    }

    const messageBytes = HybridCrypto.hexToBytes(message);
    const signature = await HybridCrypto.sign(messageBytes, keypair, options);

    if (options.asJson) {
      return HybridCrypto.serializeSignature(signature);
    }

    // Return concatenated hex: classical + pq
    return signature.classical + signature.pq;
  }

  /**
   * Sign with classical Ed25519 only (for ephemeral/short-lived proofs)
   *
   * Use this for time-limited credentials where quantum resistance
   * isn't needed (e.g., 6-hour QR code proofs, session tokens).
   *
   * @param message - Hex-encoded message to sign
   * @returns Ed25519 signature as hex string
   */
  async signClassicalOnly(message: string): Promise<string> {
    const keypair = await this.loadKeyPair();
    if (!keypair) {
      throw new Error('No keypair stored. Generate or import a keypair first.');
    }

    const messageBytes = HybridCrypto.hexToBytes(message);
    return HybridCrypto.signClassicalOnly(messageBytes, keypair);
  }

  /**
   * Sign raw bytes (for binary payloads)
   *
   * @param messageBytes - Message bytes to sign
   * @param options - Signing options
   * @returns Hybrid signature
   */
  async signBytes(messageBytes: Uint8Array, options: HybridCryptoOptions = {}): Promise<HybridSignature> {
    const keypair = await this.loadKeyPair();
    if (!keypair) {
      throw new Error('No keypair stored. Generate or import a keypair first.');
    }

    return HybridCrypto.sign(messageBytes, keypair, options);
  }

  // ===========================================================================
  // Verification (Static)
  // ===========================================================================

  /**
   * Verify a hybrid signature
   *
   * @param message - Original message bytes
   * @param signature - Hybrid signature to verify
   * @param publicKey - Hybrid public key
   */
  static async verify(
    message: Uint8Array,
    signature: HybridSignature,
    publicKey: HybridPublicKey
  ): Promise<boolean> {
    return HybridCrypto.verify(message, signature, publicKey);
  }

  /**
   * Verify a hex-encoded signature
   *
   * @param messageHex - Hex-encoded message
   * @param signatureJson - JSON-serialized signature
   * @param publicKeyJson - JSON-serialized public key
   */
  static async verifyHex(
    messageHex: string,
    signatureJson: string,
    publicKeyJson: string
  ): Promise<boolean> {
    const message = HybridCrypto.hexToBytes(messageHex);
    const signature = HybridCrypto.deserializeSignature(signatureJson);
    const publicKey = HybridCrypto.deserializePublicKey(publicKeyJson);
    return HybridCrypto.verify(message, signature, publicKey);
  }

  // ===========================================================================
  // Migration from Classical
  // ===========================================================================

  /**
   * Check if wallet has classical-only keys (needs upgrade)
   */
  async hasClassicalOnlyKeys(): Promise<boolean> {
    const hasClassical = await this.storage.hasItem(CLASSICAL_PRIVATE_KEY);
    const hasHybrid = await this.storage.hasItem(HYBRID_KEYPAIR_KEY);
    return hasClassical && !hasHybrid;
  }

  /**
   * Upgrade classical Ed25519-only wallet to hybrid
   *
   * This generates a new ML-DSA keypair and combines it with the
   * existing Ed25519 key. The DID remains the same (based on Ed25519 key).
   *
   * @returns The upgraded hybrid key pair info
   */
  async upgradeToHybrid(): Promise<HybridKeyPairInfo> {
    const classicalPrivateHex = await this.storage.getItem(CLASSICAL_PRIVATE_KEY);
    if (!classicalPrivateHex) {
      throw new Error('No classical private key found');
    }

    const classicalPublicHex = await this.storage.getItem(CLASSICAL_PUBLIC_KEY);
    if (!classicalPublicHex) {
      throw new Error('No classical public key found');
    }

    // Generate new ML-DSA keypair
    const { ml_dsa65 } = await import('@noble/post-quantum/ml-dsa.js');
    const pqKeypair = ml_dsa65.keygen();

    // Get existing DID
    const existingDid = await this.storage.getItem(DID_KEY);
    const did = existingDid || `did:icn:${this.deriveDidSuffix(classicalPublicHex)}`;

    // Construct hybrid keypair
    const keypair: HybridKeyPair = {
      publicKey: {
        classical: classicalPublicHex,
        pq: HybridCrypto.bytesToHex(pqKeypair.publicKey),
        version: 1,
      },
      secretKey: {
        classical: classicalPrivateHex,
        pq: HybridCrypto.bytesToHex(pqKeypair.secretKey),
        version: 1,
      },
      did,
    };

    // Store hybrid keypair
    const keypairJson = JSON.stringify({
      publicKey: keypair.publicKey,
      secretKey: keypair.secretKey,
      did: keypair.did,
    });

    await Promise.all([
      this.storage.setItem(HYBRID_KEYPAIR_KEY, keypairJson),
      this.storage.setItem(HYBRID_PUBLIC_KEY_KEY, HybridCrypto.serializePublicKey(keypair.publicKey)),
      this.storage.setItem(DID_KEY, did),
      this.storage.setItem(WALLET_VERSION_KEY, WALLET_VERSION_HYBRID),
    ]);

    // Clean up old classical-only keys (optional - keep for backwards compat)
    // await this.storage.removeItem(CLASSICAL_PRIVATE_KEY);
    // await this.storage.removeItem(CLASSICAL_PUBLIC_KEY);

    this.cachedKeyPair = keypair;
    this.isHybridMode = true;

    return this.toKeyPairInfo(keypair);
  }

  // ===========================================================================
  // Key Management
  // ===========================================================================

  /**
   * Delete all keys (classical and hybrid)
   */
  async deleteKeyPair(): Promise<void> {
    await Promise.all([
      this.storage.removeItem(HYBRID_KEYPAIR_KEY),
      this.storage.removeItem(HYBRID_PUBLIC_KEY_KEY),
      this.storage.removeItem(DID_KEY),
      this.storage.removeItem(WALLET_VERSION_KEY),
      this.storage.removeItem(CLASSICAL_PRIVATE_KEY),
      this.storage.removeItem(CLASSICAL_PUBLIC_KEY),
    ]);

    this.cachedKeyPair = null;
    this.isHybridMode = false;
  }

  /**
   * Export the keypair for backup (encrypted externally)
   *
   * WARNING: This exports secret key material. Encrypt before storing!
   */
  async exportKeyPair(): Promise<string> {
    const keypairJson = await this.storage.getItem(HYBRID_KEYPAIR_KEY);
    if (!keypairJson) {
      throw new Error('No keypair to export');
    }
    return keypairJson;
  }

  /**
   * Get classical KeyPair for backwards compatibility
   */
  async getClassicalKeyPair(): Promise<ClassicalKeyPair | null> {
    const keypair = await this.loadKeyPair();
    if (!keypair) return null;

    return {
      publicKey: keypair.publicKey.classical,
      did: keypair.did,
    };
  }

  // ===========================================================================
  // Internal Methods
  // ===========================================================================

  private async loadKeyPair(): Promise<HybridKeyPair | null> {
    if (this.cachedKeyPair) {
      return this.cachedKeyPair;
    }

    const keypairJson = await this.storage.getItem(HYBRID_KEYPAIR_KEY);
    if (!keypairJson) {
      return null;
    }

    const parsed = JSON.parse(keypairJson);
    this.cachedKeyPair = {
      publicKey: parsed.publicKey,
      secretKey: parsed.secretKey,
      did: parsed.did,
    };

    return this.cachedKeyPair;
  }

  private toKeyPairInfo(keypair: HybridKeyPair): HybridKeyPairInfo {
    return {
      did: keypair.did,
      classicalPublicKey: keypair.publicKey.classical,
      pqPublicKey: keypair.publicKey.pq,
      isHybrid: true,
      publicKeySize: HybridCrypto.publicKeySize(keypair.publicKey),
    };
  }

  private deriveDidSuffix(publicKeyHex: string): string {
    const ALPHABET = '123456789ABCDEFGHJKLMNPQRSTUVWXYZabcdefghijkmnopqrstuvwxyz';
    const bytes = HybridCrypto.hexToBytes(publicKeyHex);
    let num = BigInt('0x' + publicKeyHex);
    let result = '';

    while (num > 0n) {
      const remainder = Number(num % 58n);
      num = num / 58n;
      result = ALPHABET[remainder] + result;
    }

    for (const byte of bytes) {
      if (byte === 0) {
        result = '1' + result;
      } else {
        break;
      }
    }

    return 'z' + result;
  }
}

/**
 * Create a hybrid wallet instance
 *
 * @param storage - Secure storage implementation
 */
export function createHybridWallet(storage: SecureStorage): HybridWallet {
  return new HybridWallet(storage);
}

/**
 * Get hybrid cryptography info (sizes, etc.)
 */
export { getHybridCryptoInfo };

/**
 * Re-export core types
 */
export type { HybridKeyPair, HybridPublicKey, HybridSignature } from './hybrid-crypto';
