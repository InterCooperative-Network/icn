/**
 * Tests for QueueManager identity-reset concurrency hardening.
 *
 * Covers the guarantees added alongside ICNMobileClient.resetIdentity():
 * - purge() cannot be undone by a stale persist() write (serialized storage writes)
 * - an in-flight processQueue() run stops replaying once the queue is purged (generation fence)
 * - purge() propagates a storage removal failure (strict reset)
 * - normal enqueue/process behavior is unaffected (regression)
 */

import { QueueManager } from './queue-manager';
import { SecureStorage } from './types';

const QUEUE_KEY = 'icn_operation_queue';

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

function makeStorage(hooks?: {
  setItem?: () => Promise<void>;
  removeItem?: () => Promise<void>;
}): SecureStorage & { store: Map<string, string> } {
  const store = new Map<string, string>();
  return {
    store,
    async setItem(key: string, value: string): Promise<void> {
      if (hooks?.setItem) await hooks.setItem();
      store.set(key, value);
    },
    async getItem(key: string): Promise<string | null> {
      return store.get(key) ?? null;
    },
    async removeItem(key: string): Promise<void> {
      if (hooks?.removeItem) await hooks.removeItem();
      store.delete(key);
    },
    async hasItem(key: string): Promise<boolean> {
      return store.has(key);
    },
  };
}

describe('QueueManager identity-reset concurrency', () => {
  it('purge is not undone by a stale persist write (serialized storage writes)', async () => {
    const gate = deferred<void>();
    let firstSet = true;
    const storage = makeStorage({
      setItem: async () => {
        if (firstSet) {
          firstSet = false;
          await gate.promise; // hold the in-flight enqueue persist
        }
      },
    });
    const qm = new QueueManager(storage);

    const enqueueP = qm.enqueue({ type: 'vote', data: {} });
    const purgeP = qm.purge(); // its removeItem is serialized behind the gated setItem
    gate.resolve(); // release the stale persist
    await Promise.all([enqueueP, purgeP]);

    // The purge wins: no resurrected queue.
    expect(storage.store.has(QUEUE_KEY)).toBe(false);
    expect(qm.getQueue().length).toBe(0);
  });

  it('an in-flight processQueue run stops replaying after purge', async () => {
    const storage = makeStorage();
    const qm = new QueueManager(storage);
    await qm.enqueue({ type: 'vote', data: { n: 1 } });
    await qm.enqueue({ type: 'vote', data: { n: 2 } });

    const execGate = deferred<void>();
    let executed = 0;
    const procP = qm.processQueue(async () => {
      executed += 1;
      if (executed === 1) await execGate.promise; // park on the first operation
    });
    await tick(); // let processing reach the first parked executor

    await qm.purge(); // empties the queue + bumps the generation while op1 is in-flight
    execGate.resolve(); // let the stale op1 executor finish
    await procP;

    expect(executed).toBe(1); // the second op is never executed after purge
    expect(qm.getQueue().length).toBe(0);
    expect(storage.store.has(QUEUE_KEY)).toBe(false);
  });

  it('purge propagates a storage removal failure (strict reset)', async () => {
    const storage = makeStorage({
      removeItem: async () => {
        throw new Error('storage boom');
      },
    });
    const qm = new QueueManager(storage);
    await qm.enqueue({ type: 'vote', data: {} });

    await expect(qm.purge()).rejects.toThrow('storage boom');
  });

  it('initialize() discards a stale read if the queue is purged during the read', async () => {
    const store = new Map<string, string>();
    store.set(
      QUEUE_KEY,
      JSON.stringify([{ id: '1', type: 'vote', data: {}, queuedAt: 0, retries: 0, status: 'pending' }])
    );
    const gate = deferred<void>();
    const storage: SecureStorage & { store: Map<string, string> } = {
      store,
      async getItem(k: string): Promise<string | null> {
        if (k === QUEUE_KEY) await gate.promise; // hold the load in flight
        return store.get(k) ?? null;
      },
      async setItem(k: string, v: string): Promise<void> {
        store.set(k, v);
      },
      async removeItem(k: string): Promise<void> {
        store.delete(k);
      },
      async hasItem(k: string): Promise<boolean> {
        return store.has(k);
      },
    };
    const qm = new QueueManager(storage);

    const initP = qm.initialize(); // getItem gated; generation captured = 0
    await tick();
    await qm.purge(); // generation -> 1, queue emptied, storage removed
    gate.resolve(); // the stale read now resolves with the old queue JSON
    await initP;

    // The stale read is discarded by the generation fence; purge wins.
    expect(qm.getQueue().length).toBe(0);
    expect(storage.store.has(QUEUE_KEY)).toBe(false);
  });

  it('normal enqueue + processQueue still works (no regression)', async () => {
    const storage = makeStorage();
    const qm = new QueueManager(storage);
    await qm.enqueue({ type: 'vote', data: {} });
    expect(qm.getQueue().length).toBe(1);

    let ran = 0;
    await qm.processQueue(async () => {
      ran += 1;
    });

    expect(ran).toBe(1);
    expect(qm.getQueue().length).toBe(0); // removed after success
    expect(storage.store.get(QUEUE_KEY)).toBe('[]');
  });
});
