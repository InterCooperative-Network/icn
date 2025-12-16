/**
 * Tests for Hybrid Post-Quantum Cryptography
 */

import {
  HybridCrypto,
  ML_DSA_SIZES,
  isHybridCryptoSupported,
  getHybridCryptoInfo,
} from './hybrid-crypto';

describe('HybridCrypto', () => {
  describe('generateKeyPair', () => {
    it('should generate a valid hybrid keypair', async () => {
      const keypair = await HybridCrypto.generateKeyPair();

      expect(keypair).toBeDefined();
      expect(keypair.publicKey).toBeDefined();
      expect(keypair.secretKey).toBeDefined();
      expect(keypair.did).toBeDefined();

      // Check version
      expect(keypair.publicKey.version).toBe(1);
      expect(keypair.secretKey.version).toBe(1);

      // Check key sizes (hex encoded, so double the byte size)
      expect(keypair.publicKey.classical.length).toBe(64); // 32 bytes
      expect(keypair.secretKey.classical.length).toBe(64); // 32 bytes
      expect(keypair.publicKey.pq.length).toBe(ML_DSA_SIZES.PUBLIC_KEY * 2);
      expect(keypair.secretKey.pq.length).toBe(ML_DSA_SIZES.SECRET_KEY * 2);

      // Check DID format
      expect(keypair.did).toMatch(/^did:icn:z/);
    });

    it('should generate unique keypairs', async () => {
      const keypair1 = await HybridCrypto.generateKeyPair();
      const keypair2 = await HybridCrypto.generateKeyPair();

      expect(keypair1.publicKey.classical).not.toBe(keypair2.publicKey.classical);
      expect(keypair1.publicKey.pq).not.toBe(keypair2.publicKey.pq);
      expect(keypair1.did).not.toBe(keypair2.did);
    });
  });

  describe('sign and verify', () => {
    let keypair: Awaited<ReturnType<typeof HybridCrypto.generateKeyPair>>;
    const testMessage = new TextEncoder().encode('Hello, quantum-safe world!');

    beforeAll(async () => {
      keypair = await HybridCrypto.generateKeyPair();
    });

    it('should sign and verify a message', async () => {
      const signature = await HybridCrypto.sign(testMessage, keypair);

      expect(signature).toBeDefined();
      expect(signature.classical.length).toBe(128); // 64 bytes hex
      expect(signature.pq.length).toBe(ML_DSA_SIZES.SIGNATURE * 2);
      expect(signature.version).toBe(1);

      const valid = await HybridCrypto.verify(testMessage, signature, keypair.publicKey);
      expect(valid).toBe(true);
    });

    it('should reject tampered messages', async () => {
      const signature = await HybridCrypto.sign(testMessage, keypair);

      const tamperedMessage = new TextEncoder().encode('Hello, tampered world!');
      const valid = await HybridCrypto.verify(tamperedMessage, signature, keypair.publicKey);
      expect(valid).toBe(false);
    });

    it('should reject tampered classical signatures', async () => {
      const signature = await HybridCrypto.sign(testMessage, keypair);

      const tamperedSig = {
        ...signature,
        classical: 'a'.repeat(128), // Invalid signature
      };

      const valid = await HybridCrypto.verify(testMessage, tamperedSig, keypair.publicKey);
      expect(valid).toBe(false);
    });

    it('should reject tampered PQ signatures', async () => {
      const signature = await HybridCrypto.sign(testMessage, keypair);

      const tamperedSig = {
        ...signature,
        pq: 'b'.repeat(ML_DSA_SIZES.SIGNATURE * 2), // Invalid PQ signature
      };

      const valid = await HybridCrypto.verify(testMessage, tamperedSig, keypair.publicKey);
      expect(valid).toBe(false);
    });

    it('should reject signatures from different keypairs', async () => {
      const otherKeypair = await HybridCrypto.generateKeyPair();
      const signature = await HybridCrypto.sign(testMessage, otherKeypair);

      const valid = await HybridCrypto.verify(testMessage, signature, keypair.publicKey);
      expect(valid).toBe(false);
    });
  });

  describe('classical-only signing', () => {
    it('should sign and verify with classical only', async () => {
      const keypair = await HybridCrypto.generateKeyPair();
      const testMessage = new TextEncoder().encode('Ephemeral message');

      // Sign with classical only (via options)
      const signature = await HybridCrypto.sign(testMessage, keypair, { includePQ: false });

      expect(signature.classical.length).toBe(128);
      expect(signature.pq.length).toBe(0); // Empty PQ signature

      // Should still verify (PQ check is skipped when empty)
      const valid = await HybridCrypto.verify(testMessage, signature, keypair.publicKey);
      expect(valid).toBe(true);
    });

    it('should work with signClassicalOnly method', async () => {
      const keypair = await HybridCrypto.generateKeyPair();
      const testMessage = new TextEncoder().encode('Short-lived proof');

      const signatureHex = await HybridCrypto.signClassicalOnly(testMessage, keypair);

      expect(signatureHex.length).toBe(128); // 64 bytes hex

      // Verify classical signature
      const valid = await HybridCrypto.verifyClassical(
        testMessage,
        signatureHex,
        keypair.publicKey.classical
      );
      expect(valid).toBe(true);
    });
  });

  describe('hex message signing', () => {
    it('should sign hex-encoded messages', async () => {
      const keypair = await HybridCrypto.generateKeyPair();
      const messageHex = '48656c6c6f'; // "Hello" in hex

      const signature = await HybridCrypto.signHex(messageHex, keypair);

      expect(signature).toBeDefined();
      expect(signature.classical.length).toBe(128);
    });
  });

  describe('serialization', () => {
    it('should serialize and deserialize public keys', async () => {
      const keypair = await HybridCrypto.generateKeyPair();

      const json = HybridCrypto.serializePublicKey(keypair.publicKey);
      const restored = HybridCrypto.deserializePublicKey(json);

      expect(restored.classical).toBe(keypair.publicKey.classical);
      expect(restored.pq).toBe(keypair.publicKey.pq);
      expect(restored.version).toBe(keypair.publicKey.version);
    });

    it('should serialize and deserialize signatures', async () => {
      const keypair = await HybridCrypto.generateKeyPair();
      const message = new TextEncoder().encode('Test');
      const signature = await HybridCrypto.sign(message, keypair);

      const json = HybridCrypto.serializeSignature(signature);
      const restored = HybridCrypto.deserializeSignature(json);

      expect(restored.classical).toBe(signature.classical);
      expect(restored.pq).toBe(signature.pq);
      expect(restored.version).toBe(signature.version);
    });

    it('should reject unsupported versions', () => {
      const invalidJson = JSON.stringify({ version: 999, classical: 'abc', pq: 'def' });

      expect(() => HybridCrypto.deserializePublicKey(invalidJson)).toThrow('Unsupported public key version');
      expect(() => HybridCrypto.deserializeSignature(invalidJson)).toThrow('Unsupported signature version');
    });
  });

  describe('size calculations', () => {
    it('should calculate public key size correctly', async () => {
      const keypair = await HybridCrypto.generateKeyPair();
      const size = HybridCrypto.publicKeySize(keypair.publicKey);

      // 32 bytes (Ed25519) + 1952 bytes (ML-DSA)
      expect(size).toBe(32 + ML_DSA_SIZES.PUBLIC_KEY);
    });

    it('should calculate signature size correctly', async () => {
      const keypair = await HybridCrypto.generateKeyPair();
      const message = new TextEncoder().encode('Test');
      const signature = await HybridCrypto.sign(message, keypair);

      const size = HybridCrypto.signatureSize(signature);

      // 64 bytes (Ed25519) + 3309 bytes (ML-DSA)
      expect(size).toBe(64 + ML_DSA_SIZES.SIGNATURE);
    });
  });

  describe('utility functions', () => {
    it('should check crypto support', () => {
      const supported = isHybridCryptoSupported();
      expect(typeof supported).toBe('boolean');
      expect(supported).toBe(true); // Should be supported in test environment
    });

    it('should return crypto info', () => {
      const info = getHybridCryptoInfo();

      expect(info.publicKeySize).toBe(32 + ML_DSA_SIZES.PUBLIC_KEY);
      expect(info.secretKeySize).toBe(32 + ML_DSA_SIZES.SECRET_KEY);
      expect(info.signatureSize).toBe(64 + ML_DSA_SIZES.SIGNATURE);
      expect(info.classicalPublicKeySize).toBe(32);
      expect(info.classicalSignatureSize).toBe(64);
    });
  });

  describe('bytesToHex and hexToBytes', () => {
    it('should round-trip correctly', () => {
      const original = new Uint8Array([0, 1, 127, 128, 255]);
      const hex = HybridCrypto.bytesToHex(original);
      const restored = HybridCrypto.hexToBytes(hex);

      expect(hex).toBe('00017f80ff');
      expect(Array.from(restored)).toEqual(Array.from(original));
    });

    it('should handle empty arrays', () => {
      const empty = new Uint8Array(0);
      const hex = HybridCrypto.bytesToHex(empty);
      const restored = HybridCrypto.hexToBytes(hex);

      expect(hex).toBe('');
      expect(restored.length).toBe(0);
    });

    it('should reject odd-length hex strings', () => {
      expect(() => HybridCrypto.hexToBytes('abc')).toThrow('even length');
    });
  });
});
