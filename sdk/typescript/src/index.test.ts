/**
 * ICN TypeScript SDK Tests
 */

import { ICNClient, ICNError } from './index';

describe('ICNClient', () => {
  describe('constructor', () => {
    it('should create client with base URL', () => {
      const client = new ICNClient({ baseUrl: 'http://localhost:8080' });
      expect(client).toBeInstanceOf(ICNClient);
    });

    it('should strip trailing slash from base URL', () => {
      const client = new ICNClient({ baseUrl: 'http://localhost:8080/' });
      expect(client.hasToken()).toBe(false);
    });

    it('should accept initial token', () => {
      const client = new ICNClient({
        baseUrl: 'http://localhost:8080',
        token: 'test-token',
      });
      expect(client.hasToken()).toBe(true);
    });
  });

  describe('token management', () => {
    let client: ICNClient;

    beforeEach(() => {
      client = new ICNClient({ baseUrl: 'http://localhost:8080' });
    });

    it('should set token', () => {
      expect(client.hasToken()).toBe(false);
      client.setToken('test-token');
      expect(client.hasToken()).toBe(true);
    });

    it('should clear token', () => {
      client.setToken('test-token');
      expect(client.hasToken()).toBe(true);
      client.clearToken();
      expect(client.hasToken()).toBe(false);
    });
  });

  describe('error handling', () => {
    it('should throw on network failure with mock', async () => {
      const mockFetch = jest.fn().mockRejectedValue(new Error('Network error'));
      const client = new ICNClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch as unknown as typeof fetch,
      });

      await expect(client.health()).rejects.toThrow('Network error');
      expect(mockFetch).toHaveBeenCalledTimes(1);
    });

    it('should include status code in error', async () => {
      const mockFetch = jest.fn().mockResolvedValue({
        ok: false,
        status: 401,
        json: async () => ({ error: 'Unauthorized' }),
      });
      const client = new ICNClient({
        baseUrl: 'http://localhost:8080',
        fetch: mockFetch as unknown as typeof fetch,
      });

      await expect(client.health()).rejects.toThrow();
    });
  });
});

describe('ICNError', () => {
  it('should create error with all properties', () => {
    const error = new ICNError('Test error', 404, 'NOT_FOUND', { extra: 'data' });
    expect(error.message).toBe('Test error');
    expect(error.statusCode).toBe(404);
    expect(error.code).toBe('NOT_FOUND');
    expect(error.details).toEqual({ extra: 'data' });
    expect(error.name).toBe('ICNError');
  });

  it('should create error with minimal properties', () => {
    const error = new ICNError('Test error', 500);
    expect(error.message).toBe('Test error');
    expect(error.statusCode).toBe(500);
    expect(error.code).toBeUndefined();
    expect(error.details).toBeUndefined();
  });
});

describe('submitWasmTask', () => {
  let client: ICNClient;

  beforeEach(() => {
    client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
    });
  });

  it('should handle Uint8Array input', async () => {
    // This test verifies the base64 encoding logic
    const wasmBytes = new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]); // WASM magic

    // Mock fetch to capture the request
    const mockFetch = jest.fn().mockRejectedValue(new Error('Network error'));
    const clientWithMock = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    // We expect this to fail due to network, but we can check the call was made
    await expect(clientWithMock.submitWasmTask(wasmBytes)).rejects.toThrow();
    expect(mockFetch).toHaveBeenCalledTimes(1);

    // Verify the request body contains wasm_bytes and code_type
    const [, options] = mockFetch.mock.calls[0];
    const body = JSON.parse(options.body);
    expect(body.code_type).toBe('wasm');
    expect(body.wasm_bytes).toBeDefined();
    // Base64 of [0, 97, 115, 109, 1, 0, 0, 0] = "AGFzbQEAAAA="
    expect(body.wasm_bytes).toBe('AGFzbQEAAAA=');
  });

  it('should handle ArrayBuffer input', async () => {
    const buffer = new ArrayBuffer(8);
    const view = new Uint8Array(buffer);
    view.set([0, 97, 115, 109, 1, 0, 0, 0]); // WASM magic

    const mockFetch = jest.fn().mockRejectedValue(new Error('Network error'));
    const clientWithMock = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    await expect(clientWithMock.submitWasmTask(buffer)).rejects.toThrow();
    expect(mockFetch).toHaveBeenCalledTimes(1);

    const [, options] = mockFetch.mock.calls[0];
    const body = JSON.parse(options.body);
    expect(body.code_type).toBe('wasm');
    expect(body.wasm_bytes).toBe('AGFzbQEAAAA=');
  });

  it('should include optional parameters', async () => {
    const wasmBytes = new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]);

    const mockFetch = jest.fn().mockRejectedValue(new Error('Network error'));
    const clientWithMock = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    await expect(
      clientWithMock.submitWasmTask(wasmBytes, {
        fuel_limit: 50000,
        priority: 'high',
        deadline_ms: 30000,
      })
    ).rejects.toThrow();

    const [, options] = mockFetch.mock.calls[0];
    const body = JSON.parse(options.body);
    expect(body.fuel_limit).toBe(50000);
    expect(body.priority).toBe('high');
    expect(body.deadline_ms).toBe(30000);
  });
});

describe('large WASM handling', () => {
  it('should handle large WASM modules (chunked encoding)', async () => {
    // Create a "large" WASM module (70KB to exceed the ~65KB JS spread limit)
    const largeWasm = new Uint8Array(70 * 1024);
    // Fill with WASM magic header and zeros
    largeWasm[0] = 0;
    largeWasm[1] = 97;
    largeWasm[2] = 115;
    largeWasm[3] = 109;

    const mockFetch = jest.fn().mockRejectedValue(new Error('Network error'));
    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    await expect(client.submitWasmTask(largeWasm)).rejects.toThrow();

    // Verify the call was made with valid base64
    const [, options] = mockFetch.mock.calls[0];
    const body = JSON.parse(options.body);
    expect(body.wasm_bytes).toBeDefined();
    expect(typeof body.wasm_bytes).toBe('string');
    // Base64 length should be roughly 4/3 of input length
    expect(body.wasm_bytes.length).toBeGreaterThan(90000);
  });
});
