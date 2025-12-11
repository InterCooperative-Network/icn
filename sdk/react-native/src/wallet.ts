/**
 * ICN Wallet for React Native
 *
 * Manages identity keys with secure storage.
 * Uses react-native-keychain or expo-secure-store for key storage.
 */

import { ICNWallet, KeyPair, SecureStorage } from './types';

const PRIVATE_KEY_KEY = '@icn/wallet/private_key';
const PUBLIC_KEY_KEY = '@icn/wallet/public_key';
const DID_KEY = '@icn/wallet/did';

/**
 * Wallet implementation using secure storage
 *
 * Note: This is a reference implementation. For production, you should:
 * 1. Use react-native-keychain for iOS Keychain / Android Keystore
 * 2. Use a proper cryptographic library like @noble/ed25519
 * 3. Consider hardware security modules on supported devices
 *
 * @example
 * ```typescript
 * import { ICNWallet } from '@icn/react-native';
 * import * as Keychain from 'react-native-keychain';
 *
 * // Create secure storage adapter
 * const secureStorage: SecureStorage = {
 *   async setItem(key, value) {
 *     await Keychain.setGenericPassword(key, value, { service: key });
 *   },
 *   async getItem(key) {
 *     const result = await Keychain.getGenericPassword({ service: key });
 *     return result ? result.password : null;
 *   },
 *   async removeItem(key) {
 *     await Keychain.resetGenericPassword({ service: key });
 *   },
 *   async hasItem(key) {
 *     const result = await Keychain.getGenericPassword({ service: key });
 *     return !!result;
 *   },
 * };
 *
 * const wallet = new ICNWallet(secureStorage);
 * ```
 */
export class ICNWalletImpl implements ICNWallet {
  private storage: SecureStorage;
  private cachedKeyPair: KeyPair | null = null;

  constructor(storage: SecureStorage) {
    this.storage = storage;
  }

  /**
   * Generate a new Ed25519 key pair
   *
   * Note: This requires a cryptographic library. In production, use:
   * - react-native-quick-crypto
   * - @noble/ed25519
   * - expo-crypto
   */
  async generateKeyPair(): Promise<KeyPair> {
    // Generate 32 random bytes for the private key
    const privateKeyBytes = await this.generateRandomBytes(32);
    const privateKey = this.bytesToHex(privateKeyBytes);

    // Derive public key (simplified - use proper Ed25519 in production)
    const publicKey = await this.derivePublicKey(privateKeyBytes);

    // Generate DID from public key
    const did = `did:icn:${this.base58Encode(this.hexToBytes(publicKey))}`;

    // Store keys securely
    await Promise.all([
      this.storage.setItem(PRIVATE_KEY_KEY, privateKey),
      this.storage.setItem(PUBLIC_KEY_KEY, publicKey),
      this.storage.setItem(DID_KEY, did),
    ]);

    const keyPair: KeyPair = { publicKey, did };
    this.cachedKeyPair = keyPair;

    return keyPair;
  }

  /**
   * Import an existing key pair
   */
  async importKeyPair(privateKey: string): Promise<KeyPair> {
    const privateKeyBytes = this.hexToBytes(privateKey);
    if (privateKeyBytes.length !== 32) {
      throw new Error('Invalid private key length. Expected 32 bytes.');
    }

    const publicKey = await this.derivePublicKey(privateKeyBytes);
    const did = `did:icn:${this.base58Encode(this.hexToBytes(publicKey))}`;

    await Promise.all([
      this.storage.setItem(PRIVATE_KEY_KEY, privateKey),
      this.storage.setItem(PUBLIC_KEY_KEY, publicKey),
      this.storage.setItem(DID_KEY, did),
    ]);

    const keyPair: KeyPair = { publicKey, did };
    this.cachedKeyPair = keyPair;

    return keyPair;
  }

  /**
   * Get the stored key pair
   */
  async getKeyPair(): Promise<KeyPair | null> {
    if (this.cachedKeyPair) {
      return this.cachedKeyPair;
    }

    const [publicKey, did] = await Promise.all([
      this.storage.getItem(PUBLIC_KEY_KEY),
      this.storage.getItem(DID_KEY),
    ]);

    if (publicKey && did) {
      this.cachedKeyPair = { publicKey, did };
      return this.cachedKeyPair;
    }

    return null;
  }

  /**
   * Delete the stored key pair
   */
  async deleteKeyPair(): Promise<void> {
    await Promise.all([
      this.storage.removeItem(PRIVATE_KEY_KEY),
      this.storage.removeItem(PUBLIC_KEY_KEY),
      this.storage.removeItem(DID_KEY),
    ]);
    this.cachedKeyPair = null;
  }

  /**
   * Sign a message with the stored private key
   */
  async sign(message: string): Promise<string> {
    const privateKey = await this.storage.getItem(PRIVATE_KEY_KEY);
    if (!privateKey) {
      throw new Error('No private key stored. Generate or import a key pair first.');
    }

    // Sign the message
    const messageBytes = new TextEncoder().encode(message);
    const privateKeyBytes = this.hexToBytes(privateKey);

    // Simplified signing - use proper Ed25519 in production
    const signature = await this.ed25519Sign(messageBytes, privateKeyBytes);

    return this.bytesToBase64(signature);
  }

  /**
   * Check if a key pair is stored
   */
  async hasKeyPair(): Promise<boolean> {
    return this.storage.hasItem(PRIVATE_KEY_KEY);
  }

  // ===========================================================================
  // Cryptographic helpers - Replace with proper implementation in production
  // ===========================================================================

  /**
   * Generate random bytes
   * In production, use:
   * - react-native-quick-crypto
   * - expo-crypto
   */
  private async generateRandomBytes(length: number): Promise<Uint8Array> {
    // In React Native, you should use a native crypto module
    // This is a placeholder that won't work in production
    if (typeof crypto !== 'undefined' && crypto.getRandomValues) {
      const bytes = new Uint8Array(length);
      crypto.getRandomValues(bytes);
      return bytes;
    }

    throw new Error(
      'No secure random source available. ' +
      'Install react-native-quick-crypto or expo-crypto.'
    );
  }

  /**
   * Derive public key from private key
   * Replace with proper Ed25519 implementation
   */
  private async derivePublicKey(privateKey: Uint8Array): Promise<string> {
    // This is a placeholder - use @noble/ed25519 or similar in production
    // For now, we'll use a simple hash as a placeholder
    const hash = await this.sha256(privateKey);
    return this.bytesToHex(hash);
  }

  /**
   * Sign with Ed25519
   * Replace with proper implementation using @noble/ed25519
   */
  private async ed25519Sign(
    message: Uint8Array,
    privateKey: Uint8Array
  ): Promise<Uint8Array> {
    // This is a placeholder - use @noble/ed25519 in production
    // The actual signature should be 64 bytes
    const combined = new Uint8Array(message.length + privateKey.length);
    combined.set(privateKey);
    combined.set(message, privateKey.length);
    return this.sha256(combined);
  }

  /**
   * SHA-256 hash
   */
  private async sha256(data: Uint8Array): Promise<Uint8Array> {
    if (typeof crypto !== 'undefined' && crypto.subtle) {
      // Pass Uint8Array directly - crypto.subtle.digest accepts BufferSource
      // Avoid using data.buffer which can cause issues if the array is a view with non-zero offset
      const hashBuffer = await crypto.subtle.digest('SHA-256', data);
      return new Uint8Array(hashBuffer);
    }
    throw new Error('No SHA-256 implementation available');
  }

  // ===========================================================================
  // Encoding utilities
  // ===========================================================================

  private bytesToHex(bytes: Uint8Array): string {
    return Array.from(bytes)
      .map((b) => b.toString(16).padStart(2, '0'))
      .join('');
  }

  private hexToBytes(hex: string): Uint8Array {
    const bytes = new Uint8Array(hex.length / 2);
    for (let i = 0; i < bytes.length; i++) {
      bytes[i] = parseInt(hex.slice(i * 2, i * 2 + 2), 16);
    }
    return bytes;
  }

  private bytesToBase64(bytes: Uint8Array): string {
    const binary = String.fromCharCode(...bytes);
    return btoa(binary);
  }

  private base58Encode(bytes: Uint8Array): string {
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

    return result;
  }
}

/**
 * Create a wallet with secure storage
 */
export function createWallet(storage: SecureStorage): ICNWallet {
  return new ICNWalletImpl(storage);
}
