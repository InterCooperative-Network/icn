/**
 * Tests for Hybrid Wallet
 */

import { HybridWallet, createHybridWallet, getHybridCryptoInfo } from './hybrid-wallet';
import { HybridCrypto, ML_DSA_SIZES } from './hybrid-crypto';
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

describe('HybridWallet', () => {
  let storage: ReturnType<typeof createMockStorage>;
  let wallet: HybridWallet;

  beforeEach(() => {
    storage = createMockStorage();
    wallet = new HybridWallet(storage);
  });

  describe('generateKeyPair', () => {
    it('should generate a hybrid keypair and store it', async () => {
      const keyPairInfo = await wallet.generateKeyPair();

      expect(keyPairInfo).toBeDefined();
      expect(keyPairInfo.did).toMatch(/^did:icn:z/);
      expect(keyPairInfo.classicalPublicKey).toHaveLength(64); // 32 bytes hex
      expect(keyPairInfo.pqPublicKey).toHaveLength(ML_DSA_SIZES.PUBLIC_KEY * 2);
      expect(keyPairInfo.isHybrid).toBe(true);
      expect(keyPairInfo.publicKeySize).toBe(32 + ML_DSA_SIZES.PUBLIC_KEY);

      // Verify stored
      expect(storage.store.has('icn_hybrid_wallet_keypair')).toBe(true);
      expect(storage.store.has('icn_hybrid_wallet_public_key')).toBe(true);
      expect(storage.store.has('icn_wallet_did')).toBe(true);
      expect(storage.store.get('icn_wallet_version')).toBe('2');
    });

    it('should generate unique keypairs', async () => {
      const info1 = await wallet.generateKeyPair();

      const wallet2 = new HybridWallet(createMockStorage());
      const info2 = await wallet2.generateKeyPair();

      expect(info1.did).not.toBe(info2.did);
      expect(info1.classicalPublicKey).not.toBe(info2.classicalPublicKey);
    });
  });

  describe('importKeyPair', () => {
    it('should import a valid hybrid keypair', async () => {
      // Generate a keypair to export
      const sourceWallet = new HybridWallet(createMockStorage());
      await sourceWallet.generateKeyPair();
      const exported = await sourceWallet.exportKeyPair();

      // Import into new wallet
      const info = await wallet.importKeyPair(exported);

      expect(info).toBeDefined();
      expect(info.isHybrid).toBe(true);

      // Verify stored correctly
      expect(await wallet.hasKeyPair()).toBe(true);
      expect(await wallet.isHybrid()).toBe(true);
    });

    it('should reject invalid keypair format', async () => {
      await expect(wallet.importKeyPair('{}')).rejects.toThrow('Invalid hybrid keypair format');
    });
  });

  describe('getKeyPairInfo', () => {
    it('should return null when no keypair exists', async () => {
      const info = await wallet.getKeyPairInfo();
      expect(info).toBeNull();
    });

    it('should return keypair info after generation', async () => {
      const generated = await wallet.generateKeyPair();
      const retrieved = await wallet.getKeyPairInfo();

      expect(retrieved).toBeDefined();
      expect(retrieved!.did).toBe(generated.did);
      expect(retrieved!.classicalPublicKey).toBe(generated.classicalPublicKey);
    });
  });

  describe('getPublicKey', () => {
    it('should return the public key', async () => {
      await wallet.generateKeyPair();
      const publicKey = await wallet.getPublicKey();

      expect(publicKey).toBeDefined();
      expect(publicKey!.classical).toHaveLength(64);
      expect(publicKey!.pq).toHaveLength(ML_DSA_SIZES.PUBLIC_KEY * 2);
      expect(publicKey!.version).toBe(1);
    });
  });

  describe('getDid', () => {
    it('should return the DID', async () => {
      const info = await wallet.generateKeyPair();
      const did = await wallet.getDid();

      expect(did).toBe(info.did);
    });
  });

  describe('sign', () => {
    it('should sign a hex message', async () => {
      await wallet.generateKeyPair();

      const messageHex = '48656c6c6f'; // "Hello" in hex
      const signature = await wallet.sign(messageHex);

      expect(signature).toBeDefined();
      expect(typeof signature).toBe('string');
      // Concatenated hex: classical (128) + pq (6618)
      expect(signature.length).toBe(128 + ML_DSA_SIZES.SIGNATURE * 2);
    });

    it('should sign with JSON output', async () => {
      await wallet.generateKeyPair();

      const messageHex = '48656c6c6f';
      const signatureJson = await wallet.sign(messageHex, { asJson: true });

      const parsed = JSON.parse(signatureJson);
      expect(parsed.classical).toBeDefined();
      expect(parsed.pq).toBeDefined();
      expect(parsed.version).toBe(1);
    });

    it('should throw when no keypair exists', async () => {
      await expect(wallet.sign('test')).rejects.toThrow('No keypair stored');
    });
  });

  describe('signClassicalOnly', () => {
    it('should sign with Ed25519 only', async () => {
      await wallet.generateKeyPair();

      const messageHex = '48656c6c6f';
      const signature = await wallet.signClassicalOnly(messageHex);

      expect(signature).toHaveLength(128); // Ed25519 signature
    });
  });

  describe('signBytes', () => {
    it('should sign raw bytes', async () => {
      await wallet.generateKeyPair();

      const message = new TextEncoder().encode('Hello');
      const signature = await wallet.signBytes(message);

      expect(signature.classical).toHaveLength(128);
      expect(signature.pq).toHaveLength(ML_DSA_SIZES.SIGNATURE * 2);
    });
  });

  describe('verify (static)', () => {
    it('should verify signatures', async () => {
      await wallet.generateKeyPair();

      const message = new TextEncoder().encode('Test message');
      const signature = await wallet.signBytes(message);
      const publicKey = await wallet.getPublicKey();

      const valid = await HybridWallet.verify(message, signature, publicKey!);
      expect(valid).toBe(true);
    });

    it('should reject invalid signatures', async () => {
      await wallet.generateKeyPair();
      const publicKey = await wallet.getPublicKey();

      const message = new TextEncoder().encode('Test message');
      const fakeSignature = {
        classical: 'a'.repeat(128),
        pq: 'b'.repeat(ML_DSA_SIZES.SIGNATURE * 2),
        version: 1 as const,
      };

      const valid = await HybridWallet.verify(message, fakeSignature, publicKey!);
      expect(valid).toBe(false);
    });
  });

  describe('verifyHex (static)', () => {
    it('should verify hex-encoded signatures', async () => {
      await wallet.generateKeyPair();

      const messageHex = '48656c6c6f';
      const signatureJson = await wallet.sign(messageHex, { asJson: true });
      const publicKeyJson = HybridCrypto.serializePublicKey((await wallet.getPublicKey())!);

      const valid = await HybridWallet.verifyHex(messageHex, signatureJson, publicKeyJson);
      expect(valid).toBe(true);
    });
  });

  describe('deleteKeyPair', () => {
    it('should delete all stored keys', async () => {
      await wallet.generateKeyPair();
      expect(storage.store.size).toBeGreaterThan(0);

      await wallet.deleteKeyPair();

      expect(storage.store.has('icn_hybrid_wallet_keypair')).toBe(false);
      expect(storage.store.has('icn_hybrid_wallet_public_key')).toBe(false);
      expect(storage.store.has('icn_wallet_did')).toBe(false);
    });

    it('should clear the cache', async () => {
      await wallet.generateKeyPair();
      await wallet.deleteKeyPair();

      const info = await wallet.getKeyPairInfo();
      expect(info).toBeNull();
    });
  });

  describe('hasKeyPair', () => {
    it('should return false when no keypair exists', async () => {
      expect(await wallet.hasKeyPair()).toBe(false);
    });

    it('should return true after generating keypair', async () => {
      await wallet.generateKeyPair();
      expect(await wallet.hasKeyPair()).toBe(true);
    });

    it('should return false after deleting keypair', async () => {
      await wallet.generateKeyPair();
      await wallet.deleteKeyPair();
      expect(await wallet.hasKeyPair()).toBe(false);
    });
  });

  describe('isHybrid', () => {
    it('should return true for hybrid wallet', async () => {
      await wallet.generateKeyPair();
      expect(await wallet.isHybrid()).toBe(true);
    });

    it('should return false for empty wallet', async () => {
      expect(await wallet.isHybrid()).toBe(false);
    });
  });

  describe('exportKeyPair', () => {
    it('should export the keypair as JSON', async () => {
      await wallet.generateKeyPair();
      const exported = await wallet.exportKeyPair();

      const parsed = JSON.parse(exported);
      expect(parsed.publicKey).toBeDefined();
      expect(parsed.secretKey).toBeDefined();
      expect(parsed.did).toBeDefined();
    });

    it('should throw when no keypair exists', async () => {
      await expect(wallet.exportKeyPair()).rejects.toThrow('No keypair to export');
    });
  });

  describe('getClassicalKeyPair', () => {
    it('should return classical KeyPair for backwards compatibility', async () => {
      const info = await wallet.generateKeyPair();
      const classical = await wallet.getClassicalKeyPair();

      expect(classical).toBeDefined();
      expect(classical!.publicKey).toBe(info.classicalPublicKey);
      expect(classical!.did).toBe(info.did);
    });
  });

  describe('migration from classical', () => {
    it('should detect classical-only keys', async () => {
      // Simulate classical-only wallet
      storage.store.set('icn_wallet_private_key', 'a'.repeat(64));
      storage.store.set('icn_wallet_public_key', 'b'.repeat(64));

      expect(await wallet.hasClassicalOnlyKeys()).toBe(true);
    });

    it('should not detect classical-only when hybrid exists', async () => {
      // Set both classical and hybrid
      storage.store.set('icn_wallet_private_key', 'a'.repeat(64));
      await wallet.generateKeyPair();

      expect(await wallet.hasClassicalOnlyKeys()).toBe(false);
    });

    it('should upgrade classical wallet to hybrid', async () => {
      // Create classical-only wallet state
      // Use a valid 32-byte private key
      const privateKeyHex = '0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef';
      // Derive the public key (we'll just use a placeholder for test)
      storage.store.set('icn_wallet_private_key', privateKeyHex);
      storage.store.set('icn_wallet_public_key', 'c'.repeat(64));
      storage.store.set('icn_wallet_did', 'did:icn:zTest123');
      storage.store.set('icn_wallet_version', '1');

      expect(await wallet.hasClassicalOnlyKeys()).toBe(true);

      const info = await wallet.upgradeToHybrid();

      expect(info.isHybrid).toBe(true);
      expect(info.classicalPublicKey).toBe('c'.repeat(64)); // Preserved
      expect(info.pqPublicKey).toHaveLength(ML_DSA_SIZES.PUBLIC_KEY * 2); // New PQ key
      expect(info.did).toBe('did:icn:zTest123'); // DID preserved

      // Verify it's now a hybrid wallet
      expect(await wallet.isHybrid()).toBe(true);
      expect(await wallet.hasClassicalOnlyKeys()).toBe(false);
    });

    it('should throw when no classical key exists for upgrade', async () => {
      await expect(wallet.upgradeToHybrid()).rejects.toThrow('No classical private key found');
    });
  });
});

describe('createHybridWallet', () => {
  it('should create a wallet instance', () => {
    const storage = createMockStorage();
    const wallet = createHybridWallet(storage);

    expect(wallet).toBeInstanceOf(HybridWallet);
    expect(wallet.generateKeyPair).toBeDefined();
    expect(wallet.sign).toBeDefined();
  });
});

describe('getHybridCryptoInfo', () => {
  it('should return size information', () => {
    const info = getHybridCryptoInfo();

    expect(info.publicKeySize).toBeGreaterThan(0);
    expect(info.secretKeySize).toBeGreaterThan(0);
    expect(info.signatureSize).toBeGreaterThan(0);
    expect(info.classicalPublicKeySize).toBe(32);
    expect(info.classicalSignatureSize).toBe(64);
  });
});
