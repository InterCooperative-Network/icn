/**
 * Tests for ICN Mobile Client
 */

import { ICNMobileClient, createMobileClient } from './client';
import { SecureStorage, ICNWallet, ICNKeyring, KeyPair } from './types';

// Mock secure storage
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

// Mock wallet
function createMockWallet(): ICNWallet & { keyPair: KeyPair | null } {
  let keyPair: KeyPair | null = null;
  return {
    get keyPair() { return keyPair; },
    async generateKeyPair(): Promise<KeyPair> {
      keyPair = {
        publicKey: 'mock-public-key',
        did: 'did:icn:mock123',
      };
      return keyPair;
    },
    async importKeyPair(privateKey: string): Promise<KeyPair> {
      keyPair = {
        publicKey: 'imported-public-key',
        did: 'did:icn:imported123',
      };
      return keyPair;
    },
    async getKeyPair(): Promise<KeyPair | null> {
      return keyPair;
    },
    async deleteKeyPair(): Promise<void> {
      keyPair = null;
    },
    async sign(message: string): Promise<string> {
      return 'mock-signature-' + message.substring(0, 10);
    },
    async hasKeyPair(): Promise<boolean> {
      return keyPair !== null;
    },
  };
}

describe('ICNMobileClient', () => {
  let storage: ReturnType<typeof createMockStorage>;
  let wallet: ReturnType<typeof createMockWallet>;
  let client: ICNMobileClient;

  beforeEach(() => {
    storage = createMockStorage();
    wallet = createMockWallet();
    client = new ICNMobileClient({
      baseUrl: 'https://icn.example.org',
      wallet,
      storage,
    });
  });

  describe('constructor', () => {
    it('should create a client with all options', () => {
      const client = new ICNMobileClient({
        baseUrl: 'https://icn.example.org',
        wallet,
        storage,
        timeout: 30000,
      });

      expect(client).toBeDefined();
    });

    it('should create a client without wallet', () => {
      const client = new ICNMobileClient({
        baseUrl: 'https://icn.example.org',
        storage,
      });

      expect(client).toBeDefined();
    });

    it('should create a client without storage', () => {
      const client = new ICNMobileClient({
        baseUrl: 'https://icn.example.org',
        wallet,
      });

      expect(client).toBeDefined();
    });
  });

  describe('authState', () => {
    it('should start with unauthenticated state', () => {
      const state = client.authState;

      expect(state.isAuthenticated).toBe(false);
      expect(state.did).toBeNull();
      expect(state.coopId).toBeNull();
      expect(state.expiresAt).toBeNull();
    });

    it('should return a copy of auth state', () => {
      const state1 = client.authState;
      const state2 = client.authState;

      expect(state1).not.toBe(state2);
      expect(state1).toEqual(state2);
    });
  });

  describe('connectionState', () => {
    it('should start disconnected', () => {
      expect(client.connectionState).toBe('disconnected');
    });
  });

  describe('initialize', () => {
    it('should do nothing without storage', async () => {
      const clientNoStorage = new ICNMobileClient({
        baseUrl: 'https://icn.example.org',
      });

      await clientNoStorage.initialize();

      expect(clientNoStorage.authState.isAuthenticated).toBe(false);
    });

    it('should load persisted auth state', async () => {
      // Pre-populate storage
      const futureExpiry = Date.now() + 3600000; // 1 hour from now
      storage.store.set('icn_auth_token', 'test-token');
      storage.store.set('icn_auth_did', 'did:icn:test');
      storage.store.set('icn_coop_id', 'test-coop');
      storage.store.set('icn_expires_at', futureExpiry.toString());

      await client.initialize();

      expect(client.authState.isAuthenticated).toBe(true);
      expect(client.authState.did).toBe('did:icn:test');
      expect(client.authState.coopId).toBe('test-coop');
      expect(client.authState.expiresAt).toBe(futureExpiry);
    });

    it('should clear expired auth state', async () => {
      // Pre-populate storage with expired token
      const pastExpiry = Date.now() - 3600000; // 1 hour ago
      storage.store.set('icn_auth_token', 'expired-token');
      storage.store.set('icn_auth_did', 'did:icn:test');
      storage.store.set('icn_expires_at', pastExpiry.toString());

      await client.initialize();

      expect(client.authState.isAuthenticated).toBe(false);
      expect(storage.store.has('icn_auth_token')).toBe(false);
    });
  });

  describe('onAuthStateChange', () => {
    it('should notify listeners of auth state changes', async () => {
      const listener = jest.fn();
      client.onAuthStateChange(listener);

      // Manually trigger a state change by calling logout
      await client.logout();

      expect(listener).toHaveBeenCalled();
    });

    it('should return unsubscribe function', () => {
      const listener = jest.fn();
      const unsubscribe = client.onAuthStateChange(listener);

      expect(typeof unsubscribe).toBe('function');
    });
  });

  describe('logout', () => {
    it('should clear auth state', async () => {
      // Set up auth state first
      storage.store.set('@icn/auth_token', 'test-token');
      storage.store.set('@icn/did', 'did:icn:test');
      await client.initialize();

      await client.logout();

      expect(client.authState.isAuthenticated).toBe(false);
      expect(client.authState.did).toBeNull();
    });

    it('should clear storage', async () => {
      storage.store.set('icn_auth_token', 'test-token');
      storage.store.set('icn_auth_did', 'did:icn:test');

      await client.logout();

      expect(storage.store.has('icn_auth_token')).toBe(false);
      expect(storage.store.has('icn_auth_did')).toBe(false);
    });
  });

  describe('resetIdentity', () => {
    it('should clear persisted auth state and purge the device keyring', async () => {
      // Persisted auth bound to an old DID, plus a generated keyring key pair.
      storage.store.set('icn_auth_token', 'test-token');
      storage.store.set('icn_auth_did', 'did:icn:old');
      storage.store.set('icn_coop_id', 'test-coop');
      storage.store.set('icn_expires_at', (Date.now() + 3600000).toString());
      await wallet.generateKeyPair();
      expect(await wallet.hasKeyPair()).toBe(true);

      await client.resetIdentity();

      // All identity-bound auth keys are cleared so a new keyring DID is not shadowed.
      expect(storage.store.has('icn_auth_token')).toBe(false);
      expect(storage.store.has('icn_auth_did')).toBe(false);
      expect(storage.store.has('icn_coop_id')).toBe(false);
      expect(storage.store.has('icn_expires_at')).toBe(false);
      // The configured Device Keyring is purged.
      expect(await wallet.hasKeyPair()).toBe(false);
      // Auth state is reset.
      expect(client.authState.isAuthenticated).toBe(false);
      expect(client.authState.did).toBeNull();
    });

    it('should not throw when no keyring is configured', async () => {
      const clientNoWallet = new ICNMobileClient({
        baseUrl: 'https://icn.example.org',
        storage,
      });
      await expect(clientNoWallet.resetIdentity()).resolves.toBeUndefined();
    });

    it('should clear the offline operation queue', async () => {
      storage.store.set(
        'icn_operation_queue',
        JSON.stringify([
          { id: '1', type: 'vote', data: {}, queuedAt: Date.now(), retries: 0, status: 'pending' },
        ])
      );
      await client.initialize();
      expect(client.queue.length).toBe(1);

      await client.resetIdentity();

      expect(client.queue.length).toBe(0);
    });

    it('should clear auth and queue even if keyring deletion fails', async () => {
      const failingKeyring = {
        ...createMockWallet(),
        async deleteKeyPair(): Promise<void> {
          throw new Error('secure storage unavailable');
        },
      };
      const failClient = new ICNMobileClient({
        baseUrl: 'https://icn.example.org',
        keyring: failingKeyring,
        storage,
      });
      storage.store.set('icn_auth_token', 'test-token');
      storage.store.set('icn_auth_did', 'did:icn:old');

      await expect(failClient.resetIdentity()).rejects.toThrow('secure storage unavailable');

      // Session cleanup still ran despite the keyring deletion failure.
      expect(storage.store.has('icn_auth_token')).toBe(false);
      expect(storage.store.has('icn_auth_did')).toBe(false);
      expect(failClient.authState.isAuthenticated).toBe(false);
    });

    it('still invalidates the in-memory session if auth storage removal rejects', async () => {
      const base = createMockStorage();
      const failingStorage: SecureStorage & { store: Map<string, string> } = {
        store: base.store,
        setItem: base.setItem,
        getItem: base.getItem,
        hasItem: base.hasItem,
        async removeItem(): Promise<void> {
          throw new Error('keychain unavailable');
        },
      };
      const c = new ICNMobileClient({
        baseUrl: 'https://icn.example.org',
        wallet,
        storage: failingStorage,
      });
      failingStorage.store.set('icn_auth_token', 'test-token');
      failingStorage.store.set('icn_auth_did', 'did:icn:old');
      failingStorage.store.set('icn_expires_at', (Date.now() + 3600000).toString());
      await c.initialize();
      expect(c.authState.isAuthenticated).toBe(true);

      await expect(c.resetIdentity()).rejects.toThrow('keychain unavailable');

      // The in-memory session is invalidated even though persisted removal failed.
      expect(c.authState.isAuthenticated).toBe(false);
      expect(c.authState.did).toBeNull();
    });

    it('surfaces a queue persistence failure during reset', async () => {
      const base = createMockStorage();
      const failingStorage: SecureStorage & { store: Map<string, string> } = {
        store: base.store,
        setItem: base.setItem,
        getItem: base.getItem,
        hasItem: base.hasItem,
        async removeItem(key: string): Promise<void> {
          if (key === 'icn_operation_queue') {
            throw new Error('queue storage unavailable');
          }
          base.store.delete(key);
        },
      };
      const c = new ICNMobileClient({
        baseUrl: 'https://icn.example.org',
        wallet,
        storage: failingStorage,
      });

      // The reset must reject so the caller knows sign-out-and-forget did not fully complete.
      await expect(c.resetIdentity()).rejects.toThrow('queue storage unavailable');
      // The in-memory session is still invalidated.
      expect(c.authState.isAuthenticated).toBe(false);
    });
  });

  describe('connectRealtime', () => {
    it('should throw without coop ID', () => {
      expect(() => client.connectRealtime()).toThrow('No coop ID provided');
    });

    it('should connect with provided coop ID', () => {
      // This will create a WebSocket connection (mocked in setup)
      expect(() => client.connectRealtime('test-coop')).not.toThrow();
    });
  });

  describe('disconnectRealtime', () => {
    it('should disconnect without error', () => {
      expect(() => client.disconnectRealtime()).not.toThrow();
    });
  });

  describe('onEvent', () => {
    it('should return unsubscribe function', () => {
      const unsubscribe = client.onEvent('PaymentCreated', () => {});

      expect(typeof unsubscribe).toBe('function');
    });

    it('should allow multiple subscriptions to same event', () => {
      const listener1 = jest.fn();
      const listener2 = jest.fn();

      client.onEvent('PaymentCreated', listener1);
      client.onEvent('PaymentCreated', listener2);

      // Both should be registered (no throw)
    });
  });

  describe('onAnyEvent', () => {
    it('should return unsubscribe function', () => {
      const unsubscribe = client.onAnyEvent(() => {});

      expect(typeof unsubscribe).toBe('function');
    });
  });

  describe('onConnectionStateChange', () => {
    it('should return unsubscribe function', () => {
      const unsubscribe = client.onConnectionStateChange(() => {});

      expect(typeof unsubscribe).toBe('function');
    });
  });
});

describe('createMobileClient', () => {
  it('should create a mobile client instance', () => {
    const client = createMobileClient({
      baseUrl: 'https://icn.example.org',
    });

    expect(client).toBeInstanceOf(ICNMobileClient);
  });
});

describe('keyring option (canonical alias for legacy wallet)', () => {
  // A Device Keyring whose sign()/getKeyPair() outputs are tagged, so a test can prove
  // which configured keyring the client actually used. createCompletionSignature() and
  // createDeviceProof() exercise only local key custody + signing (no network).
  function createTaggedKeyring(tag: string): ICNKeyring {
    const keyPair: KeyPair = { publicKey: `${tag}-pub`, did: `did:icn:${tag}` };
    return {
      async generateKeyPair(): Promise<KeyPair> {
        return keyPair;
      },
      async importKeyPair(_privateKey: string): Promise<KeyPair> {
        return keyPair;
      },
      async getKeyPair(): Promise<KeyPair | null> {
        return keyPair;
      },
      async deleteKeyPair(): Promise<void> {},
      async sign(message: string): Promise<string> {
        return `${tag}:${message}`;
      },
      async hasKeyPair(): Promise<boolean> {
        return true;
      },
    };
  }

  it('accepts a keyring option and uses it for signing', async () => {
    const client = new ICNMobileClient({
      baseUrl: 'https://icn.example.org',
      keyring: createTaggedKeyring('keyring'),
    });
    const sig = await client.createCompletionSignature('enroll-1');
    expect(sig).toBe('keyring:complete:enroll-1');
  });

  it('keyring takes precedence over wallet when both are provided', async () => {
    const client = new ICNMobileClient({
      baseUrl: 'https://icn.example.org',
      keyring: createTaggedKeyring('keyring'),
      wallet: createTaggedKeyring('wallet'),
    });
    const sig = await client.createCompletionSignature('enroll-2');
    expect(sig).toBe('keyring:complete:enroll-2');
  });

  it('falls back to the legacy wallet option when no keyring is provided', async () => {
    const client = new ICNMobileClient({
      baseUrl: 'https://icn.example.org',
      wallet: createTaggedKeyring('wallet'),
    });
    const sig = await client.createCompletionSignature('enroll-3');
    expect(sig).toBe('wallet:complete:enroll-3');
  });

  it('throws when neither keyring nor wallet is configured', async () => {
    const client = new ICNMobileClient({
      baseUrl: 'https://icn.example.org',
    });
    await expect(client.createCompletionSignature('enroll-4')).rejects.toThrow(
      'No wallet configured'
    );
  });

  it('uses the keyring for device-proof key custody (getKeyPair + sign)', async () => {
    const client = new ICNMobileClient({
      baseUrl: 'https://icn.example.org',
      keyring: createTaggedKeyring('keyring'),
    });
    const proof = await client.createDeviceProof('enroll-5');
    expect(proof.ephemeral_did).toBe('did:icn:keyring');
    expect(proof.signature).toBe('keyring:enroll-5');
  });
});
