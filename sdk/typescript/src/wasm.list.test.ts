/**
 * WasmClient.list and WasmClient.get tests
 */

import { WasmClient, WasmModuleInfo } from './wasm';
import { NotFoundError } from './types';

const MODULES: WasmModuleInfo[] = [
  {
    hash: 'a'.repeat(64),
    name: 'mod-alice',
    size_bytes: 100,
    owner: 'did:icn:alice',
    deployed_at: 1_000_000,
  },
  {
    hash: 'b'.repeat(64),
    name: 'mod-bob',
    size_bytes: 200,
    owner: 'did:icn:bob',
    deployed_at: 2_000_000,
  },
  {
    hash: 'c'.repeat(64),
    name: 'mod-alice-2',
    size_bytes: 300,
    owner: 'did:icn:alice',
    deployed_at: 3_000_000,
  },
];

function mockFetch(status: number, body: unknown) {
  return jest.fn().mockResolvedValue({
    ok: status < 400,
    status,
    statusText: status < 400 ? 'OK' : 'Not Found',
    json: () => Promise.resolve(body),
  });
}

function makeClient(fetchImpl: jest.Mock) {
  return new WasmClient({
    baseUrl: 'http://localhost:8080',
    token: 'tok',
    fetch: fetchImpl as unknown as typeof fetch,
  });
}

describe('WasmClient.list', () => {
  it('returns all modules when owner is not specified', async () => {
    const client = makeClient(mockFetch(200, MODULES));
    const result = await client.list();
    expect(result).toHaveLength(3);
  });

  it('filters by owner DID', async () => {
    const client = makeClient(mockFetch(200, MODULES));
    const result = await client.list('did:icn:alice');
    expect(result).toHaveLength(2);
    expect(result.every((m) => m.owner === 'did:icn:alice')).toBe(true);
  });

  it('returns empty array when no modules match owner', async () => {
    const client = makeClient(mockFetch(200, MODULES));
    const result = await client.list('did:icn:nobody');
    expect(result).toHaveLength(0);
  });

  it('returns empty array when registry is empty', async () => {
    const client = makeClient(mockFetch(200, []));
    const result = await client.list();
    expect(result).toHaveLength(0);
  });

  it('calls GET /v1/compute/wasm', async () => {
    const fetchImpl = mockFetch(200, MODULES);
    const client = makeClient(fetchImpl);
    await client.list();
    expect(fetchImpl).toHaveBeenCalledWith(
      'http://localhost:8080/v1/compute/wasm',
      expect.objectContaining({ method: 'GET' }),
    );
  });
});

describe('WasmClient.get', () => {
  it('returns module metadata for a known hash', async () => {
    const mod = MODULES[0];
    const client = makeClient(mockFetch(200, mod));

    const result = await client.get(mod.hash);
    expect(result.hash).toBe(mod.hash);
    expect(result.owner).toBe('did:icn:alice');
    expect(result.size_bytes).toBe(100);
  });

  it('calls GET /v1/compute/wasm/{hash}', async () => {
    const fetchImpl = mockFetch(200, MODULES[0]);
    const client = makeClient(fetchImpl);
    const hash = MODULES[0].hash;
    await client.get(hash);
    expect(fetchImpl).toHaveBeenCalledWith(
      `http://localhost:8080/v1/compute/wasm/${hash}`,
      expect.objectContaining({ method: 'GET' }),
    );
  });

  it('throws NotFoundError for unknown hash', async () => {
    const client = makeClient(mockFetch(404, { error: 'Not found', code: 'NOT_FOUND' }));
    await expect(client.get('0'.repeat(64))).rejects.toBeInstanceOf(NotFoundError);
  });

  it('preserves optional description field', async () => {
    const mod: WasmModuleInfo = { ...MODULES[0], description: 'my desc' };
    const client = makeClient(mockFetch(200, mod));
    const result = await client.get(mod.hash);
    expect(result.description).toBe('my desc');
  });
});
