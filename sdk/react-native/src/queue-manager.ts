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

  constructor(storage?: SecureStorage) {
    this.storage = storage;
  }

  /**
   * Initialize and load persisted queue
   */
  async initialize(): Promise<void> {
    if (!this.storage) return;

    try {
      const stored = await this.storage.getItem(QUEUE_STORAGE_KEY);
      if (stored) {
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

    try {
      const pending = this.queue.filter((op) => op.status === 'pending');

      for (const op of pending) {
        try {
          await this.updateStatus(op.id, 'processing');
          await executor(op);
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
    this.queue = [];
    await this.persist();
    this.notifyListeners();
  }

  /**
   * Listen to queue changes
   */
  onChange(listener: (queue: QueuedOperation[]) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  private async persist(): Promise<void> {
    if (!this.storage) return;

    try {
      await this.storage.setItem(QUEUE_STORAGE_KEY, JSON.stringify(this.queue));
    } catch (error) {
      console.error('Failed to persist queue:', error);
    }
  }

  private notifyListeners(): void {
    this.listeners.forEach((listener) => listener(this.getQueue()));
  }
}
