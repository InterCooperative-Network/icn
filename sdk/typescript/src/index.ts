/**
 * ICN TypeScript SDK
 *
 * Client library for interacting with the ICN Gateway API.
 *
 * @example
 * ```typescript
 * import { ICNClient } from '@icn/client';
 *
 * const client = new ICNClient({
 *   baseUrl: 'http://localhost:8080',
 * });
 *
 * // Authenticate
 * const challenge = await client.getChallenge('did:icn:alice');
 * const signature = await signChallenge(challenge.challenge); // Your signing logic
 * const auth = await client.verify('did:icn:alice', signature, 'my-coop');
 *
 * // Now use authenticated client
 * client.setToken(auth.token);
 *
 * // Get balance
 * const balance = await client.getBalance('my-coop', 'did:icn:alice');
 * ```
 */

import WebSocket from 'ws';
import {
  ICNClientOptions,
  ICNError,
  ChallengeResponse,
  VerifyResponse,
  Cooperative,
  CreateCoopRequest,
  UpdateCoopRequest,
  Member,
  AddMemberRequest,
  UpdateMemberRequest,
  Balance,
  PaymentRequest,
  PaymentResponse,
  TransactionHistory,
  GovernanceDomain,
  CreateDomainRequest,
  Proposal,
  CreateProposalRequest,
  VoteTally,
  CastVoteRequest,
  ProposalOutcome,
  HealthResponse,
  WsMessage,
  SignatureProvider,
  SubmitTaskRequest,
  SubmitTaskResponse,
  ComputeTaskStatus,
  CancelTaskRequest,
  CancelTaskResponse,
  RetryOptions,
  WebSocketOptions,
} from './types';

export * from './types';

/** Default retry options */
const DEFAULT_RETRY: Required<RetryOptions> = {
  maxRetries: 3,
  initialDelayMs: 1000,
  maxDelayMs: 10000,
  backoffMultiplier: 2,
  jitterFactor: 0.1,
  retryableStatuses: [408, 429, 500, 502, 503, 504],
};

/**
 * ICN Gateway API Client
 */
export class ICNClient {
  private baseUrl: string;
  private token?: string;
  private tokenExpiresAt?: number;
  private timeout: number;
  private fetchImpl: typeof fetch;
  private retryOptions: Required<RetryOptions>;
  private autoRefresh: boolean;
  private refreshBeforeExpiry: number;
  private signer?: SignatureProvider;
  private did?: string;
  private coopId?: string;
  private scopes?: string[];

  constructor(options: ICNClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/$/, '');
    this.token = options.token;
    this.timeout = options.timeout ?? 30000;
    // Bind fetch to globalThis to preserve context when called as a method
    this.fetchImpl = options.fetch ?? globalThis.fetch.bind(globalThis);
    this.retryOptions = { ...DEFAULT_RETRY, ...options.retry };
    this.autoRefresh = options.autoRefresh ?? false;
    this.refreshBeforeExpiry = options.refreshBeforeExpiry ?? 60;
  }

  /**
   * Set the JWT token for authenticated requests
   */
  setToken(token: string, expiresAt?: number): void {
    this.token = token;
    this.tokenExpiresAt = expiresAt;
  }

  /**
   * Clear the JWT token and authentication state
   */
  clearToken(): void {
    this.token = undefined;
    this.tokenExpiresAt = undefined;
    this.signer = undefined;
    this.did = undefined;
    this.coopId = undefined;
    this.scopes = undefined;
  }

  /**
   * Check if client has a token set
   */
  hasToken(): boolean {
    return !!this.token;
  }

  /**
   * Check if the token is expired or about to expire
   */
  isTokenExpired(): boolean {
    if (!this.tokenExpiresAt) {
      return false; // Unknown expiration, assume valid
    }
    const now = Math.floor(Date.now() / 1000);
    return now >= this.tokenExpiresAt - this.refreshBeforeExpiry;
  }

  /**
   * Get the token expiration timestamp
   */
  getTokenExpiresAt(): number | undefined {
    return this.tokenExpiresAt;
  }

  /**
   * Calculate delay with exponential backoff and jitter
   */
  private calculateRetryDelay(attempt: number): number {
    const { initialDelayMs, maxDelayMs, backoffMultiplier, jitterFactor } = this.retryOptions;
    const baseDelay = Math.min(
      initialDelayMs * Math.pow(backoffMultiplier, attempt),
      maxDelayMs
    );
    const jitter = baseDelay * jitterFactor * (Math.random() * 2 - 1);
    return Math.max(0, baseDelay + jitter);
  }

  /**
   * Check if a request should be retried based on error
   */
  private shouldRetry(error: ICNError, attempt: number): boolean {
    if (attempt >= this.retryOptions.maxRetries) {
      return false;
    }
    return this.retryOptions.retryableStatuses.includes(error.statusCode);
  }

  /**
   * Sleep for a given duration
   */
  private sleep(ms: number): Promise<void> {
    return new Promise((resolve) => setTimeout(resolve, ms));
  }

  /**
   * Refresh the authentication token if expired
   */
  private async refreshTokenIfNeeded(): Promise<void> {
    if (!this.autoRefresh || !this.signer || !this.did) {
      return;
    }
    if (!this.isTokenExpired()) {
      return;
    }
    // Re-authenticate with stored credentials
    await this.authenticate(this.did, this.signer, this.coopId, this.scopes);
  }

  // ===========================================================================
  // Internal HTTP methods
  // ===========================================================================

  private async request<T>(
    method: string,
    path: string,
    body?: unknown,
    requireAuth = true
  ): Promise<T> {
    // Refresh token if needed before making request
    if (requireAuth) {
      await this.refreshTokenIfNeeded();
    }

    const url = `${this.baseUrl}/v1${path}`;

    let lastError: ICNError | undefined;
    for (let attempt = 0; attempt <= this.retryOptions.maxRetries; attempt++) {
      try {
        const result = await this.executeRequest<T>(url, method, body, requireAuth);
        return result;
      } catch (error) {
        if (error instanceof ICNError) {
          lastError = error;
          if (this.shouldRetry(error, attempt)) {
            const delay = this.calculateRetryDelay(attempt);
            await this.sleep(delay);
            continue;
          }
        }
        throw error;
      }
    }
    throw lastError!;
  }

  private async executeRequest<T>(
    url: string,
    method: string,
    body?: unknown,
    requireAuth = true
  ): Promise<T> {
    const headers: Record<string, string> = {
      'Content-Type': 'application/json',
    };

    if (requireAuth) {
      if (!this.token) {
        throw new ICNError('Authentication required', 401);
      }
      headers['Authorization'] = `Bearer ${this.token}`;
    }

    const controller = new AbortController();
    const timeoutId = setTimeout(() => controller.abort(), this.timeout);

    try {
      const response = await this.fetchImpl(url, {
        method,
        headers,
        body: body ? JSON.stringify(body) : undefined,
        signal: controller.signal,
      });

      clearTimeout(timeoutId);

      if (!response.ok) {
        const errorBody = await response.json().catch(() => ({})) as { error?: string; code?: string; details?: unknown };
        throw new ICNError(
          errorBody.error || response.statusText,
          response.status,
          errorBody.code,
          errorBody.details
        );
      }

      if (response.status === 204) {
        return undefined as T;
      }

      return await response.json() as T;
    } catch (error) {
      clearTimeout(timeoutId);
      if (error instanceof ICNError) {
        throw error;
      }
      if ((error as Error).name === 'AbortError') {
        throw new ICNError('Request timeout', 408);
      }
      throw new ICNError((error as Error).message, 0);
    }
  }

  private get<T>(path: string, requireAuth = true): Promise<T> {
    return this.request<T>('GET', path, undefined, requireAuth);
  }

  private post<T>(path: string, body?: unknown, requireAuth = true): Promise<T> {
    return this.request<T>('POST', path, body, requireAuth);
  }

  private put<T>(path: string, body?: unknown, requireAuth = true): Promise<T> {
    return this.request<T>('PUT', path, body, requireAuth);
  }

  private delete<T>(path: string, requireAuth = true): Promise<T> {
    return this.request<T>('DELETE', path, undefined, requireAuth);
  }

  // ===========================================================================
  // Authentication
  // ===========================================================================

  /**
   * Get an authentication challenge for a DID
   */
  async getChallenge(did: string): Promise<ChallengeResponse> {
    return this.post<ChallengeResponse>('/auth/challenge', { did }, false);
  }

  /**
   * Verify a signed challenge and get a JWT token
   */
  async verify(
    did: string,
    signature: string,
    coopId?: string,
    scopes?: string[]
  ): Promise<VerifyResponse> {
    return this.post<VerifyResponse>(
      '/auth/verify',
      { did, signature, coop_id: coopId, scopes },
      false
    );
  }

  /**
   * Complete authentication flow with a signature provider
   *
   * If autoRefresh is enabled in client options, stores the signer for automatic
   * token refresh when the token expires.
   */
  async authenticate(
    did: string,
    signer: SignatureProvider,
    coopId?: string,
    scopes?: string[]
  ): Promise<VerifyResponse> {
    const challenge = await this.getChallenge(did);
    const signature = await signer.sign(challenge.challenge);
    const auth = await this.verify(did, signature, coopId, scopes);
    this.setToken(auth.token, auth.expires_at);

    // Store credentials for auto-refresh
    if (this.autoRefresh) {
      this.signer = signer;
      this.did = did;
      this.coopId = coopId;
      this.scopes = scopes;
    }

    return auth;
  }

  // ===========================================================================
  // Health
  // ===========================================================================

  /**
   * Get gateway health status
   */
  async health(): Promise<HealthResponse> {
    return this.get<HealthResponse>('/health', false);
  }

  // ===========================================================================
  // Cooperatives
  // ===========================================================================

  /**
   * List all cooperatives the authenticated user has access to
   */
  async listCoops(): Promise<Cooperative[]> {
    return this.get<Cooperative[]>('/coops');
  }

  /**
   * Create a new cooperative
   */
  async createCoop(req: CreateCoopRequest): Promise<Cooperative> {
    return this.post<Cooperative>('/coops', req);
  }

  /**
   * Get cooperative by ID
   */
  async getCoop(coopId: string): Promise<Cooperative> {
    return this.get<Cooperative>(`/coops/${coopId}`);
  }

  /**
   * Update cooperative
   */
  async updateCoop(coopId: string, req: UpdateCoopRequest): Promise<Cooperative> {
    return this.put<Cooperative>(`/coops/${coopId}`, req);
  }

  /**
   * Delete cooperative
   */
  async deleteCoop(coopId: string): Promise<void> {
    return this.delete<void>(`/coops/${coopId}`);
  }

  /**
   * List cooperative members
   */
  async listMembers(coopId: string): Promise<Member[]> {
    return this.get<Member[]>(`/coops/${coopId}/members`);
  }

  /**
   * Add a member to cooperative
   */
  async addMember(coopId: string, req: AddMemberRequest): Promise<Member> {
    return this.post<Member>(`/coops/${coopId}/members`, req);
  }

  /**
   * Update member role
   */
  async updateMember(
    coopId: string,
    did: string,
    req: UpdateMemberRequest
  ): Promise<Member> {
    return this.put<Member>(`/coops/${coopId}/members/${encodeURIComponent(did)}`, req);
  }

  /**
   * Remove member from cooperative
   */
  async removeMember(coopId: string, did: string): Promise<void> {
    return this.delete<void>(`/coops/${coopId}/members/${encodeURIComponent(did)}`);
  }

  // ===========================================================================
  // Ledger
  // ===========================================================================

  /**
   * Get balance for a member
   */
  async getBalance(coopId: string, did: string): Promise<Balance> {
    return this.get<Balance>(`/ledger/${coopId}/balance/${encodeURIComponent(did)}`);
  }

  /**
   * Create a payment
   */
  async pay(coopId: string, req: PaymentRequest): Promise<PaymentResponse> {
    return this.post<PaymentResponse>(`/ledger/${coopId}/payment`, req);
  }

  /**
   * Get transaction history
   */
  async getHistory(
    coopId: string,
    options?: { offset?: number; limit?: number }
  ): Promise<TransactionHistory> {
    const params = new URLSearchParams();
    if (options?.offset !== undefined) {
      params.set('offset', options.offset.toString());
    }
    if (options?.limit !== undefined) {
      params.set('limit', options.limit.toString());
    }
    const query = params.toString();
    const path = `/ledger/${coopId}/history${query ? `?${query}` : ''}`;
    return this.get<TransactionHistory>(path);
  }

  // ===========================================================================
  // Governance
  // ===========================================================================

  /**
   * Create a governance domain
   */
  async createDomain(req: CreateDomainRequest): Promise<GovernanceDomain> {
    return this.post<GovernanceDomain>('/gov/domains', req);
  }

  /**
   * Get governance domain
   */
  async getDomain(domainId: string): Promise<GovernanceDomain> {
    return this.get<GovernanceDomain>(`/gov/domains/${encodeURIComponent(domainId)}`);
  }

  /**
   * List governance domains
   */
  async listDomains(): Promise<GovernanceDomain[]> {
    return this.get<GovernanceDomain[]>('/gov/domains');
  }

  /**
   * Create a proposal
   */
  async createProposal(req: CreateProposalRequest): Promise<Proposal> {
    return this.post<Proposal>('/gov/proposals', req);
  }

  /**
   * Get proposal
   */
  async getProposal(proposalId: string): Promise<Proposal> {
    return this.get<Proposal>(`/gov/proposals/${proposalId}`);
  }

  /**
   * List proposals
   */
  async listProposals(domainId?: string, state?: string): Promise<Proposal[]> {
    const params = new URLSearchParams();
    if (domainId) params.set('domain_id', domainId);
    if (state) params.set('state', state);
    const query = params.toString();
    return this.get<Proposal[]>(`/gov/proposals${query ? `?${query}` : ''}`);
  }

  /**
   * Open a proposal for voting
   */
  async openProposal(proposalId: string): Promise<Proposal> {
    return this.post<Proposal>(`/gov/proposals/${proposalId}/open`);
  }

  /**
   * Close a proposal and calculate outcome
   */
  async closeProposal(proposalId: string): Promise<ProposalOutcome> {
    return this.post<ProposalOutcome>(`/gov/proposals/${proposalId}/close`);
  }

  /**
   * Cast a vote on a proposal
   */
  async vote(proposalId: string, req: CastVoteRequest): Promise<void> {
    return this.post<void>(`/gov/proposals/${proposalId}/vote`, req);
  }

  /**
   * Get vote tally for a proposal
   */
  async getVotes(proposalId: string): Promise<VoteTally> {
    return this.get<VoteTally>(`/gov/proposals/${proposalId}/votes`);
  }

  // ===========================================================================
  // Compute
  // ===========================================================================

  /**
   * Submit a compute task
   *
   * @example
   * ```typescript
   * // Submit CCL contract (default)
   * const result = await client.submitTask({
   *   code: JSON.stringify(cclContract),
   *   fuel_limit: 10000,
   *   payment_rate: 100, // 100 credits per 1000 fuel
   * });
   * console.log('Task submitted:', result.task_hash);
   *
   * // Submit WASM module
   * const wasmResult = await client.submitTask({
   *   code_type: 'wasm',
   *   wasm_bytes: btoa(String.fromCharCode(...wasmBytes)), // Base64 encoded
   *   fuel_limit: 10000,
   * });
   * ```
   */
  async submitTask(req: SubmitTaskRequest): Promise<SubmitTaskResponse> {
    return this.post<SubmitTaskResponse>('/compute/submit', req);
  }

  /**
   * Submit a WASM compute task
   *
   * Helper method that handles base64 encoding of WASM bytes.
   *
   * @example
   * ```typescript
   * // From ArrayBuffer
   * const wasmBytes = await fetch('/module.wasm').then(r => r.arrayBuffer());
   * const result = await client.submitWasmTask(new Uint8Array(wasmBytes), {
   *   fuel_limit: 10000,
   * });
   *
   * // From Uint8Array
   * const result = await client.submitWasmTask(wasmModule, {
   *   fuel_limit: 10000,
   *   inputs: { x: 42 },
   * });
   * ```
   */
  async submitWasmTask(
    wasmBytes: Uint8Array | ArrayBuffer,
    options?: Omit<SubmitTaskRequest, 'code' | 'wasm_bytes' | 'code_type'>
  ): Promise<SubmitTaskResponse> {
    const bytes = wasmBytes instanceof ArrayBuffer
      ? new Uint8Array(wasmBytes)
      : wasmBytes;

    // Convert to base64 - handle large arrays that exceed JS argument limits
    let base64: string;
    if (typeof Buffer !== 'undefined') {
      // Node.js environment - use Buffer for efficiency
      base64 = Buffer.from(bytes).toString('base64');
    } else if (typeof btoa !== 'undefined') {
      // Browser environment - process in chunks to avoid call stack limits
      // String.fromCharCode(...bytes) fails for arrays >65KB due to argument limits
      const CHUNK_SIZE = 32768; // 32KB chunks - safe for all JS engines
      let binary = '';
      for (let i = 0; i < bytes.length; i += CHUNK_SIZE) {
        const chunk = bytes.subarray(i, Math.min(i + CHUNK_SIZE, bytes.length));
        binary += String.fromCharCode.apply(null, chunk as unknown as number[]);
      }
      base64 = btoa(binary);
    } else {
      throw new Error('No base64 encoding method available');
    }

    return this.submitTask({
      ...options,
      code_type: 'wasm',
      wasm_bytes: base64,
    });
  }

  /**
   * Get the status of a compute task
   *
   * @example
   * ```typescript
   * const status = await client.getTaskStatus(taskHash);
   * if (status.status === 'completed') {
   *   console.log('Result:', status.result?.output);
   * }
   * ```
   */
  async getTaskStatus(taskHash: string): Promise<ComputeTaskStatus> {
    return this.get<ComputeTaskStatus>(`/compute/status/${taskHash}`);
  }

  /**
   * Poll for task completion
   *
   * @param taskHash - Task hash to poll
   * @param intervalMs - Polling interval in milliseconds (default 1000)
   * @param timeoutMs - Maximum time to wait (default 60000)
   */
  async waitForTask(
    taskHash: string,
    intervalMs = 1000,
    timeoutMs = 60000
  ): Promise<ComputeTaskStatus> {
    const start = Date.now();
    while (Date.now() - start < timeoutMs) {
      const status = await this.getTaskStatus(taskHash);
      if (status.status === 'completed' || status.status === 'failed' || status.status === 'cancelled') {
        return status;
      }
      await new Promise((resolve) => setTimeout(resolve, intervalMs));
    }
    throw new ICNError('Task polling timeout', 408);
  }

  /**
   * Cancel a compute task
   *
   * @example
   * ```typescript
   * await client.cancelTask(taskHash, {
   *   reason: 'No longer needed'
   * });
   * ```
   */
  async cancelTask(
    taskHash: string,
    req?: CancelTaskRequest
  ): Promise<CancelTaskResponse> {
    return this.post<CancelTaskResponse>(
      `/compute/cancel/${taskHash}`,
      req || {}
    );
  }

  // ===========================================================================
  // WebSocket
  // ===========================================================================

  /**
   * Connect to WebSocket for real-time events (basic, no auto-reconnect)
   */
  connectWebSocket(
    coopId: string,
    handlers: {
      onOpen?: () => void;
      onMessage?: (message: WsMessage) => void;
      onError?: (error: Error) => void;
      onClose?: () => void;
    }
  ): WebSocket {
    const wsUrl = this.baseUrl.replace(/^http/, 'ws');
    const ws = new WebSocket(`${wsUrl}/v1/ws/${coopId}`);

    ws.on('open', () => {
      // Send auth message
      if (this.token) {
        ws.send(JSON.stringify({ type: 'Auth', token: this.token }));
      }
      handlers.onOpen?.();
    });

    ws.on('message', (data) => {
      try {
        const message = JSON.parse(data.toString()) as WsMessage;
        handlers.onMessage?.(message);
      } catch (error) {
        handlers.onError?.(error as Error);
      }
    });

    ws.on('error', (error) => {
      handlers.onError?.(error);
    });

    ws.on('close', () => {
      handlers.onClose?.();
    });

    return ws;
  }

  /**
   * Create a managed WebSocket subscription with auto-reconnect
   *
   * @example
   * ```typescript
   * const subscription = client.subscribe('my-coop', {
   *   onEvent: (event) => console.log('Event:', event),
   *   onReconnect: (attempt) => console.log('Reconnecting...', attempt),
   * });
   *
   * // Later: close the subscription
   * subscription.close();
   * ```
   */
  subscribe(
    coopId: string,
    handlers: {
      onEvent?: (event: WsMessage) => void;
      onAuthOk?: (did: string) => void;
      onError?: (error: Error) => void;
      onReconnect?: (attempt: number) => void;
      onDisconnect?: () => void;
    },
    options?: WebSocketOptions
  ): ICNSubscription {
    return new ICNSubscription(this, coopId, handlers, options);
  }
}

/** Default WebSocket options */
const DEFAULT_WS_OPTIONS: Required<WebSocketOptions> = {
  autoReconnect: true,
  maxReconnectAttempts: 10,
  reconnectDelayMs: 1000,
  maxReconnectDelayMs: 30000,
};

/**
 * Managed WebSocket subscription with auto-reconnect
 */
export class ICNSubscription {
  private client: ICNClient;
  private coopId: string;
  private handlers: {
    onEvent?: (event: WsMessage) => void;
    onAuthOk?: (did: string) => void;
    onError?: (error: Error) => void;
    onReconnect?: (attempt: number) => void;
    onDisconnect?: () => void;
  };
  private options: Required<WebSocketOptions>;
  private ws?: WebSocket;
  private reconnectAttempt = 0;
  private closed = false;
  private reconnectTimer?: ReturnType<typeof setTimeout>;

  constructor(
    client: ICNClient,
    coopId: string,
    handlers: {
      onEvent?: (event: WsMessage) => void;
      onAuthOk?: (did: string) => void;
      onError?: (error: Error) => void;
      onReconnect?: (attempt: number) => void;
      onDisconnect?: () => void;
    },
    options?: WebSocketOptions
  ) {
    this.client = client;
    this.coopId = coopId;
    this.handlers = handlers;
    this.options = { ...DEFAULT_WS_OPTIONS, ...options };
    this.connect();
  }

  private connect(): void {
    if (this.closed) return;

    this.ws = this.client.connectWebSocket(this.coopId, {
      onOpen: () => {
        // Reset reconnect counter on successful connection
        this.reconnectAttempt = 0;
      },
      onMessage: (message) => {
        if (message.type === 'AuthOk') {
          this.handlers.onAuthOk?.(message.did);
        } else if (message.type === 'Error') {
          this.handlers.onError?.(new Error(message.message));
        } else {
          this.handlers.onEvent?.(message);
        }
      },
      onError: (error) => {
        this.handlers.onError?.(error);
      },
      onClose: () => {
        this.handlers.onDisconnect?.();
        this.scheduleReconnect();
      },
    });
  }

  private scheduleReconnect(): void {
    if (this.closed || !this.options.autoReconnect) return;

    if (this.reconnectAttempt >= this.options.maxReconnectAttempts) {
      this.handlers.onError?.(new Error('Max reconnection attempts reached'));
      return;
    }

    this.reconnectAttempt++;
    this.handlers.onReconnect?.(this.reconnectAttempt);

    // Calculate delay with exponential backoff
    const delay = Math.min(
      this.options.reconnectDelayMs * Math.pow(2, this.reconnectAttempt - 1),
      this.options.maxReconnectDelayMs
    );

    this.reconnectTimer = setTimeout(() => {
      this.connect();
    }, delay);
  }

  /**
   * Check if the subscription is connected
   */
  isConnected(): boolean {
    return this.ws?.readyState === WebSocket.OPEN;
  }

  /**
   * Close the subscription and stop reconnecting
   */
  close(): void {
    this.closed = true;
    if (this.reconnectTimer) {
      clearTimeout(this.reconnectTimer);
    }
    if (this.ws) {
      this.ws.close();
      this.ws = undefined;
    }
  }

  /**
   * Send a message through the WebSocket
   */
  send(message: WsMessage): void {
    if (this.ws?.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify(message));
    }
  }
}

/**
 * Create an ICN client
 */
export function createClient(options: ICNClientOptions): ICNClient {
  return new ICNClient(options);
}
