/**
 * Tests for ICN Mobile Client
 */

import { ICNMobileClient, createMobileClient } from './client';
import { SecureStorage, ICNWallet, KeyPair } from './types';

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
      storage.store.set('@icn/auth_token', 'test-token');
      storage.store.set('@icn/did', 'did:icn:test');
      storage.store.set('@icn/coop_id', 'test-coop');
      storage.store.set('@icn/expires_at', futureExpiry.toString());

      await client.initialize();

      expect(client.authState.isAuthenticated).toBe(true);
      expect(client.authState.did).toBe('did:icn:test');
      expect(client.authState.coopId).toBe('test-coop');
      expect(client.authState.expiresAt).toBe(futureExpiry);
    });

    it('should clear expired auth state', async () => {
      // Pre-populate storage with expired token
      const pastExpiry = Date.now() - 3600000; // 1 hour ago
      storage.store.set('@icn/auth_token', 'expired-token');
      storage.store.set('@icn/did', 'did:icn:test');
      storage.store.set('@icn/expires_at', pastExpiry.toString());

      await client.initialize();

      expect(client.authState.isAuthenticated).toBe(false);
      expect(storage.store.has('@icn/auth_token')).toBe(false);
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
      storage.store.set('@icn/auth_token', 'test-token');
      storage.store.set('@icn/did', 'did:icn:test');

      await client.logout();

      expect(storage.store.has('@icn/auth_token')).toBe(false);
      expect(storage.store.has('@icn/did')).toBe(false);
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
