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
