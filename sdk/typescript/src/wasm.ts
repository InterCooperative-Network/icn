/**
 * ICN WASM Module Client
 *
 * A focused client for deploying and querying WASM modules in the ICN
 * compute registry. Errors are mapped to the typed exception hierarchy
 * defined in types.ts (G0 ErrCode taxonomy).
 *
 * @example
 * ```typescript
 * const wasm = new WasmClient({ baseUrl: 'http://localhost:8080', token });
 *
 * const { hash } = await wasm.deploy(wasmBytes, { name: 'my-module' });
 * const meta = await wasm.get(hash);
 * const all = await wasm.list();
 * const mine = await wasm.list('did:icn:alice');
 * ```
 */

import {
  ErrorCode,
  ICNError,
  AuthenticationError,
  NetworkError,
  TimeoutError,
  createErrorFromResponse,
} from './types';

// ============================================================================
// Response types — shape matches gateway WasmMetadataResponse
// ============================================================================

/** Metadata for a WASM module as returned by the gateway */
export interface WasmModuleInfo {
  /** Blake3 hash of the module bytes (64-char hex) */
  hash: string;
  /** Human-readable name, if set at upload time */
  name?: string;
  /** Size of the module in bytes */
  size_bytes: number;
  /** DID of the account that deployed this module */
  owner: string;
  /** Unix timestamp (seconds) when the module was deployed */
  deployed_at: number;
  /** Optional description set at upload time */
  description?: string;
}

/** Result from a successful WASM deploy */
export interface WasmDeployResult {
  /** Blake3 hash of the deployed module (64-char hex) */
  hash: string;
  /** Size of the module in bytes */
  size: number;
  /** Name as stored, if provided */
  name?: string;
}

// ============================================================================
// Client options
// ============================================================================

export interface WasmClientOptions {
  /** Gateway base URL without /v1 prefix (e.g. "http://localhost:8080") */
  baseUrl: string;
  /** JWT bearer token for authenticated requests */
  token?: string;
  /** Request timeout in milliseconds (default: 30 000) */
  timeout?: number;
  /** Custom fetch implementation (useful in tests) */
  fetch?: typeof fetch;
}

// ============================================================================
// WasmClient
// ============================================================================

export class WasmClient {
  private readonly baseUrl: string;
  private token: string | undefined;
  private readonly timeout: number;
  private readonly fetchImpl: typeof fetch;

  constructor(options: WasmClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/$/, '');
    this.token = options.token;
    this.timeout = options.timeout ?? 30_000;
    this.fetchImpl = options.fetch ?? globalThis.fetch.bind(globalThis);
  }

  /** Set or replace the bearer token */
  setToken(token: string): void {
    this.token = token;
  }

  /** Remove the current token (subsequent calls will throw AuthenticationError) */
  clearToken(): void {
    this.token = undefined;
  }

  /**
   * Deploy a WASM module to the registry.
   *
   * Throws `ConflictError` (409) if the same module has already been deployed.
   * Throws `ValidationError` (400) if the bytes are not valid WASM.
   *
   * @param bytes - Raw WASM module bytes
   * @param options - Optional name and description metadata
   * @returns Hash and size of the deployed module
   */
  async deploy(
    bytes: Uint8Array | ArrayBuffer,
    options?: { name?: string; description?: string },
  ): Promise<WasmDeployResult> {
    const arr = bytes instanceof ArrayBuffer ? new Uint8Array(bytes) : bytes;
    const wasm_bytes = encodeBase64(arr);
    return this.request<WasmDeployResult>('POST', '/compute/wasm/upload', {
      wasm_bytes,
      name: options?.name,
      description: options?.description,
    });
  }

  /**
   * Fetch metadata for a WASM module by its hash.
   *
   * Throws `NotFoundError` (404) if no module with that hash exists.
   *
   * @param hash - 64-char hex Blake3 hash of the module
   */
  async get(hash: string): Promise<WasmModuleInfo> {
    return this.request<WasmModuleInfo>(
      'GET',
      `/compute/wasm/${encodeURIComponent(hash)}`,
    );
  }

  /**
   * List deployed WASM modules.
   *
   * @param owner - Optional DID to filter by deployer. Filtering is applied
   *   client-side because the gateway does not yet support server-side owner
   *   filtering.
   * @returns Array of matching module metadata
   */
  async list(owner?: string): Promise<WasmModuleInfo[]> {
    const modules = await this.request<WasmModuleInfo[]>(
      'GET',
      '/compute/wasm',
    );
    if (owner !== undefined) {
      return modules.filter((m) => m.owner === owner);
    }
    return modules;
  }

  // --------------------------------------------------------------------------
  // Internal HTTP helper
  // --------------------------------------------------------------------------

  private async request<T>(
    method: string,
    path: string,
    body?: unknown,
  ): Promise<T> {
    if (!this.token) {
      throw new AuthenticationError(
        'Authentication required',
        ErrorCode.AUTHENTICATION_REQUIRED,
      );
    }

    const url = `${this.baseUrl}/v1${path}`;
    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), this.timeout);

    try {
      const response = await this.fetchImpl(url, {
        method,
        headers: {
          'Content-Type': 'application/json',
          Authorization: `Bearer ${this.token}`,
        },
        body: body !== undefined ? JSON.stringify(body) : undefined,
        signal: controller.signal,
      });

      clearTimeout(timeoutId);

      if (!response.ok) {
        const errorBody = (await response
          .json()
          .catch(() => ({}))) as { error?: string; code?: string; details?: unknown };
        throw createErrorFromResponse(
          response.status,
          errorBody.error ?? response.statusText,
          errorBody.code,
          errorBody.details,
        );
      }

      return (await response.json()) as T;
    } catch (err) {
      clearTimeout(timeoutId);
      if (err instanceof ICNError) {
        throw err;
      }
      if ((err as Error).name === 'AbortError') {
        throw new TimeoutError('Request timeout', this.timeout);
      }
      throw new NetworkError((err as Error).message);
    }
  }
}

// ============================================================================
// Internal helpers
// ============================================================================

/**
 * Encode a Uint8Array as a base64 string.
 * Uses Buffer in Node.js for efficiency; falls back to btoa in browsers.
 */
function encodeBase64(bytes: Uint8Array): string {
  if (typeof Buffer !== 'undefined') {
    return Buffer.from(bytes).toString('base64');
  }
  // Browser: process in 32 KB chunks to avoid call-stack limits for large modules
  const CHUNK = 32_768;
  let binary = '';
  for (let i = 0; i < bytes.length; i += CHUNK) {
    const chunk = bytes.subarray(i, Math.min(i + CHUNK, bytes.length));
    binary += String.fromCharCode.apply(null, chunk as unknown as number[]);
  }
  return btoa(binary);
}
