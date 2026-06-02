/**
 * Queue Manager for Offline Operations
 *
 * Handles queuing and retrying operations when offline or on network errors.
 */

import { QueuedOperation, SecureStorage } from './types';

const QUEUE_STORAGE_KEY = 'icn_operation_queue';
const MAX_RETRIES = 3;
const INITIAL_RETRY_DELAY = 1000; // 1 second

export class QueueManager {
  private storage?: SecureStorage;
  private queue: QueuedOperation[] = [];
  private processing = false;
  private listeners: Set<(queue: QueuedOperation[]) => void> = new Set();
  // Bumped whenever the queue is emptied (purge/clear). An in-flight processQueue() run captures the
  // value at start and stops if it changes, so a run begun before an identity reset cannot keep
  // replaying or re-persisting the forgotten identity's operations afterwards.
  private generation = 0;
  // Serializes durable storage mutations (persist/purge) so a late persist() write cannot interleave
  // with — or land after — purge()'s removal and resurrect the queue.
  private writeChain: Promise<unknown> = Promise.resolve();

  constructor(storage?: SecureStorage) {
    this.storage = storage;
  }

  /**
   * Initialize and load persisted queue
   */
  async initialize(): Promise<void> {
    if (!this.storage) return;

    // Capture the generation before the (awaited) read; if a purge()/clear() runs while getItem is in
    // flight (e.g. an identity reset), the loaded result is stale and must be discarded so the
    // forgotten identity's queued operations cannot reappear in memory after the reset.
    const gen = this.generation;
    try {
      const stored = await this.storage.getItem(QUEUE_STORAGE_KEY);
      if (stored && gen === this.generation) {
        this.queue = JSON.parse(stored);
        this.notifyListeners();
      }
    } catch (error) {
      console.error('Failed to load queue:', error);
    }
  }

  /**
   * Add operation to queue
   */
  async enqueue(operation: Omit<QueuedOperation, 'id' | 'queuedAt' | 'retries' | 'status'>): Promise<string> {
    const id = `${Date.now()}-${Math.random().toString(36).substr(2, 9)}`;
    const queuedOp: QueuedOperation = {
      ...operation,
      id,
      queuedAt: Date.now(),
      retries: 0,
      status: 'pending',
    };

    this.queue.push(queuedOp);
    await this.persist();
    this.notifyListeners();

    return id;
  }

  /**
   * Get all queued operations
   */
  getQueue(): QueuedOperation[] {
    return [...this.queue];
  }

  /**
   * Get pending operations count
   */
  getPendingCount(): number {
    return this.queue.filter((op) => op.status === 'pending' || op.status === 'processing').length;
  }

  /**
   * Remove operation from queue
   */
  async remove(id: string): Promise<void> {
    this.queue = this.queue.filter((op) => op.id !== id);
    await this.persist();
    this.notifyListeners();
  }

  /**
   * Update operation status
   */
  async updateStatus(id: string, status: QueuedOperation['status'], error?: string): Promise<void> {
    const op = this.queue.find((o) => o.id === id);
    if (op) {
      op.status = status;
      op.error = error;
      await this.persist();
      this.notifyListeners();
    }
  }

  /**
   * Process queue with executor function
   */
  async processQueue(executor: (op: QueuedOperation) => Promise<void>): Promise<void> {
    if (this.processing) return;

    this.processing = true;
    // Capture the queue generation; if it changes (purge/clear, e.g. during an identity reset) this
    // run is stale and must stop without executing or persisting further operations.
    const gen = this.generation;

    try {
      const pending = this.queue.filter((op) => op.status === 'pending');

      for (const op of pending) {
        if (gen !== this.generation) break; // queue was emptied (reset) — stop replaying
        try {
          await this.updateStatus(op.id, 'processing');
          if (gen !== this.generation) break; // re-check before executing a forgotten operation
          await executor(op);
          if (gen !== this.generation) break; // re-check before durable removal
          await this.remove(op.id); // Success - remove from queue
        } catch (error) {
          op.retries += 1;

          if (op.retries >= MAX_RETRIES) {
            await this.updateStatus(op.id, 'failed', (error as Error).message);
          } else {
            // Exponential backoff
            const delay = INITIAL_RETRY_DELAY * Math.pow(2, op.retries);
            await new Promise((resolve) => setTimeout(resolve, delay));
            await this.updateStatus(op.id, 'pending');
          }
        }
      }
    } finally {
      this.processing = false;
    }
  }

  /**
   * Clear all failed operations
   */
  async clearFailed(): Promise<void> {
    this.queue = this.queue.filter((op) => op.status !== 'failed');
    await this.persist();
    this.notifyListeners();
  }

  /**
   * Clear entire queue
   */
  async clear(): Promise<void> {
    this.generation += 1; // fence any in-flight processQueue() run
    this.queue = [];
    await this.persist();
    this.notifyListeners();
  }

  /**
   * Strictly clear the queue, propagating any persistence failure.
   *
   * Unlike {@link clear} (best-effort: `persist()` swallows storage errors), `purge()` removes the
   * persisted queue and lets a storage failure reject, so sign-out-and-forget callers learn that the
   * persisted queue could not be removed and the reset did not fully complete.
   */
  async purge(): Promise<void> {
    this.generation += 1; // fence any in-flight processQueue() run
    this.queue = [];
    // Remove the persisted queue under the write lock (so a late persist() cannot resurrect it) and
    // BEFORE notifying listeners. Any storage failure propagates to the caller (strict reset).
    if (this.storage) {
      await this.runExclusive(() => this.storage!.removeItem(QUEUE_STORAGE_KEY));
    }
    this.notifyListeners();
  }

  /**
   * Listen to queue changes
   */
  onChange(listener: (queue: QueuedOperation[]) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /**
   * Run a durable storage mutation exclusively, serialized after any prior one. Returns the
   * operation's own result/rejection while keeping the chain alive regardless of outcome, so a
   * failed write never poisons subsequent operations.
   */
  private runExclusive<T>(fn: () => Promise<T>): Promise<T> {
    const result = this.writeChain.then(fn, fn);
    this.writeChain = result.then(
      () => undefined,
      () => undefined
    );
    return result;
  }

  private async persist(): Promise<void> {
    if (!this.storage) return;

    // Best-effort, but serialized: compute the snapshot INSIDE the lock so it reflects the live queue
    // at write time and cannot land out of order with a concurrent purge().
    await this.runExclusive(async () => {
      try {
        await this.storage!.setItem(QUEUE_STORAGE_KEY, JSON.stringify(this.queue));
      } catch (error) {
        console.error('Failed to persist queue:', error);
      }
    });
  }

  private notifyListeners(): void {
    this.listeners.forEach((listener) => {
      try {
        listener(this.getQueue());
      } catch (error) {
        // A misbehaving subscriber must not break queue operations (e.g. purge during identity reset).
        console.error('Queue change listener threw:', error);
      }
    });
  }
}
