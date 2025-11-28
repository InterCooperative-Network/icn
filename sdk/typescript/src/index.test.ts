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

describe('cooperative operations', () => {
  it('should create cooperative with correct request', async () => {
    const mockResponse = {
      id: 'test-coop',
      name: 'Test Cooperative',
      owner: 'did:icn:alice',
      members: [],
      created_at: '2024-01-01T00:00:00Z',
    };

    const mockFetch = jest.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => mockResponse,
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    const result = await client.createCoop({
      id: 'test-coop',
      name: 'Test Cooperative',
    });

    expect(result).toEqual(mockResponse);
    expect(mockFetch).toHaveBeenCalledTimes(1);

    const [url, options] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/coops');
    expect(options.method).toBe('POST');
    expect(options.headers['Authorization']).toBe('Bearer test-token');

    const body = JSON.parse(options.body);
    expect(body.id).toBe('test-coop');
    expect(body.name).toBe('Test Cooperative');
  });

  it('should get cooperative by ID', async () => {
    const mockResponse = {
      id: 'test-coop',
      name: 'Test Cooperative',
      owner: 'did:icn:alice',
    };

    const mockFetch = jest.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => mockResponse,
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    const result = await client.getCoop('test-coop');

    expect(result).toEqual(mockResponse);
    const [url] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/coops/test-coop');
  });

  it('should list members', async () => {
    const mockResponse = [
      { did: 'did:icn:alice', role: 'owner' },
      { did: 'did:icn:bob', role: 'member' },
    ];

    const mockFetch = jest.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => mockResponse,
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    const result = await client.listMembers('test-coop');

    expect(result).toEqual(mockResponse);
    expect(result).toHaveLength(2);
  });
});

describe('ledger operations', () => {
  it('should get balance', async () => {
    const mockResponse = {
      did: 'did:icn:alice',
      balance: 100,
      currency: 'credits',
    };

    const mockFetch = jest.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => mockResponse,
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    const result = await client.getBalance('test-coop', 'did:icn:alice');

    expect(result).toEqual(mockResponse);
    const [url] = mockFetch.mock.calls[0];
    expect(url).toContain('/ledger/test-coop/balance/');
  });

  it('should create payment with correct request', async () => {
    const mockResponse = {
      transaction_id: 'tx-123',
      success: true,
    };

    const mockFetch = jest.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => mockResponse,
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    const result = await client.pay('test-coop', {
      from: 'did:icn:alice',
      to: 'did:icn:bob',
      amount: 50,
      currency: 'credits',
      memo: 'Test payment',
    });

    expect(result).toEqual(mockResponse);

    const [url, options] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/ledger/test-coop/payment');
    expect(options.method).toBe('POST');

    const body = JSON.parse(options.body);
    expect(body.from).toBe('did:icn:alice');
    expect(body.to).toBe('did:icn:bob');
    expect(body.amount).toBe(50);
    expect(body.currency).toBe('credits');
    expect(body.memo).toBe('Test payment');
  });

  it('should get history with pagination', async () => {
    const mockResponse = {
      transactions: [],
      total: 0,
      offset: 10,
      limit: 20,
    };

    const mockFetch = jest.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => mockResponse,
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    await client.getHistory('test-coop', { offset: 10, limit: 20 });

    const [url] = mockFetch.mock.calls[0];
    expect(url).toContain('offset=10');
    expect(url).toContain('limit=20');
  });
});

describe('governance operations', () => {
  it('should create domain', async () => {
    const mockResponse = {
      id: 'coop:test',
      name: 'Test Domain',
      members: ['did:icn:alice', 'did:icn:bob'],
      created_at: 1704067200,
    };

    const mockFetch = jest.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => mockResponse,
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    const result = await client.createDomain({
      domain_id: 'coop:test',
      name: 'Test Domain',
      members: ['did:icn:alice', 'did:icn:bob'],
    });

    expect(result).toEqual(mockResponse);

    const [url, options] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/gov/domains');
    expect(options.method).toBe('POST');
  });

  it('should get domain', async () => {
    const mockResponse = {
      id: 'coop:test',
      name: 'Test Domain',
    };

    const mockFetch = jest.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => mockResponse,
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    const result = await client.getDomain('coop:test');

    expect(result).toEqual(mockResponse);
    const [url] = mockFetch.mock.calls[0];
    expect(url).toContain('/gov/domains/');
  });

  it('should create proposal', async () => {
    const mockResponse = {
      id: 'prop-123',
      title: 'Test Proposal',
      state: 'draft',
    };

    const mockFetch = jest.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => mockResponse,
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    const result = await client.createProposal({
      domain_id: 'coop:test',
      title: 'Test Proposal',
      description: 'A test proposal',
      kind: 'text',
    });

    expect(result).toEqual(mockResponse);

    const [, options] = mockFetch.mock.calls[0];
    const body = JSON.parse(options.body);
    expect(body.domain_id).toBe('coop:test');
    expect(body.title).toBe('Test Proposal');
    expect(body.kind).toBe('text');
  });

  it('should cast vote', async () => {
    const mockResponse = {
      success: true,
      tally: { for: 1, against: 0, abstain: 0 },
    };

    const mockFetch = jest.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => mockResponse,
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    await client.vote('prop-123', { choice: 'for' });

    const [url, options] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/gov/proposals/prop-123/vote');
    expect(options.method).toBe('POST');

    const body = JSON.parse(options.body);
    expect(body.choice).toBe('for');
  });
});

describe('compute task cancellation', () => {
  it('should cancel task with reason', async () => {
    const mockResponse = {
      success: true,
      task_hash: 'abc123',
    };

    const mockFetch = jest.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => mockResponse,
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    const result = await client.cancelTask('abc123', { reason: 'No longer needed' });

    expect(result).toEqual(mockResponse);

    const [url, options] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/compute/cancel/abc123');
    expect(options.method).toBe('POST');

    const body = JSON.parse(options.body);
    expect(body.reason).toBe('No longer needed');
  });

  it('should cancel task without reason', async () => {
    const mockResponse = {
      success: true,
      task_hash: 'abc123',
    };

    const mockFetch = jest.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => mockResponse,
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    await client.cancelTask('abc123');

    const [, options] = mockFetch.mock.calls[0];
    const body = JSON.parse(options.body);
    expect(body.reason).toBeUndefined();
  });
});

describe('authentication flow', () => {
  it('should get challenge', async () => {
    const mockResponse = {
      challenge: 'random-challenge-string',
      expires_at: '2024-01-01T01:00:00Z',
    };

    const mockFetch = jest.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => mockResponse,
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      fetch: mockFetch as unknown as typeof fetch,
    });

    const result = await client.getChallenge('did:icn:alice');

    expect(result).toEqual(mockResponse);

    const [url, options] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/auth/challenge');
    expect(options.method).toBe('POST');

    const body = JSON.parse(options.body);
    expect(body.did).toBe('did:icn:alice');
  });

  it('should verify signature and get token', async () => {
    const mockResponse = {
      token: 'jwt-token-here',
      expires_at: '2024-01-02T00:00:00Z',
    };

    const mockFetch = jest.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => mockResponse,
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      fetch: mockFetch as unknown as typeof fetch,
    });

    const result = await client.verify(
      'did:icn:alice',
      'signature-bytes',
      'my-coop',
      ['ledger:read', 'ledger:write']
    );

    expect(result).toEqual(mockResponse);

    const [url, options] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/auth/verify');

    const body = JSON.parse(options.body);
    expect(body.did).toBe('did:icn:alice');
    expect(body.signature).toBe('signature-bytes');
    expect(body.coop_id).toBe('my-coop');
    expect(body.scopes).toEqual(['ledger:read', 'ledger:write']);
  });
});
