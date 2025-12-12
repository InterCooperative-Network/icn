/**
 * ICN Wallet for React Native
 *
 * Manages identity keys with secure storage.
 * Uses @noble/ed25519 for cryptographic operations.
 */

import * as ed from '@noble/ed25519';
import { sha512 } from '@noble/hashes/sha2';
import { ICNWallet, KeyPair, SecureStorage } from './types';

// Configure @noble/ed25519 to use synchronous SHA-512 from @noble/hashes
// This is required for React Native which doesn't have crypto.subtle
ed.hashes.sha512 = (message: Uint8Array) => sha512(message);

// SecureStore keys must be alphanumeric with periods, underscores, or hyphens only (no slashes)
const PRIVATE_KEY_KEY = 'icn_wallet_private_key';
const PUBLIC_KEY_KEY = 'icn_wallet_public_key';
const DID_KEY = 'icn_wallet_did';

/**
 * Wallet implementation using secure storage and @noble/ed25519
 *
 * @example
 * ```typescript
 * import { createWallet, SecureStorage } from '@icn/react-native';
 * import * as SecureStore from 'expo-secure-store';
 *
 * // Create secure storage adapter
 * const secureStorage: SecureStorage = {
 *   async setItem(key, value) {
 *     await SecureStore.setItemAsync(key, value);
 *   },
 *   async getItem(key) {
 *     return SecureStore.getItemAsync(key);
 *   },
 *   async removeItem(key) {
 *     await SecureStore.deleteItemAsync(key);
 *   },
 *   async hasItem(key) {
 *     const value = await SecureStore.getItemAsync(key);
 *     return value !== null;
 *   },
 * };
 *
 * const wallet = createWallet(secureStorage);
 * ```
 */
export class ICNWalletImpl implements ICNWallet {
  private storage: SecureStorage;
  private cachedKeyPair: KeyPair | null = null;
  private cachedPrivateKey: Uint8Array | null = null;

  constructor(storage: SecureStorage) {
    this.storage = storage;
  }

  /**
   * Generate a new Ed25519 key pair
   */
  async generateKeyPair(): Promise<KeyPair> {
    // Generate 32 random bytes for the private key
    const privateKey = ed.utils.randomSecretKey();

    // Derive public key from private key (sync version - uses hashes.sha512)
    const publicKey = ed.getPublicKey(privateKey);

    // Convert to hex strings for storage
    const privateKeyHex = this.bytesToHex(privateKey);
    const publicKeyHex = this.bytesToHex(publicKey);

    // Generate DID from public key using multibase base58btc
    const did = `did:icn:${this.base58btcEncode(publicKey)}`;

    // Store keys securely
    await Promise.all([
      this.storage.setItem(PRIVATE_KEY_KEY, privateKeyHex),
      this.storage.setItem(PUBLIC_KEY_KEY, publicKeyHex),
      this.storage.setItem(DID_KEY, did),
    ]);

    const keyPair: KeyPair = { publicKey: publicKeyHex, did };
    this.cachedKeyPair = keyPair;
    this.cachedPrivateKey = privateKey;

    return keyPair;
  }

  /**
   * Import an existing key pair
   */
  async importKeyPair(privateKeyHex: string): Promise<KeyPair> {
    const privateKey = this.hexToBytes(privateKeyHex);
    if (privateKey.length !== 32) {
      throw new Error('Invalid private key length. Expected 32 bytes.');
    }

    // Derive public key from private key (sync version - uses hashes.sha512)
    const publicKey = ed.getPublicKey(privateKey);
    const publicKeyHex = this.bytesToHex(publicKey);
    const did = `did:icn:${this.base58btcEncode(publicKey)}`;

    await Promise.all([
      this.storage.setItem(PRIVATE_KEY_KEY, privateKeyHex),
      this.storage.setItem(PUBLIC_KEY_KEY, publicKeyHex),
      this.storage.setItem(DID_KEY, did),
    ]);

    const keyPair: KeyPair = { publicKey: publicKeyHex, did };
    this.cachedKeyPair = keyPair;
    this.cachedPrivateKey = privateKey;

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
    this.cachedPrivateKey = null;
  }

  /**
   * Sign a message with the stored private key
   *
   * The message is expected to be a hex-encoded string (the challenge nonce).
   * Returns the signature as a hex-encoded string.
   */
  async sign(message: string): Promise<string> {
    // Validate message
    if (!message || typeof message !== 'string') {
      throw new Error(`Invalid message to sign: expected hex string, got ${typeof message}`);
    }

    // Get private key from cache or storage
    let privateKey = this.cachedPrivateKey;
    if (!privateKey) {
      const privateKeyHex = await this.storage.getItem(PRIVATE_KEY_KEY);
      if (!privateKeyHex) {
        throw new Error('No private key stored. Generate or import a key pair first.');
      }
      privateKey = this.hexToBytes(privateKeyHex);
      this.cachedPrivateKey = privateKey;
    }

    // The message from the gateway is a hex-encoded nonce
    // We need to sign the raw bytes of the nonce
    const messageBytes = this.hexToBytes(message);

    // Sign with Ed25519 (sync version - uses hashes.sha512)
    const signature = ed.sign(messageBytes, privateKey);

    // Return signature as hex string
    return this.bytesToHex(signature);
  }

  /**
   * Check if a key pair is stored
   */
  async hasKeyPair(): Promise<boolean> {
    return this.storage.hasItem(PRIVATE_KEY_KEY);
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

  /**
   * Base58btc encoding with multibase prefix 'z'
   */
  private base58btcEncode(bytes: Uint8Array): string {
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
 * Create a wallet with secure storage
 */
export function createWallet(storage: SecureStorage): ICNWallet {
  return new ICNWalletImpl(storage);
}
