/**
 * WasmClient typed error mapping tests
 *
 * Verifies that HTTP status codes from the gateway are translated into the
 * correct typed exception subclasses (G0 ErrCode taxonomy).
 */

import { WasmClient } from './wasm';
import {
  AuthenticationError,
  ConflictError,
  NotFoundError,
  NetworkError,
  RateLimitError,
  ServerError,
  ValidationError,
} from './types';

const WASM_MAGIC = new Uint8Array([0x00, 0x61, 0x73, 0x6d]);
const FAKE_HASH = 'a'.repeat(64);

function mockFetch(status: number, body: unknown) {
  return jest.fn().mockResolvedValue({
    ok: status < 400,
    status,
    statusText: 'Error',
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

describe('WasmClient typed error mapping', () => {
  it('throws AuthenticationError (401) — invalid token', async () => {
    const client = makeClient(
      mockFetch(401, { error: 'Unauthorized', code: 'AUTHENTICATION_REQUIRED' }),
    );
    const err = await client.deploy(WASM_MAGIC).catch((e) => e);
    expect(err).toBeInstanceOf(AuthenticationError);
    expect(err.statusCode).toBe(401);
  });

  it('throws AuthenticationError immediately when token is absent', async () => {
    const fetchImpl = mockFetch(200, {});
    const client = new WasmClient({
      baseUrl: 'http://localhost:8080',
      fetch: fetchImpl as unknown as typeof fetch,
    });
    await expect(client.get(FAKE_HASH)).rejects.toBeInstanceOf(AuthenticationError);
    expect(fetchImpl).not.toHaveBeenCalled();
  });

  it('throws ValidationError (400 VALIDATION_FAILED)', async () => {
    const client = makeClient(
      mockFetch(400, { error: 'Bad WASM', code: 'VALIDATION_FAILED', details: { fields: {} } }),
    );
    const err = await client.deploy(WASM_MAGIC).catch((e) => e);
    expect(err).toBeInstanceOf(ValidationError);
    expect(err.statusCode).toBe(400);
  });

  it('throws NotFoundError (404) for get', async () => {
    const client = makeClient(
      mockFetch(404, { error: 'Not found', code: 'NOT_FOUND' }),
    );
    const err = await client.get(FAKE_HASH).catch((e) => e);
    expect(err).toBeInstanceOf(NotFoundError);
    expect(err.statusCode).toBe(404);
  });

  it('throws ConflictError (409) for duplicate deploy', async () => {
    const client = makeClient(
      mockFetch(409, { error: 'Already exists', code: 'CONFLICT' }),
    );
    const err = await client.deploy(WASM_MAGIC).catch((e) => e);
    expect(err).toBeInstanceOf(ConflictError);
    expect(err.statusCode).toBe(409);
  });

  it('throws RateLimitError (429)', async () => {
    const client = makeClient(
      mockFetch(429, { error: 'Too many requests', code: 'RATE_LIMITED', details: { retry_after: 30 } }),
    );
    const err = await client.list().catch((e) => e);
    expect(err).toBeInstanceOf(RateLimitError);
    expect(err.statusCode).toBe(429);
    expect(err.retryAfter).toBe(30);
  });

  it('throws ServerError (500)', async () => {
    const client = makeClient(
      mockFetch(500, { error: 'Internal error', code: 'INTERNAL_ERROR' }),
    );
    const err = await client.list().catch((e) => e);
    expect(err).toBeInstanceOf(ServerError);
    expect(err.statusCode).toBe(500);
  });

  it('throws NetworkError when fetch rejects', async () => {
    const fetchImpl = jest.fn().mockRejectedValue(new Error('ECONNREFUSED'));
    const client = new WasmClient({
      baseUrl: 'http://localhost:8080',
      token: 'tok',
      fetch: fetchImpl as unknown as typeof fetch,
    });
    const err = await client.list().catch((e) => e);
    expect(err).toBeInstanceOf(NetworkError);
    expect(err.message).toContain('ECONNREFUSED');
  });

  it('error.code is propagated for client inspection', async () => {
    const client = makeClient(
      mockFetch(409, { error: 'WASM module already exists', code: 'CONFLICT' }),
    );
    const err = await client.deploy(WASM_MAGIC).catch((e) => e);
    expect(err.code).toBe('CONFLICT');
  });

  it('setToken / clearToken gates requests', async () => {
    const fetchImpl = mockFetch(200, []);
    const client = new WasmClient({
      baseUrl: 'http://localhost:8080',
      fetch: fetchImpl as unknown as typeof fetch,
    });

    await expect(client.list()).rejects.toBeInstanceOf(AuthenticationError);

    client.setToken('new-token');
    await expect(client.list()).resolves.toEqual([]);

    client.clearToken();
    await expect(client.list()).rejects.toBeInstanceOf(AuthenticationError);
  });
});
