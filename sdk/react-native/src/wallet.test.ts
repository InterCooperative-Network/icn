/**
 * Tests for ICN Wallet
 */

import { ICNWalletImpl, createWallet } from './wallet';
import { SecureStorage } from './types';

// Mock secure storage implementation
function createMockStorage(): SecureStorage & { store: Map<string, string> } {
  const store = new Map<string, string>();
  return {
    store,
    async setItem(key: string, value: string): Promise<void> {
      store.set(key, value);
    },
    async getItem(key: string): Promise<string | null> {
      return store.get(key) ?? null;
    },
    async removeItem(key: string): Promise<void> {
      store.delete(key);
    },
    async hasItem(key: string): Promise<boolean> {
      return store.has(key);
    },
  };
}

describe('ICNWalletImpl', () => {
  let storage: ReturnType<typeof createMockStorage>;
  let wallet: ICNWalletImpl;

  beforeEach(() => {
    storage = createMockStorage();
    wallet = new ICNWalletImpl(storage);
  });

  describe('generateKeyPair', () => {
    it('should generate a key pair and store it', async () => {
      const keyPair = await wallet.generateKeyPair();

      expect(keyPair).toBeDefined();
      expect(keyPair.publicKey).toBeDefined();
      expect(keyPair.did).toBeDefined();
      expect(keyPair.did).toMatch(/^did:icn:/);

      // Verify stored in storage
      expect(storage.store.has('@icn/wallet/private_key')).toBe(true);
      expect(storage.store.has('@icn/wallet/public_key')).toBe(true);
      expect(storage.store.has('@icn/wallet/did')).toBe(true);
    });

    it('should generate unique key pairs', async () => {
      const keyPair1 = await wallet.generateKeyPair();

      // Create new wallet to generate different keys
      const wallet2 = new ICNWalletImpl(createMockStorage());
      const keyPair2 = await wallet2.generateKeyPair();

      expect(keyPair1.publicKey).not.toBe(keyPair2.publicKey);
      expect(keyPair1.did).not.toBe(keyPair2.did);
    });
  });

  describe('importKeyPair', () => {
    it('should import a valid private key', async () => {
      // 32 bytes = 64 hex characters
      const privateKey = 'a'.repeat(64);
      const keyPair = await wallet.importKeyPair(privateKey);

      expect(keyPair).toBeDefined();
      expect(keyPair.publicKey).toBeDefined();
      expect(keyPair.did).toMatch(/^did:icn:/);
    });

    it('should reject invalid private key length', async () => {
      const invalidKey = 'abc123'; // Too short

      await expect(wallet.importKeyPair(invalidKey)).rejects.toThrow(
        'Invalid private key length'
      );
    });

    it('should store imported key pair', async () => {
      const privateKey = 'b'.repeat(64);
      await wallet.importKeyPair(privateKey);

      expect(storage.store.get('@icn/wallet/private_key')).toBe(privateKey);
      expect(storage.store.has('@icn/wallet/public_key')).toBe(true);
      expect(storage.store.has('@icn/wallet/did')).toBe(true);
    });
  });

  describe('getKeyPair', () => {
    it('should return null when no key pair exists', async () => {
      const keyPair = await wallet.getKeyPair();
      expect(keyPair).toBeNull();
    });

    it('should return the stored key pair', async () => {
      const generated = await wallet.generateKeyPair();
      const retrieved = await wallet.getKeyPair();

      expect(retrieved).toEqual(generated);
    });

    it('should cache the key pair', async () => {
      await wallet.generateKeyPair();

      // First retrieval
      const first = await wallet.getKeyPair();

      // Clear storage to verify caching
      storage.store.clear();

      // Should still return cached value
      const second = await wallet.getKeyPair();
      expect(second).toEqual(first);
    });
  });

  describe('deleteKeyPair', () => {
    it('should remove all stored keys', async () => {
      await wallet.generateKeyPair();
      expect(storage.store.size).toBeGreaterThan(0);

      await wallet.deleteKeyPair();

      expect(storage.store.has('@icn/wallet/private_key')).toBe(false);
      expect(storage.store.has('@icn/wallet/public_key')).toBe(false);
      expect(storage.store.has('@icn/wallet/did')).toBe(false);
    });

    it('should clear the cache', async () => {
      await wallet.generateKeyPair();
      await wallet.deleteKeyPair();

      const keyPair = await wallet.getKeyPair();
      expect(keyPair).toBeNull();
    });
  });

  describe('sign', () => {
    it('should sign a message', async () => {
      await wallet.generateKeyPair();

      const signature = await wallet.sign('test message');

      expect(signature).toBeDefined();
      expect(typeof signature).toBe('string');
      // Base64 encoded
      expect(signature).toMatch(/^[A-Za-z0-9+/]+=*$/);
    });

    it('should throw when no key pair exists', async () => {
      await expect(wallet.sign('test')).rejects.toThrow(
        'No private key stored'
      );
    });

    it('should produce consistent signatures for same message', async () => {
      await wallet.generateKeyPair();

      const sig1 = await wallet.sign('same message');
      const sig2 = await wallet.sign('same message');

      expect(sig1).toBe(sig2);
    });

    it('should produce different signatures for different messages', async () => {
      await wallet.generateKeyPair();

      const sig1 = await wallet.sign('message 1');
      const sig2 = await wallet.sign('message 2');

      expect(sig1).not.toBe(sig2);
    });
  });

  describe('hasKeyPair', () => {
    it('should return false when no key pair exists', async () => {
      expect(await wallet.hasKeyPair()).toBe(false);
    });

    it('should return true after generating key pair', async () => {
      await wallet.generateKeyPair();
      expect(await wallet.hasKeyPair()).toBe(true);
    });

    it('should return false after deleting key pair', async () => {
      await wallet.generateKeyPair();
      await wallet.deleteKeyPair();
      expect(await wallet.hasKeyPair()).toBe(false);
    });
  });
});

describe('createWallet', () => {
  it('should create a wallet instance', () => {
    const storage = createMockStorage();
    const wallet = createWallet(storage);

    expect(wallet).toBeDefined();
    expect(wallet.generateKeyPair).toBeDefined();
    expect(wallet.sign).toBeDefined();
  });
});
