/**
 * WasmClient.deploy tests
 */

import { WasmClient, WasmDeployResult } from './wasm';
import { ConflictError, ValidationError, AuthenticationError } from './types';

const FAKE_HASH = 'a'.repeat(64);

function mockFetch(status: number, body: unknown) {
  return jest.fn().mockResolvedValue({
    ok: status < 400,
    status,
    statusText: status < 400 ? 'OK' : 'Error',
    json: () => Promise.resolve(body),
  });
}

function makeClient(fetchImpl: jest.Mock, token = 'test-token') {
  return new WasmClient({
    baseUrl: 'http://localhost:8080',
    token,
    fetch: fetchImpl as unknown as typeof fetch,
  });
}

const WASM_MAGIC = new Uint8Array([0x00, 0x61, 0x73, 0x6d]); // \0asm

describe('WasmClient.deploy', () => {
  it('uploads bytes and returns deploy result', async () => {
    const deployResult: WasmDeployResult = { hash: FAKE_HASH, size: 4, name: 'test-mod' };
    const fetchImpl = mockFetch(201, deployResult);
    const client = makeClient(fetchImpl);

    const result = await client.deploy(WASM_MAGIC, { name: 'test-mod' });

    expect(result.hash).toBe(FAKE_HASH);
    expect(result.size).toBe(4);
    expect(result.name).toBe('test-mod');
    expect(fetchImpl).toHaveBeenCalledWith(
      'http://localhost:8080/v1/compute/wasm/upload',
      expect.objectContaining({ method: 'POST' }),
    );
  });

  it('sends name and description in request body', async () => {
    const fetchImpl = mockFetch(201, { hash: FAKE_HASH, size: 4 });
    const client = makeClient(fetchImpl);

    await client.deploy(WASM_MAGIC, { name: 'my-mod', description: 'does things' });

    const [, init] = fetchImpl.mock.calls[0] as [string, RequestInit];
    const body = JSON.parse(init.body as string) as {
      wasm_bytes: string;
      name?: string;
      description?: string;
    };
    expect(body.name).toBe('my-mod');
    expect(body.description).toBe('does things');
    expect(typeof body.wasm_bytes).toBe('string');
  });

  it('accepts ArrayBuffer input', async () => {
    const fetchImpl = mockFetch(201, { hash: FAKE_HASH, size: 4 });
    const client = makeClient(fetchImpl);

    const ab = WASM_MAGIC.buffer;
    await expect(client.deploy(ab)).resolves.toBeDefined();
  });

  it('throws AuthenticationError when no token set', async () => {
    const fetchImpl = mockFetch(201, {});
    const client = new WasmClient({
      baseUrl: 'http://localhost:8080',
      fetch: fetchImpl as unknown as typeof fetch,
    });

    await expect(client.deploy(WASM_MAGIC)).rejects.toBeInstanceOf(AuthenticationError);
    expect(fetchImpl).not.toHaveBeenCalled();
  });

  it('throws ConflictError (409) when module already exists', async () => {
    const fetchImpl = mockFetch(409, { error: 'Already exists', code: 'CONFLICT' });
    const client = makeClient(fetchImpl);

    await expect(client.deploy(WASM_MAGIC)).rejects.toBeInstanceOf(ConflictError);
  });

  it('throws ValidationError (400) for invalid WASM bytes', async () => {
    const fetchImpl = mockFetch(400, {
      error: 'Invalid WASM',
      code: 'VALIDATION_FAILED',
      details: { fields: {} },
    });
    const client = makeClient(fetchImpl);

    await expect(client.deploy(new Uint8Array([0xff]))).rejects.toBeInstanceOf(ValidationError);
  });

  it('passes Authorization header', async () => {
    const fetchImpl = mockFetch(201, { hash: FAKE_HASH, size: 4 });
    const client = makeClient(fetchImpl, 'my-secret-token');

    await client.deploy(WASM_MAGIC);

    const [, init] = fetchImpl.mock.calls[0] as [string, RequestInit];
    expect((init.headers as Record<string, string>)['Authorization']).toBe('Bearer my-secret-token');
  });
});
