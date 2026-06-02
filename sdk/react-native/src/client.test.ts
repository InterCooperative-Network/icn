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

    it('removes the persisted queue even if a queue-change listener throws', async () => {
      storage.store.set(
        'icn_operation_queue',
        JSON.stringify([
          { id: '1', type: 'vote', data: {}, queuedAt: Date.now(), retries: 0, status: 'pending' },
        ])
      );
      await client.initialize();
      client.onQueueChange(() => {
        throw new Error('listener boom');
      });

      // A throwing subscriber must not break reset or skip the persisted queue removal.
      await expect(client.resetIdentity()).resolves.toBeUndefined();
      expect(storage.store.has('icn_operation_queue')).toBe(false);
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

    it('clears the session even if keyring deleteKeyPair throws synchronously', async () => {
      const syncThrowKeyring = {
        ...createMockWallet(),
        deleteKeyPair(): Promise<void> {
          throw new Error('sync boom');
        },
      };
      const c = new ICNMobileClient({
        baseUrl: 'https://icn.example.org',
        keyring: syncThrowKeyring,
        storage,
      });
      storage.store.set('icn_auth_token', 'test-token');
      storage.store.set('icn_auth_did', 'did:icn:old');

      await expect(c.resetIdentity()).rejects.toThrow('sync boom');

      // A synchronous throw from the keyring must not skip auth/queue cleanup or state reset.
      expect(storage.store.has('icn_auth_token')).toBe(false);
      expect(storage.store.has('icn_auth_did')).toBe(false);
      expect(c.authState.isAuthenticated).toBe(false);
    });

    it('completes reset even if a connection-state listener throws', async () => {
      storage.store.set('icn_auth_token', 'test-token');
      storage.store.set('icn_auth_did', 'did:icn:old');
      storage.store.set('icn_expires_at', (Date.now() + 3600000).toString());
      await client.initialize();
      client.onConnectionStateChange(() => {
        throw new Error('ws listener boom');
      });

      // A throwing connection-state subscriber must not skip the auth-state reset.
      await expect(client.resetIdentity()).resolves.toBeUndefined();
      expect(client.authState.isAuthenticated).toBe(false);
      expect(storage.store.has('icn_auth_token')).toBe(false);
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

  describe('identity reset concurrency', () => {
    function deferred<T>() {
      let resolve!: (v: T) => void;
      let reject!: (e: unknown) => void;
      const promise = new Promise<T>((res, rej) => {
        resolve = res;
        reject = rej;
      });
      return { promise, resolve, reject };
    }
    const tick = () => new Promise<void>((resolve) => setTimeout(resolve, 0));

    it('stale login cannot restore auth after reset', async () => {
      await wallet.generateKeyPair();
      const authStates: boolean[] = [];
      client.onAuthStateChange((st) => authStates.push(st.isAuthenticated));

      const d = deferred<{ token: string; expires_at: number }>();
      jest.spyOn(client as any, 'authenticate').mockReturnValue(d.promise);

      const loginPromise = client.login('coop-1');
      await tick(); // let login park at the authenticate() await

      await client.resetIdentity();

      // Resolve the now-superseded authentication response.
      d.resolve({ token: 'stale-token', expires_at: Date.now() + 3600000 });
      const result = await loginPromise;

      expect(result.isAuthenticated).toBe(false);
      expect(client.authState.isAuthenticated).toBe(false);
      expect(storage.store.has('icn_auth_token')).toBe(false);
      expect(storage.store.has('icn_auth_did')).toBe(false);
      // No authenticated state was ever published after the reset.
      expect(authStates).not.toContain(true);
    });

    it('stale completeEnrollment cannot restore identity after reset', async () => {
      const authStates: boolean[] = [];
      client.onAuthStateChange((st) => authStates.push(st.isAuthenticated));

      const d = deferred<{ ok: boolean; json: () => Promise<unknown>; text: () => Promise<string> }>();
      const originalFetch = (globalThis as any).fetch;
      (globalThis as any).fetch = jest.fn().mockReturnValue(d.promise);
      try {
        const enrollPromise = client.completeEnrollment(
          'enroll-1',
          'did:icn:ephemeral',
          'sig',
          { os: 'ios' } as any
        );
        await tick();

        await client.resetIdentity();

        d.resolve({
          ok: true,
          json: async () => ({ auth_token: 'Bearer stale', did: 'did:icn:old' }),
          text: async () => '',
        });
        const result = (await enrollPromise) as { did: string };

        // The server result is still returned, but no local auth/session is restored.
        expect(result.did).toBe('did:icn:old');
        expect(client.authState.isAuthenticated).toBe(false);
        expect(storage.store.has('icn_auth_token')).toBe(false);
        expect(authStates).not.toContain(true);
      } finally {
        (globalThis as any).fetch = originalFetch;
      }
    });

    it('normal login still authenticates (no false fencing)', async () => {
      await wallet.generateKeyPair();
      jest
        .spyOn(client as any, 'authenticate')
        .mockResolvedValue({ token: 'tok', expires_at: Date.now() + 3600000 });

      const result = await client.login('coop-1');

      expect(result.isAuthenticated).toBe(true);
      expect(result.did).toBe('did:icn:mock123');
      expect(storage.store.get('icn_auth_token')).toBe('tok');
    });

    it('clears the inherited token that a stale authenticate restored after reset', async () => {
      await wallet.generateKeyPair();
      const d = deferred<{ token: string; expires_at: number }>();
      // The real base ICNClient.authenticate() calls setToken() before resolving; mimic that so the
      // forgotten identity's JWT is set on the base/WASM client during the stale login.
      jest.spyOn(client as any, 'authenticate').mockImplementation(() =>
        d.promise.then((auth: { token: string; expires_at: number }) => {
          (client as any).setToken(auth.token, auth.expires_at);
          return auth;
        })
      );

      const loginPromise = client.login('coop-1');
      await tick();
      await client.resetIdentity();
      d.resolve({ token: 'stale-jwt', expires_at: Date.now() + 3600000 });
      await loginPromise;

      expect(client.authState.isAuthenticated).toBe(false);
      // The token authenticate() restored after the reset must be cleared from the base client.
      expect((client as any).hasToken()).toBe(false);
      expect(storage.store.has('icn_auth_token')).toBe(false);
    });

    it('stale persistAuth cannot resurrect auth after reset (serialized auth writes)', async () => {
      await wallet.generateKeyPair();
      const gate = deferred<void>();
      let gateOnce = true;
      const base = createMockStorage();
      // First setItem is gated, so login's persistAuth holds the auth write lock while the reset's
      // clearAuth is forced to wait behind it.
      const gatedStorage: SecureStorage & { store: Map<string, string> } = {
        store: base.store,
        getItem: base.getItem,
        hasItem: base.hasItem,
        removeItem: base.removeItem,
        async setItem(k: string, v: string): Promise<void> {
          if (gateOnce) {
            gateOnce = false;
            await gate.promise;
          }
          base.store.set(k, v);
        },
      };
      const c = new ICNMobileClient({
        baseUrl: 'https://icn.example.org',
        wallet,
        storage: gatedStorage,
      });
      jest
        .spyOn(c as any, 'authenticate')
        .mockResolvedValue({ token: 'tok', expires_at: Date.now() + 3600000 });

      const loginPromise = c.login('coop-1'); // parks inside persistAuth at the gated setItem
      await tick();
      const resetPromise = c.resetIdentity(); // clearAuth serialized behind persistAuth
      gate.resolve();
      await Promise.all([loginPromise, resetPromise]);

      // clearAuth wins (serialized after persistAuth): no resurrected auth, not authenticated.
      expect(gatedStorage.store.has('icn_auth_token')).toBe(false);
      expect(gatedStorage.store.has('icn_auth_did')).toBe(false);
      expect(c.authState.isAuthenticated).toBe(false);
    });

    it('a partially-failed persistAuth batch cannot resurrect auth after reset (allSettled barrier)', async () => {
      await wallet.generateKeyPair();
      // DID write rejects immediately; TOKEN write is slow (gated). The allSettled barrier must keep
      // the auth-write lock held until BOTH settle, so the reset's clearAuth runs strictly afterward
      // and the slow token write cannot land after it.
      const tokenGate = deferred<void>();
      const base = createMockStorage();
      const gatedStorage: SecureStorage & { store: Map<string, string> } = {
        store: base.store,
        getItem: base.getItem,
        hasItem: base.hasItem,
        removeItem: base.removeItem,
        async setItem(k: string, v: string): Promise<void> {
          if (k === 'icn_auth_did') {
            throw new Error('did write failed');
          }
          if (k === 'icn_auth_token') {
            await tokenGate.promise; // straggler write that must not outlive the lock
          }
          base.store.set(k, v);
        },
      };
      const c = new ICNMobileClient({
        baseUrl: 'https://icn.example.org',
        wallet,
        storage: gatedStorage,
      });
      jest
        .spyOn(c as any, 'authenticate')
        .mockResolvedValue({ token: 'tok', expires_at: Date.now() + 3600000 });

      const loginPromise = c.login('coop-1'); // parks in persistAuth holding the auth-write lock
      await tick();
      const resetPromise = c.resetIdentity(); // clearAuth queued strictly behind persistAuth
      tokenGate.resolve(); // release the slow token write
      await Promise.allSettled([loginPromise, resetPromise]);

      // The straggler token write completed inside the lock; clearAuth then removed it. No resurrection.
      expect(gatedStorage.store.has('icn_auth_token')).toBe(false);
      expect(gatedStorage.store.has('icn_auth_did')).toBe(false);
      expect(c.authState.isAuthenticated).toBe(false);
    });

    it('a synchronous storage throw cannot resurrect auth after reset (sync-throw normalized)', async () => {
      await wallet.generateKeyPair();
      // DID write throws SYNCHRONOUSLY; TOKEN write is slow (gated). The async-wrapped allSettled
      // barrier must still await the in-flight token write before releasing the lock.
      const tokenGate = deferred<void>();
      const base = createMockStorage();
      const gatedStorage: SecureStorage & { store: Map<string, string> } = {
        store: base.store,
        getItem: base.getItem,
        hasItem: base.hasItem,
        removeItem: base.removeItem,
        // Intentionally NOT async so it can throw synchronously, like a misbehaving native adapter.
        setItem(k: string, v: string): Promise<void> {
          if (k === 'icn_auth_did') {
            throw new Error('sync did write failed');
          }
          if (k === 'icn_auth_token') {
            return tokenGate.promise.then(() => {
              base.store.set(k, v);
            });
          }
          base.store.set(k, v);
          return Promise.resolve();
        },
      };
      const c = new ICNMobileClient({
        baseUrl: 'https://icn.example.org',
        wallet,
        storage: gatedStorage,
      });
      jest
        .spyOn(c as any, 'authenticate')
        .mockResolvedValue({ token: 'tok', expires_at: Date.now() + 3600000 });

      const loginPromise = c.login('coop-1');
      await tick();
      const resetPromise = c.resetIdentity();
      tokenGate.resolve();
      await Promise.allSettled([loginPromise, resetPromise]);

      expect(gatedStorage.store.has('icn_auth_token')).toBe(false);
      expect(c.authState.isAuthenticated).toBe(false);
    });

    it('initialize() running during a reset cannot restore a token mid-removal', async () => {
      // A valid persisted session exists; resetIdentity()'s clearAuth removals are gated so they are
      // still in flight while initialize() runs concurrently.
      const removeGate = deferred<void>();
      let removeGateOnce = true;
      const base = createMockStorage();
      base.store.set('icn_auth_token', 'old-token');
      base.store.set('icn_auth_did', 'did:icn:old');
      base.store.set('icn_expires_at', (Date.now() + 3600000).toString());
      const gatedStorage: SecureStorage & { store: Map<string, string> } = {
        store: base.store,
        getItem: base.getItem,
        setItem: base.setItem,
        hasItem: base.hasItem,
        async removeItem(k: string): Promise<void> {
          if (removeGateOnce && k === 'icn_auth_token') {
            removeGateOnce = false;
            await removeGate.promise;
          }
          base.store.delete(k);
        },
      };
      const c = new ICNMobileClient({
        baseUrl: 'https://icn.example.org',
        wallet,
        storage: gatedStorage,
      });

      const resetPromise = c.resetIdentity(); // gen++, clearAuth removals gated (auth lock held)
      const initPromise = c.initialize(); // its serialized auth read is queued behind clearAuth
      removeGate.resolve();
      await Promise.all([resetPromise, initPromise]);

      // initialize read after clearAuth completed -> empty -> nothing restored.
      expect(c.authState.isAuthenticated).toBe(false);
      expect(gatedStorage.store.has('icn_auth_token')).toBe(false);
      expect((c as any).hasToken()).toBe(false);
    });

    it('a superseded initialize does not clear auth in its expiry branch', async () => {
      // initialize() reads an EXPIRED old session; its read is gated so a reset can bump the
      // generation before initialize reaches the expiry branch.
      const readGate = deferred<void>();
      let readGateOnce = true;
      const base = createMockStorage();
      base.store.set('icn_auth_token', 'old-expired');
      base.store.set('icn_auth_did', 'did:icn:old');
      base.store.set('icn_expires_at', (Date.now() - 1000).toString()); // already expired
      const gatedStorage: SecureStorage & { store: Map<string, string> } = {
        store: base.store,
        setItem: base.setItem,
        hasItem: base.hasItem,
        removeItem: base.removeItem,
        async getItem(k: string): Promise<string | null> {
          const snapshot = base.store.get(k) ?? null; // value at read time (the old expired session)
          if (readGateOnce && k === 'icn_auth_token') {
            readGateOnce = false;
            await readGate.promise;
          }
          return snapshot;
        },
      };
      const c = new ICNMobileClient({
        baseUrl: 'https://icn.example.org',
        wallet,
        storage: gatedStorage,
      });
      const clearAuthSpy = jest.spyOn(c as any, 'clearAuth');

      const initPromise = c.initialize(); // read gated; generation captured before reset
      await tick();
      const resetPromise = c.resetIdentity(); // gen++; its own clearAuth is the only legitimate one
      readGate.resolve();
      await Promise.all([initPromise, resetPromise]);

      // resetIdentity legitimately calls clearAuth once; the superseded initialize must NOT call it
      // again from its expiry branch (which would clobber any replacement session).
      expect(clearAuthSpy).toHaveBeenCalledTimes(1);
      expect(c.authState.isAuthenticated).toBe(false);
    });
  });
});

describe('login concurrency (auth-attempt fence)', () => {
  type AuthResp = { token: string; expires_at: number };
  function deferred<T>() {
    let resolve!: (value: T) => void;
    let reject!: (reason?: unknown) => void;
    const promise = new Promise<T>((res, rej) => {
      resolve = res;
      reject = rej;
    });
    return { promise, resolve, reject };
  }
  const tick = () => new Promise<void>((resolve) => setTimeout(resolve, 0));
  const signer = { sign: async (msg: string) => 'sig-' + msg };

  // Mimics the base ICNClient.authenticate() generation guard (the fix in this PR): it claims the
  // base auth-generation up front and commits the token (applyToken) only if still current, so only
  // the newest authenticate() wins. Drives realistic base behavior with a gated, per-DID verify
  // response instead of a live gateway.
  function mockBaseAuthenticate(
    client: ICNMobileClient,
    responseByDid: Record<string, Promise<AuthResp>>
  ) {
    jest.spyOn(client as any, 'authenticate').mockImplementation(async (did: unknown) => {
      const c = client as any;
      const generation = (c.authGeneration += 1);
      const auth = await responseByDid[did as string];
      if (c.authGeneration === generation) {
        c.applyToken(auth.token, auth.expires_at);
      }
      return auth;
    });
  }

  it('overlapping logins: a stale login cannot publish identity A while the base token is B', async () => {
    const storage = createMockStorage();
    const client = new ICNMobileClient({ baseUrl: 'https://icn.example.org', storage });
    const aResp = deferred<AuthResp>();
    const bResp = deferred<AuthResp>();
    mockBaseAuthenticate(client, { 'did:icn:A': aResp.promise, 'did:icn:B': bResp.promise });

    const pA = client.loginWithSignature('did:icn:A', signer); // older attempt
    const pB = client.loginWithSignature('did:icn:B', signer); // newer attempt
    await tick();

    // B (newer) resolves first and publishes.
    bResp.resolve({ token: 'token-B', expires_at: Date.now() + 3600000 });
    await pB;
    expect(client.authState.did).toBe('did:icn:B');
    expect((client as any).token).toBe('token-B');

    // A (older) resolves late: base keeps B, and the wrapper must NOT publish A over B.
    aResp.resolve({ token: 'token-A', expires_at: Date.now() + 3600000 });
    await pA;
    expect(client.authState.did).toBe('did:icn:B');
    expect((client as any).token).toBe('token-B');
    expect(storage.store.get('icn_auth_did')).toBe('did:icn:B');
  });

  it('a reset-stale login cleanup cannot cancel a replacement login', async () => {
    const wallet = createMockWallet();
    await wallet.generateKeyPair();
    const storage = createMockStorage();
    const client = new ICNMobileClient({ baseUrl: 'https://icn.example.org', wallet, storage });
    const aResp = deferred<AuthResp>();
    const bResp = deferred<AuthResp>();
    mockBaseAuthenticate(client, { 'did:icn:A': aResp.promise, 'did:icn:B': bResp.promise });

    const pA = client.loginWithSignature('did:icn:A', signer); // parks in the mocked authenticate
    await tick();
    await client.resetIdentity(); // bumps identity gen + base clearToken
    const pB = client.loginWithSignature('did:icn:B', signer); // replacement login, newer attempt
    await tick();

    // The old login resolves: it is reset-stale, but a newer login exists, so it must NOT call base
    // clearToken() (which would bump the base auth-generation and cancel B).
    aResp.resolve({ token: 'token-A', expires_at: Date.now() + 3600000 });
    await pA;

    // The replacement login resolves and must end authenticated with its own token intact.
    bResp.resolve({ token: 'token-B', expires_at: Date.now() + 3600000 });
    await pB;

    expect(client.authState.isAuthenticated).toBe(true);
    expect(client.authState.did).toBe('did:icn:B');
    expect((client as any).hasToken()).toBe(true);
    expect((client as any).token).toBe('token-B');
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
