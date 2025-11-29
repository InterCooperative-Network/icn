/**
 * Jest setup for React Native SDK tests
 */

import { __resetMockState } from './react';

// Reset React mock state before each test
beforeEach(() => {
  __resetMockState();
});

// Mock WebSocket for client tests
class MockWebSocket {
  url: string;
  readyState: number = 0;
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: string }) => void) | null = null;
  onerror: (() => void) | null = null;
  onclose: (() => void) | null = null;

  constructor(url: string) {
    this.url = url;
    // Simulate connection opening
    setTimeout(() => {
      this.readyState = 1;
      this.onopen?.();
    }, 0);
  }

  send(data: string): void {
    // Mock send
  }

  close(): void {
    this.readyState = 3;
    this.onclose?.();
  }
}

(global as unknown as { WebSocket: typeof MockWebSocket }).WebSocket = MockWebSocket;
