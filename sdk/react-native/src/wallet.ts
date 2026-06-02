/**
 * ICN Device Keyring for React Native (legacy-named "wallet")
 *
 * Local private-key custody and signing. Generates / imports / stores an Ed25519
 * key pair in secure storage (iOS Keychain / Android Keystore) and signs challenges
 * with it. In the passport / keyring / position / receipt doctrine this is a
 * **Device Keyring**: it holds keys and produces signatures — it does NOT hold
 * balances, tokens, or value, and is not an account or a settlement instrument.
 *
 * The exported names (`ICNWalletImpl`, `createWallet`, and the `ICNWallet`
 * interface) are retained for backward compatibility. A future, compatibility-safe
 * PR may add canonical `Keyring` / `DeviceKeyring` aliases without removing them.
 *
 * Uses @noble/ed25519 for cryptographic operations.
 * See https://github.com/InterCooperative-Network/icn/blob/main/docs/design/passport-keyring-position-receipt.md
 */

import * as ed from '@noble/ed25519';
// Use .js extension for ESM subpath exports
import { sha512 } from '@noble/hashes/sha2.js';
import { ICNWallet, KeyPair, SecureStorage } from './types';

// Configure @noble/ed25519 to use synchronous SHA-512 from @noble/hashes
// This is required for React Native which doesn't have crypto.subtle
ed.hashes.sha512 = (message: Uint8Array) => sha512(message);

// SecureStore keys must be alphanumeric with periods, underscores, or hyphens only (no slashes).
// Canonical Device Keyring storage keys. The legacy `icn_wallet_*` names are pre-release with no
// installed base, so they are renamed directly here — no lazy migration is performed
// (see docs/design/wallet-did-migration-boundary.md).
const PRIVATE_KEY_KEY = 'icn_keyring_private_key';
const PUBLIC_KEY_KEY = 'icn_keyring_public_key';
const DID_KEY = 'icn_keyring_did';

// Defensive purge set for deleteKeyPair(): legacy classical names PLUS the hybrid keyring namespace
// (canonical + legacy) and the version markers. None of these are written by this classical keyring;
// removing them is a no-op safety net so a sign-out-and-forget on a device that previously held a
// hybrid keyring (or pre-rename keys) leaves no stray keyring secret behind.
const DEFENSIVE_PURGE_KEYS = [
  // legacy classical
  'icn_wallet_private_key',
  'icn_wallet_public_key',
  'icn_wallet_did',
  // hybrid keyring namespace (canonical + legacy)
  'icn_hybrid_keyring_keypair',
  'icn_hybrid_keyring_public_key',
  'icn_hybrid_wallet_keypair',
  'icn_hybrid_wallet_public_key',
  // version markers (canonical + legacy)
  'icn_keyring_version',
  'icn_wallet_version',
];

/**
 * Device Keyring implementation (legacy-named "wallet") using secure storage and @noble/ed25519
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
    try {
      await Promise.all([
        this.storage.removeItem(PRIVATE_KEY_KEY),
        this.storage.removeItem(PUBLIC_KEY_KEY),
        this.storage.removeItem(DID_KEY),
        // Defensive: also purge legacy names and the hybrid keyring namespace (no-op on fresh
        // installs) so a reset leaves no stray keyring secret behind.
        ...DEFENSIVE_PURGE_KEYS.map((k) => this.storage.removeItem(k)),
      ]);
    } finally {
      // Always drop cached secrets, even if a storage removal rejected — otherwise the in-memory
      // keyring could still return the old DID and sign with the cached private key after deletion.
      this.cachedKeyPair = null;
      this.cachedPrivateKey = null;
    }
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
 * Create a Device Keyring backed by secure storage.
 *
 * Returns local key custody + signing only; it holds no balances, tokens, or value.
 *
 * @deprecated Prefer the canonical {@link createKeyring}. `createWallet` is retained as a
 * backward-compatible alias and behaves identically (same return type and storage keys).
 */
export function createWallet(storage: SecureStorage): ICNWallet {
  return new ICNWalletImpl(storage);
}

/**
 * Canonical Device Keyring factory — preferred alias of {@link createWallet}.
 *
 * Identical behavior, signature, and persisted storage keys; no migration occurs.
 */
export const createKeyring = createWallet;

/**
 * Canonical Device Keyring implementation class — preferred alias of {@link ICNWalletImpl}.
 */
export { ICNWalletImpl as ICNKeyringImpl };
