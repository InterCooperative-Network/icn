/**
 * Tests for canonical Keyring aliases (added in feat/rn-sdk-keyring-aliases).
 *
 * Proves the canonical Keyring names are exact, behavior-preserving aliases of the legacy
 * wallet-named exports: identical references, working key custody + signing, and unchanged
 * persisted secure-storage keys (i.e. no migration). Imports come from the source modules
 * (./wallet, ./hybrid-wallet, ./types) — matching the existing tests — to avoid pulling the
 * `@icn/client` workspace dependency through ./index.
 */

import {
  ICNWalletImpl,
  createWallet,
  ICNKeyringImpl,
  createKeyring,
} from './wallet';
import {
  HybridWallet,
  createHybridWallet,
  HybridKeyring,
  createHybridKeyring,
} from './hybrid-wallet';
import { SecureStorage, ICNWallet, ICNKeyring } from './types';

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

describe('Keyring aliases', () => {
  it('canonical names are the exact same references as the legacy wallet names', () => {
    expect(createKeyring).toBe(createWallet);
    expect(ICNKeyringImpl).toBe(ICNWalletImpl);
    expect(HybridKeyring).toBe(HybridWallet);
    expect(createHybridKeyring).toBe(createHybridWallet);
  });

  it('createKeyring produces a working Device Keyring (generate + sign)', async () => {
    const keyring: ICNKeyring = createKeyring(createMockStorage());
    const kp = await keyring.generateKeyPair();
    expect(kp.did).toMatch(/^did:icn:z/);
    const sig = await keyring.sign('00');
    expect(typeof sig).toBe('string');
    expect(sig.length).toBeGreaterThan(0);
  });

  it('legacy createWallet still works and shares the implementation class', async () => {
    const legacy: ICNWallet = createWallet(createMockStorage());
    expect(legacy).toBeInstanceOf(ICNKeyringImpl); // ICNKeyringImpl === ICNWalletImpl
    expect(await legacy.hasKeyPair()).toBe(false);
  });

  it('persisted secure-storage keys are unchanged (no migration)', async () => {
    const storage = createMockStorage();
    const keyring = createKeyring(storage);
    await keyring.generateKeyPair();
    // Canonical naming must not migrate or rename the legacy persisted keys.
    expect(storage.store.has('icn_wallet_private_key')).toBe(true);
    expect(storage.store.has('icn_wallet_public_key')).toBe(true);
    expect(storage.store.has('icn_wallet_did')).toBe(true);
  });

  it('createHybridKeyring produces a working hybrid Device Keyring', async () => {
    const hybridKeyring = createHybridKeyring(createMockStorage());
    expect(hybridKeyring).toBeInstanceOf(HybridWallet); // HybridKeyring === HybridWallet
    const info = await hybridKeyring.generateKeyPair();
    expect(info.did).toMatch(/^did:icn:z/);
  });
});
