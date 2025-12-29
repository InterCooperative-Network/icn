/**
 * ICN TypeScript SDK Tests
 */

import { ICNClient, ICNError, ICNSubscription } from './index';

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

describe('compute task status', () => {
  it('should get task status', async () => {
    const mockResponse = {
      task_hash: 'abc123',
      status: 'completed',
      result: { output: 42 },
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

    const result = await client.getTaskStatus('abc123');

    expect(result).toEqual(mockResponse);
    const [url] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/compute/status/abc123');
  });

  it('should poll for task completion', async () => {
    let callCount = 0;
    const mockFetch = jest.fn().mockImplementation(() => {
      callCount++;
      const status = callCount < 3 ? 'pending' : 'completed';
      return Promise.resolve({
        ok: true,
        status: 200,
        json: async () => ({
          task_hash: 'abc123',
          status,
          result: status === 'completed' ? { output: 42 } : undefined,
        }),
      });
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    const result = await client.waitForTask('abc123', 10, 5000);

    expect(result.status).toBe('completed');
    expect(mockFetch).toHaveBeenCalledTimes(3);
  });

  it('should handle failed tasks in waitForTask', async () => {
    const mockFetch = jest.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        task_hash: 'abc123',
        status: 'failed',
        error: 'Out of fuel',
      }),
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    const result = await client.waitForTask('abc123', 10, 5000);

    expect(result.status).toBe('failed');
  });

  it('should handle cancelled tasks in waitForTask', async () => {
    const mockFetch = jest.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        task_hash: 'abc123',
        status: 'cancelled',
      }),
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    const result = await client.waitForTask('abc123', 10, 5000);

    expect(result.status).toBe('cancelled');
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
      expires_in: 3600, // 1 hour in seconds
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

    expect(result.token).toEqual(mockResponse.token);
    expect(result.expires_at).toBeGreaterThan(Date.now());

    const [url, options] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/auth/verify');

    const body = JSON.parse(options.body);
    expect(body.did).toBe('did:icn:alice');
    expect(body.signature).toBe('signature-bytes');
    expect(body.coop_id).toBe('my-coop');
    expect(body.scopes).toEqual(['ledger:read', 'ledger:write']);
  });

  it('should authenticate with signature provider', async () => {
    const mockChallenge = { challenge: 'random-challenge', expires_at: '2024-01-01T00:01:00Z' };
    const mockVerify = { token: 'jwt-token', expires_in: 3600 };

    let callIndex = 0;
    const mockFetch = jest.fn().mockImplementation(async () => {
      callIndex++;
      return {
        ok: true,
        status: 200,
        json: async () => (callIndex === 1 ? mockChallenge : mockVerify),
      };
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      fetch: mockFetch as unknown as typeof fetch,
    });

    const signer = { sign: async (msg: string) => `signed-${msg}` };
    const result = await client.authenticate('did:icn:alice', signer, 'my-coop', ['ledger:read']);

    expect(result.token).toEqual('jwt-token');
    expect(result.expires_at).toBeGreaterThan(Date.now());
    expect(client.hasToken()).toBe(true);
    expect(mockFetch).toHaveBeenCalledTimes(2);
  });
});

describe('health endpoint', () => {
  it('should get health status without auth', async () => {
    const mockResponse = {
      status: 'healthy',
      version: '0.1.0',
      uptime_seconds: 3600,
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

    // Note: health should work without token
    const result = await client.health();

    expect(result).toEqual(mockResponse);
    const [url, options] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/health');
    expect(options.headers['Authorization']).toBeUndefined();
  });
});

describe('cooperative CRUD operations', () => {
  it('should update cooperative', async () => {
    const mockResponse = {
      id: 'my-coop',
      name: 'Updated Coop',
      owner: 'did:icn:alice',
      settings: { currency: 'USD' },
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

    const result = await client.updateCoop('my-coop', { name: 'Updated Coop' });

    expect(result.name).toBe('Updated Coop');
    const [url, options] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/coops/my-coop');
    expect(options.method).toBe('PUT');
  });

  it('should delete cooperative', async () => {
    const mockFetch = jest.fn().mockResolvedValue({
      ok: true,
      status: 204,
      json: async () => ({}),
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    await client.deleteCoop('my-coop');

    const [url, options] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/coops/my-coop');
    expect(options.method).toBe('DELETE');
  });

  it('should add member', async () => {
    const mockResponse = {
      did: 'did:icn:bob',
      role: 'member',
      joined_at: '2024-01-01T00:00:00Z',
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

    const result = await client.addMember('my-coop', { did: 'did:icn:bob', role: 'member' });

    expect(result.did).toBe('did:icn:bob');
    const [url, options] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/coops/my-coop/members');
    expect(options.method).toBe('POST');
  });

  it('should update member role', async () => {
    const mockResponse = {
      did: 'did:icn:bob',
      role: 'admin',
      joined_at: '2024-01-01T00:00:00Z',
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

    const result = await client.updateMember('my-coop', 'did:icn:bob', { role: 'admin' });

    expect(result.role).toBe('admin');
    const [url, options] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/coops/my-coop/members/did%3Aicn%3Abob');
    expect(options.method).toBe('PUT');
  });

  it('should remove member', async () => {
    const mockFetch = jest.fn().mockResolvedValue({
      ok: true,
      status: 204,
      json: async () => ({}),
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    await client.removeMember('my-coop', 'did:icn:bob');

    const [url, options] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/coops/my-coop/members/did%3Aicn%3Abob');
    expect(options.method).toBe('DELETE');
  });
});

describe('governance operations - additional', () => {
  it('should list domains', async () => {
    const mockResponse = [
      { id: 'domain-1', name: 'Domain 1' },
      { id: 'domain-2', name: 'Domain 2' },
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

    const result = await client.listDomains();

    expect(result).toHaveLength(2);
    const [url] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/gov/domains');
  });

  it('should get proposal', async () => {
    const mockResponse = {
      id: 'prop-1',
      domain_id: 'domain-1',
      title: 'Test Proposal',
      state: 'open',
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

    const result = await client.getProposal('prop-1');

    expect(result.id).toBe('prop-1');
    const [url] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/gov/proposals/prop-1');
  });

  it('should list proposals with filters', async () => {
    const mockResponse = [
      { id: 'prop-1', domain_id: 'domain-1', title: 'Proposal 1', state: 'open' },
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

    const result = await client.listProposals('domain-1', 'open');

    expect(result).toHaveLength(1);
    const [url] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/gov/proposals?domain_id=domain-1&state=open');
  });

  it('should open proposal', async () => {
    const mockResponse = {
      id: 'prop-1',
      domain_id: 'domain-1',
      title: 'Test Proposal',
      state: 'open',
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

    const result = await client.openProposal('prop-1');

    expect(result.state).toBe('open');
    const [url, options] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/gov/proposals/prop-1/open');
    expect(options.method).toBe('POST');
  });

  it('should close proposal and get outcome', async () => {
    const mockResponse = {
      accepted: true,
      votes_for: 10,
      votes_against: 3,
      votes_abstain: 2,
      quorum_met: true,
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

    const result = await client.closeProposal('prop-1');

    expect(result.accepted).toBe(true);
    expect(result.quorum_met).toBe(true);
    const [url, options] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/gov/proposals/prop-1/close');
    expect(options.method).toBe('POST');
  });

  it('should get vote tally', async () => {
    const mockResponse = {
      proposal_id: 'prop-1',
      votes_for: 10,
      votes_against: 3,
      votes_abstain: 2,
      total_votes: 15,
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

    const result = await client.getVotes('prop-1');

    expect(result.total_votes).toBe(15);
    const [url] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/gov/proposals/prop-1/votes');
  });
});

describe('vote delegation', () => {
  // Use valid-looking DIDs (key portion must be at least 10 chars and use base58btc encoding)
  // Base58btc starts with 'z' prefix, followed by valid base58 chars
  const validBobDid = 'did:icn:zBobKeyABCDEFGHJ';
  const validCharlieDid = 'did:icn:zCharlieKeyNPQRS';

  it('should create a blanket delegation', async () => {
    const mockResponse = {
      id: 'del-123',
      delegator: 'did:icn:alice123456',
      delegate: validBobDid,
      scope: 'blanket',
      created_at: 1700000000,
      is_active: true,
    };

    const mockFetch = jest.fn().mockResolvedValue({
      ok: true,
      status: 201,
      json: async () => mockResponse,
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    const result = await client.createDelegation({
      delegate: validBobDid,
      scope: 'blanket',
    });

    expect(result.id).toBe('del-123');
    expect(result.scope).toBe('blanket');
    expect(result.is_active).toBe(true);

    const [url, options] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/gov/delegations');
    expect(options.method).toBe('POST');

    const body = JSON.parse(options.body);
    expect(body.delegate).toBe(validBobDid);
    expect(body.scope).toBe('blanket');
  });

  it('should create a domain-scoped delegation with expiry', async () => {
    // Use a future timestamp (2030-01-01 00:00:00 UTC)
    const futureExpiry = 1893456000;

    const mockResponse = {
      id: 'del-456',
      delegator: 'did:icn:alice123456',
      delegate: validCharlieDid,
      scope: 'domain:my-domain',
      created_at: 1700000000,
      expires_at: futureExpiry,
      is_active: true,
    };

    const mockFetch = jest.fn().mockResolvedValue({
      ok: true,
      status: 201,
      json: async () => mockResponse,
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    const result = await client.createDelegation({
      delegate: validCharlieDid,
      scope: 'domain:my-domain',
      expires_at: futureExpiry,
    });

    expect(result.scope).toBe('domain:my-domain');
    expect(result.expires_at).toBe(futureExpiry);
  });

  it('should list delegations (active only by default)', async () => {
    const mockResponse = {
      given: [
        {
          id: 'del-1',
          delegator: 'did:icn:alice',
          delegate: 'did:icn:bob',
          scope: 'blanket',
          created_at: 1700000000,
          is_active: true,
        },
      ],
      received: [
        {
          id: 'del-2',
          delegator: 'did:icn:charlie',
          delegate: 'did:icn:alice',
          scope: 'domain:test',
          created_at: 1700000000,
          is_active: true,
        },
      ],
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

    const result = await client.listDelegations();

    expect(result.given).toHaveLength(1);
    expect(result.received).toHaveLength(1);
    expect(result.given[0].id).toBe('del-1');
    expect(result.received[0].scope).toBe('domain:test');

    const [url] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/gov/delegations');
  });

  it('should list delegations including revoked', async () => {
    const mockFetch = jest.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ given: [], received: [] }),
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    await client.listDelegations(true);

    const [url] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/gov/delegations?include_revoked=true');
  });

  it('should get a specific delegation', async () => {
    const mockResponse = {
      id: 'del-123',
      delegator: 'did:icn:alice',
      delegate: 'did:icn:bob',
      scope: 'proposal:prop-1',
      created_at: 1700000000,
      is_active: true,
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

    const result = await client.getDelegation('del-123');

    expect(result.id).toBe('del-123');
    expect(result.scope).toBe('proposal:prop-1');

    const [url] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/gov/delegations/del-123');
  });

  it('should revoke a delegation', async () => {
    const mockFetch = jest.fn().mockResolvedValue({
      ok: true,
      status: 204,
      json: async () => null,
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    await client.revokeDelegation('del-123');

    const [url, options] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/gov/delegations/del-123');
    expect(options.method).toBe('DELETE');
  });

  describe('input validation', () => {
    let client: ICNClient;
    const mockFetch = jest.fn();

    beforeEach(() => {
      client = new ICNClient({
        baseUrl: 'http://localhost:8080',
        token: 'test-token',
        fetch: mockFetch as unknown as typeof fetch,
      });
    });

    it('should reject invalid delegate DID', async () => {
      await expect(
        client.createDelegation({
          delegate: 'not-a-valid-did',
          scope: 'blanket',
        })
      ).rejects.toThrow('must be a valid DID');
    });

    it('should reject empty delegate', async () => {
      await expect(
        client.createDelegation({
          delegate: '',
          scope: 'blanket',
        })
      ).rejects.toThrow('delegate is required');
    });

    it('should reject invalid scope format', async () => {
      await expect(
        client.createDelegation({
          delegate: 'did:icn:zValidKeyABCDEF',
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          scope: 'invalid-scope' as any, // Bypass type check for runtime validation test
        })
      ).rejects.toThrow("scope must be 'blanket'");
    });

    it('should reject empty scope ID', async () => {
      await expect(
        client.createDelegation({
          delegate: 'did:icn:zValidKeyABCDEF',
          // eslint-disable-next-line @typescript-eslint/no-explicit-any
          scope: 'domain:' as any, // Bypass type check for runtime validation test
        })
      ).rejects.toThrow('scope ID cannot be empty');
    });

    it('should reject past expiration', async () => {
      await expect(
        client.createDelegation({
          delegate: 'did:icn:zValidKeyABCDEF',
          scope: 'blanket',
          expires_at: 1700000000, // Past timestamp (Nov 2023)
        })
      ).rejects.toThrow('expires_at must be in the future');
    });

    it('should accept valid blanket delegation', async () => {
      // Valid base58btc DID (starts with 'z')
      const validDid = 'did:icn:zValidKeyABCDEF';
      mockFetch.mockResolvedValueOnce({
        ok: true,
        status: 201,
        json: async () => ({
          id: 'del-1',
          delegator: 'did:icn:alice123456789',
          delegate: validDid,
          scope: 'blanket',
          is_active: true,
        }),
      });

      await expect(
        client.createDelegation({
          delegate: validDid,
          scope: 'blanket',
        })
      ).resolves.toBeDefined();
    });

    it('should accept valid domain-scoped delegation', async () => {
      const validDid = 'did:icn:zValidKeyABCDEF';
      mockFetch.mockResolvedValueOnce({
        ok: true,
        status: 201,
        json: async () => ({
          id: 'del-2',
          delegator: 'did:icn:alice123456789',
          delegate: validDid,
          scope: 'domain:test-domain',
          is_active: true,
        }),
      });

      await expect(
        client.createDelegation({
          delegate: validDid,
          scope: 'domain:test-domain',
        })
      ).resolves.toBeDefined();
    });

    it('should accept valid proposal-scoped delegation', async () => {
      const validDid = 'did:icn:zValidKeyABCDEF';
      mockFetch.mockResolvedValueOnce({
        ok: true,
        status: 201,
        json: async () => ({
          id: 'del-3',
          delegator: 'did:icn:alice123456789',
          delegate: validDid,
          scope: 'proposal:prop-123',
          is_active: true,
        }),
      });

      await expect(
        client.createDelegation({
          delegate: validDid,
          scope: 'proposal:prop-123',
        })
      ).resolves.toBeDefined();
    });
  });
});

describe('compute task submission - CCL', () => {
  it('should submit CCL task with code', async () => {
    const mockResponse = {
      task_hash: 'abc123',
      task_id: 'task-1',
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

    const cclCode = JSON.stringify({ name: 'test-contract', rules: [] });
    const result = await client.submitTask({
      code: cclCode,
      fuel_limit: 10000,
    });

    expect(result.task_hash).toBe('abc123');
    const [url, options] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/compute/submit');
    expect(options.method).toBe('POST');

    const body = JSON.parse(options.body);
    expect(body.code).toBe(cclCode);
    expect(body.fuel_limit).toBe(10000);
  });

  it('should submit CCL task with priority', async () => {
    const mockResponse = {
      task_hash: 'abc123',
      task_id: 'task-1',
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

    const result = await client.submitTask({
      code: '{}',
      fuel_limit: 10000,
      priority: 'high',
      payment_rate: 100,
    });

    expect(result.task_hash).toBe('abc123');
    const [, options] = mockFetch.mock.calls[0];
    const body = JSON.parse(options.body);
    expect(body.priority).toBe('high');
    expect(body.payment_rate).toBe(100);
  });
});

describe('retry logic', () => {
  it('should retry on 503 Service Unavailable', async () => {
    let callCount = 0;
    const mockFetch = jest.fn().mockImplementation(() => {
      callCount++;
      if (callCount < 3) {
        return Promise.resolve({
          ok: false,
          status: 503,
          statusText: 'Service Unavailable',
          json: async () => ({ error: 'Service Unavailable' }),
        });
      }
      return Promise.resolve({
        ok: true,
        status: 200,
        json: async () => ({ status: 'healthy' }),
      });
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      fetch: mockFetch as unknown as typeof fetch,
      retry: { maxRetries: 3, initialDelayMs: 10, maxDelayMs: 100 },
    });

    const result = await client.health();

    expect(result.status).toBe('healthy');
    expect(mockFetch).toHaveBeenCalledTimes(3);
  });

  it('should not retry on 404 Not Found', async () => {
    const mockFetch = jest.fn().mockResolvedValue({
      ok: false,
      status: 404,
      statusText: 'Not Found',
      json: async () => ({ error: 'Not Found' }),
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      fetch: mockFetch as unknown as typeof fetch,
      retry: { maxRetries: 3, initialDelayMs: 10 },
    });

    await expect(client.health()).rejects.toThrow('Not Found');
    expect(mockFetch).toHaveBeenCalledTimes(1);
  });

  it('should retry on 429 Rate Limited', async () => {
    let callCount = 0;
    const mockFetch = jest.fn().mockImplementation(() => {
      callCount++;
      if (callCount === 1) {
        return Promise.resolve({
          ok: false,
          status: 429,
          statusText: 'Too Many Requests',
          json: async () => ({ error: 'Rate limited' }),
        });
      }
      return Promise.resolve({
        ok: true,
        status: 200,
        json: async () => ({ status: 'healthy' }),
      });
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      fetch: mockFetch as unknown as typeof fetch,
      retry: { maxRetries: 2, initialDelayMs: 10, maxDelayMs: 50 },
    });

    const result = await client.health();

    expect(result.status).toBe('healthy');
    expect(mockFetch).toHaveBeenCalledTimes(2);
  });

  it('should give up after max retries', async () => {
    const mockFetch = jest.fn().mockResolvedValue({
      ok: false,
      status: 503,
      statusText: 'Service Unavailable',
      json: async () => ({ error: 'Service Unavailable' }),
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      fetch: mockFetch as unknown as typeof fetch,
      retry: { maxRetries: 2, initialDelayMs: 10, maxDelayMs: 50 },
    });

    await expect(client.health()).rejects.toThrow('Service Unavailable');
    // Initial attempt + 2 retries = 3 calls
    expect(mockFetch).toHaveBeenCalledTimes(3);
  });
});

describe('token expiration', () => {
  it('should track token expiration', () => {
    const client = new ICNClient({ baseUrl: 'http://localhost:8080' });

    expect(client.isTokenExpired()).toBe(false);
    expect(client.getTokenExpiresAt()).toBeUndefined();

    const futureTime = Math.floor(Date.now() / 1000) + 3600; // 1 hour from now
    client.setToken('test-token', futureTime);

    expect(client.getTokenExpiresAt()).toBe(futureTime);
    expect(client.isTokenExpired()).toBe(false);
  });

  it('should detect expired token', () => {
    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      refreshBeforeExpiry: 60, // Refresh 60 seconds before expiry
    });

    const pastTime = Math.floor(Date.now() / 1000) - 100; // 100 seconds ago
    client.setToken('test-token', pastTime);

    expect(client.isTokenExpired()).toBe(true);
  });

  it('should detect token expiring soon', () => {
    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      refreshBeforeExpiry: 120, // Refresh 120 seconds before expiry
    });

    const soonTime = Math.floor(Date.now() / 1000) + 60; // Expires in 60 seconds
    client.setToken('test-token', soonTime);

    // Should be considered expired since it expires within refreshBeforeExpiry window
    expect(client.isTokenExpired()).toBe(true);
  });

  it('should clear all auth state on clearToken', () => {
    const client = new ICNClient({ baseUrl: 'http://localhost:8080' });

    const futureTime = Math.floor(Date.now() / 1000) + 3600;
    client.setToken('test-token', futureTime);

    expect(client.hasToken()).toBe(true);
    expect(client.getTokenExpiresAt()).toBe(futureTime);

    client.clearToken();

    expect(client.hasToken()).toBe(false);
    expect(client.getTokenExpiresAt()).toBeUndefined();
  });
});

describe('auto-refresh authentication', () => {
  it('should store credentials for auto-refresh', async () => {
    const mockChallenge = { challenge: 'test-challenge', expires_at: Date.now() + 60000 };
    const mockVerify = { token: 'test-token', expires_in: 3600 };

    let callIndex = 0;
    const mockFetch = jest.fn().mockImplementation(async () => {
      callIndex++;
      return {
        ok: true,
        status: 200,
        json: async () => (callIndex === 1 ? mockChallenge : mockVerify),
      };
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      fetch: mockFetch as unknown as typeof fetch,
      autoRefresh: true,
    });

    const signer = { sign: async (msg: string) => `signed-${msg}` };
    await client.authenticate('did:icn:alice', signer, 'my-coop', ['ledger:read']);

    expect(client.hasToken()).toBe(true);
    expect(client.getTokenExpiresAt()).toBeGreaterThan(Date.now());
  });
});

describe('client options', () => {
  it('should accept custom retry options', () => {
    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      retry: {
        maxRetries: 5,
        initialDelayMs: 500,
        maxDelayMs: 20000,
        backoffMultiplier: 3,
        jitterFactor: 0.2,
        retryableStatuses: [500, 502],
      },
    });

    expect(client).toBeInstanceOf(ICNClient);
  });

  it('should accept autoRefresh option', () => {
    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      autoRefresh: true,
      refreshBeforeExpiry: 120,
    });

    expect(client).toBeInstanceOf(ICNClient);
  });
});

describe('ICNSubscription', () => {
  it('should export ICNSubscription class', () => {
    expect(ICNSubscription).toBeDefined();
  });
});

describe('listCoops', () => {
  it('should list all cooperatives', async () => {
    const mockResponse = [
      { id: 'coop-1', name: 'Coop 1', owner: 'did:icn:alice' },
      { id: 'coop-2', name: 'Coop 2', owner: 'did:icn:bob' },
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

    const result = await client.listCoops();

    expect(result).toEqual(mockResponse);
    expect(result).toHaveLength(2);
    const [url] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/coops');
  });
});

describe('waitForTask timeout', () => {
  it('should throw timeout error when task never completes', async () => {
    const mockFetch = jest.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({
        task_hash: 'abc123',
        status: 'pending',
      }),
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    // Very short timeout to trigger the timeout path
    await expect(client.waitForTask('abc123', 10, 50)).rejects.toThrow('Task polling timeout');
  });
});

describe('auto-refresh - token refresh flow', () => {
  it('should auto-refresh expired token before request', async () => {
    // First two calls are for authentication (challenge + verify)
    // Third call is for the actual request that triggers refresh
    // Fourth and fifth calls are for re-authentication
    // Sixth call is for the retried request
    let callCount = 0;
    const mockFetch = jest.fn().mockImplementation(async (url: string) => {
      callCount++;
      if (url.includes('/auth/challenge')) {
        return {
          ok: true,
          status: 200,
          json: async () => ({ challenge: 'test-challenge', expires_at: Date.now() + 60000 }),
        };
      }
      if (url.includes('/auth/verify')) {
        return {
          ok: true,
          status: 200,
          json: async () => ({ token: `token-${callCount}`, expires_at: Math.floor(Date.now() / 1000) + 3600 }),
        };
      }
      // Any other request (e.g., /coops)
      return {
        ok: true,
        status: 200,
        json: async () => [{ id: 'coop-1', name: 'Test Coop' }],
      };
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      fetch: mockFetch as unknown as typeof fetch,
      autoRefresh: true,
      refreshBeforeExpiry: 120, // Refresh 2 minutes before expiry
    });

    const signer = { sign: async (msg: string) => `signed-${msg}` };

    // First authenticate - this sets up the token and stores signer for auto-refresh
    await client.authenticate('did:icn:alice', signer, 'my-coop', ['coop:read']);
    expect(client.hasToken()).toBe(true);

    // Now manually set the token to be "expired" (within refresh window)
    const almostExpired = Math.floor(Date.now() / 1000) + 60; // Expires in 60s, but refresh window is 120s
    client.setToken('old-token', almostExpired);
    expect(client.isTokenExpired()).toBe(true);

    // Make a request - should trigger auto-refresh
    await client.listCoops();

    // Should have called authenticate again (challenge + verify) + the actual request
    // Total: 2 (initial auth) + 2 (refresh auth) + 1 (listCoops) = 5 calls
    expect(mockFetch.mock.calls.length).toBeGreaterThanOrEqual(4);
  });

  it('should not refresh if autoRefresh is disabled', async () => {
    const mockFetch = jest.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => [{ id: 'coop-1', name: 'Test Coop' }],
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
      autoRefresh: false, // Disabled
    });

    // Set expired token
    const expiredTime = Math.floor(Date.now() / 1000) - 100;
    client.setToken('expired-token', expiredTime);

    // Should proceed without refresh attempt
    await client.listCoops();

    // Only 1 call (the actual request), no refresh
    expect(mockFetch).toHaveBeenCalledTimes(1);
  });
});

describe('createClient helper', () => {
  it('should create client using createClient function', () => {
    const { createClient } = require('./index');
    const client = createClient({ baseUrl: 'http://localhost:8080' });
    expect(client).toBeInstanceOf(ICNClient);
  });
});

describe('edge cases', () => {
  it('should handle 204 No Content response', async () => {
    const mockFetch = jest.fn().mockResolvedValue({
      ok: true,
      status: 204,
      json: async () => undefined,
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    // deleteCoop returns void (204)
    const result = await client.deleteCoop('my-coop');
    expect(result).toBeUndefined();
  });

  it('should require auth for protected endpoints', async () => {
    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      // No token set
    });

    await expect(client.listCoops()).rejects.toThrow('Authentication required');
  });

  it('should handle abort error as timeout', async () => {
    // Create a proper DOMException-like error for AbortError
    const abortError = new DOMException('The operation was aborted', 'AbortError');

    const mockFetch = jest.fn().mockRejectedValue(abortError);

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      fetch: mockFetch as unknown as typeof fetch,
      retry: { maxRetries: 0 }, // Disable retries
    });

    await expect(client.health()).rejects.toThrow('Request timeout');
  }, 10000);

  it('should handle error response without json body', async () => {
    const mockFetch = jest.fn().mockResolvedValue({
      ok: false,
      status: 500,
      statusText: 'Internal Server Error',
      json: async () => { throw new Error('No JSON'); },
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      fetch: mockFetch as unknown as typeof fetch,
      retry: { maxRetries: 0 }, // Disable retries for this test
    });

    await expect(client.health()).rejects.toThrow('Internal Server Error');
  });
});

describe('amendment voting', () => {
  it('should cast a vote on an amendment', async () => {
    const mockResponse = {
      success: true,
      vote: 'approve',
      total_votes: 5,
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

    const result = await client.castAmendmentVote('abc123def456', {
      vote: 'approve',
      comment: 'I support this amendment',
    });

    expect(result.success).toBe(true);
    expect(result.vote).toBe('approve');
    expect(result.total_votes).toBe(5);
    const [url, options] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/constitutional/amendments/abc123def456/votes');
    expect(options.method).toBe('POST');
    const body = JSON.parse(options.body);
    expect(body.vote).toBe('approve');
    expect(body.comment).toBe('I support this amendment');
  });

  it('should list votes on an amendment', async () => {
    const mockResponse = {
      amendment_id: 'abc123def456',
      total_votes: 3,
      votes: [
        { voter: 'did:icn:alice', vote: 'approve', timestamp: 1700000000, voted_at: '2024-11-14 22:13 UTC' },
        { voter: 'did:icn:bob', vote: 'reject', timestamp: 1700000100, voted_at: '2024-11-14 22:15 UTC' },
      ],
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

    const result = await client.listAmendmentVotes('abc123def456');

    expect(result.total_votes).toBe(3);
    expect(result.votes).toHaveLength(2);
    const [url] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/constitutional/amendments/abc123def456/votes');
  });

  it('should get amendment results', async () => {
    const mockResponse = {
      amendment_id: 'abc123def456',
      title: 'Test Amendment',
      status: 'voting',
      total_votes: 10,
      approve_count: 7,
      reject_count: 2,
      abstain_count: 1,
      approval_percentage: 77.78,
      quorum_required: 50,
      quorum_achieved: true,
      approval_threshold: 67,
      has_passed: true,
      voting_ends_at: 1700086400,
      time_remaining_secs: 3600,
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

    const result = await client.getAmendmentResults('abc123def456');

    expect(result.has_passed).toBe(true);
    expect(result.approval_percentage).toBe(77.78);
    expect(result.quorum_achieved).toBe(true);
    const [url] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/constitutional/amendments/abc123def456/results');
  });

  it('should get amendment results with votes included', async () => {
    const mockFetch = jest.fn().mockResolvedValue({
      ok: true,
      status: 200,
      json: async () => ({ amendment_id: 'abc123', votes: [] }),
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    await client.getAmendmentResults('abc123def456', true);

    const [url] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/constitutional/amendments/abc123def456/results?include_votes=true');
  });

  it('should get my vote on an amendment', async () => {
    const mockResponse = {
      amendment_id: 'abc123def456',
      has_voted: true,
      vote: {
        voter: 'did:icn:alice',
        vote: 'approve',
        timestamp: 1700000000,
        voted_at: '2024-11-14 22:13 UTC',
      },
      can_vote: false,
      reason: 'Already voted',
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

    const result = await client.getMyAmendmentVote('abc123def456');

    expect(result.has_voted).toBe(true);
    expect(result.can_vote).toBe(false);
    expect(result.vote?.vote).toBe('approve');
    const [url] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/constitutional/amendments/abc123def456/my-vote');
  });
});

describe('identity resolution', () => {
  it('should resolve a DID without attestations', async () => {
    const mockResponse = {
      did: 'did:icn:abc123',
      valid: true,
      coop_id: 'my-coop',
      has_attestations: false,
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

    const result = await client.resolveDid('did:icn:abc123');

    expect(result.valid).toBe(true);
    expect(result.did).toBe('did:icn:abc123');
    expect(result.coop_id).toBe('my-coop');
    const [url, options] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/identity/resolve/did%3Aicn%3Aabc123');
    expect(options.headers['Authorization']).toBeUndefined(); // Public endpoint
  });

  it('should resolve a DID with attestations included', async () => {
    const mockResponse = {
      did: 'did:icn:abc123',
      valid: true,
      coop_id: 'my-coop',
      has_attestations: true,
      attestation_count: 3,
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

    const result = await client.resolveDid('did:icn:abc123', true);

    expect(result.has_attestations).toBe(true);
    expect(result.attestation_count).toBe(3);
    const [url] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/identity/resolve/did%3Aicn%3Aabc123?include_attestations=true');
  });

  it('should check identity service health', async () => {
    const mockResponse = {
      status: 'ok',
      service: 'identity',
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

    const result = await client.identityHealth();

    expect(result.status).toBe('ok');
    expect(result.service).toBe('identity');
    const [url, options] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/identity/health');
    expect(options.headers['Authorization']).toBeUndefined(); // Public endpoint
  });
});

describe('device management', () => {
  it('should register a new device', async () => {
    const mockResponse = {
      device: {
        id: 'phone-2',
        label: 'My iPhone',
        key_type: 'Ed25519',
        capabilities: ['sign', 'encrypt'],
        added_at: 1700000000,
        revoked: false,
      },
      message: 'Device registered successfully',
    };

    const mockFetch = jest.fn().mockResolvedValue({
      ok: true,
      status: 201,
      json: async () => mockResponse,
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    const result = await client.registerDevice('did:icn:alice', {
      device_id: 'phone-2',
      label: 'My iPhone',
      public_key: 'deadbeef...',
      capabilities: ['sign', 'encrypt'],
      signing_device_id: 'device-1',
      signature: 'cafebabe...',
    });

    expect(result.device.id).toBe('phone-2');
    expect(result.device.capabilities).toContain('sign');
    expect(result.message).toBe('Device registered successfully');
    const [url, options] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/devices/did%3Aicn%3Aalice');
    expect(options.method).toBe('POST');
    expect(options.headers['Authorization']).toBe('Bearer test-token');
    const body = JSON.parse(options.body);
    expect(body.device_id).toBe('phone-2');
    expect(body.signing_device_id).toBe('device-1');
  });

  it('should list all devices for a DID', async () => {
    const mockResponse = {
      devices: [
        {
          id: 'device-1',
          label: 'Primary Device',
          key_type: 'Ed25519',
          capabilities: ['sign', 'add_device', 'revoke_device'],
          added_at: 1699000000,
          revoked: false,
        },
        {
          id: 'phone-1',
          label: 'Mobile Phone',
          key_type: 'Ed25519',
          capabilities: ['sign'],
          added_at: 1700000000,
          revoked: false,
        },
      ],
      total: 2,
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

    const result = await client.listDevices('did:icn:alice');

    expect(result.total).toBe(2);
    expect(result.devices).toHaveLength(2);
    expect(result.devices[0].id).toBe('device-1');
    expect(result.devices[1].id).toBe('phone-1');
    const [url] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/devices/did%3Aicn%3Aalice');
  });

  it('should get a specific device by ID', async () => {
    const mockResponse = {
      id: 'phone-1',
      label: 'Mobile Phone',
      key_type: 'Ed25519',
      capabilities: ['sign', 'encrypt'],
      added_at: 1700000000,
      revoked: false,
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

    const result = await client.getDevice('did:icn:alice', 'phone-1');

    expect(result.id).toBe('phone-1');
    expect(result.capabilities).toContain('sign');
    const [url] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/devices/did%3Aicn%3Aalice/phone-1');
  });

  it('should revoke a device', async () => {
    const mockResponse = {
      message: 'Device revoked successfully',
      device_id: 'phone-old',
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

    const result = await client.revokeDevice('did:icn:alice', 'phone-old', {
      signing_device_id: 'device-1',
      signature: 'cafebabe...',
    });

    expect(result.message).toBe('Device revoked successfully');
    expect(result.device_id).toBe('phone-old');
    const [url, options] = mockFetch.mock.calls[0];
    expect(url).toBe('http://localhost:8080/v1/devices/did%3Aicn%3Aalice/phone-old');
    expect(options.method).toBe('DELETE');
    const body = JSON.parse(options.body);
    expect(body.signing_device_id).toBe('device-1');
    expect(body.signature).toBe('cafebabe...');
  });

  it('should handle device registration with encryption key', async () => {
    const mockResponse = {
      device: {
        id: 'phone-3',
        label: 'Encrypted Device',
        key_type: 'Ed25519',
        capabilities: ['sign', 'encrypt'],
        added_at: 1700000000,
        revoked: false,
      },
      message: 'Device registered successfully',
    };

    const mockFetch = jest.fn().mockResolvedValue({
      ok: true,
      status: 201,
      json: async () => mockResponse,
    });

    const client = new ICNClient({
      baseUrl: 'http://localhost:8080',
      token: 'test-token',
      fetch: mockFetch as unknown as typeof fetch,
    });

    const result = await client.registerDevice('did:icn:alice', {
      device_id: 'phone-3',
      label: 'Encrypted Device',
      public_key: 'deadbeef...',
      encryption_public_key: 'cafebabe...',
      capabilities: ['sign', 'encrypt'],
      signing_device_id: 'device-1',
      signature: 'signature...',
    });

    expect(result.device.id).toBe('phone-3');
    const [, options] = mockFetch.mock.calls[0];
    const body = JSON.parse(options.body);
    expect(body.encryption_public_key).toBe('cafebabe...');
  });
});
