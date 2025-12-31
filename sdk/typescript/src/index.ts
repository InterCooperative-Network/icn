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

// Use global WebSocket (available in browsers, React Native, and Node.js 18+)
// For older Node.js, users must polyfill global.WebSocket with 'ws' package
declare const WebSocket: {
  new(url: string): WebSocket;
  prototype: WebSocket;
  readonly CONNECTING: 0;
  readonly OPEN: 1;
  readonly CLOSING: 2;
  readonly CLOSED: 3;
};
import {
  ICNClientOptions,
  ICNError,
  ErrorCode,
  createErrorFromResponse,
  AuthenticationError,
  TimeoutError,
  NetworkError,
  ChallengeResponse,
  VerifyResponse,
  Cooperative,
  CreateCoopRequest,
  UpdateCoopRequest,
  CoopStatsResponse,
  Member,
  MemberProfile,
  AddMemberRequest,
  UpdateMemberRequest,
  Balance,
  PaymentRequest,
  PaymentResponse,
  CrossPaymentRequest,
  CrossPaymentResponse,
  CrossPaymentQuoteRequest,
  CrossPaymentQuote,
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
  WsAuthOkMessage,
  WsEventMessage,
  WsBackfillMessage,
  WsBackfillCompleteMessage,
  WsErrorMessage,
  WsShutdownMessage,
  SignatureProvider,
  SubmitTaskRequest,
  SubmitTaskResponse,
  ComputeTaskStatus,
  CancelTaskRequest,
  CancelTaskResponse,
  RetryOptions,
  WebSocketOptions,
  // Commons Evolution types
  Charter,
  CharterSummary,
  CreateCharterRequest,
  CharterSignResponse,
  CharterActionResponse,
  CommonsHolder,
  MemberListResponse,
  MembershipActionResponse,
  CapabilityCheckResponse,
  Amendment,
  AmendmentListResponse,
  CreateAmendmentRequest,
  AddAmendmentChangeRequest,
  RatifyAmendmentRequest,
  // Amendment voting types
  CastAmendmentVoteRequest,
  CastAmendmentVoteResponse,
  AmendmentVoteResults,
  MyAmendmentVoteResponse,
  ListAmendmentVotesResponse,
  Appeal,
  AppealListResponse,
  FileAppealRequest,
  AddEvidenceRequest,
  AddResponseRequest,
  ResolveAppealRequest,
  // Identity and device types
  DidResolutionResponse,
  IdentityHealthResponse,
  DeviceInfo,
  RegisterDeviceRequest,
  RegisterDeviceResponse,
  ListDevicesResponse,
  RevokeDeviceRequest,
  RevokeDeviceResponse,
  // Vote delegation types
  CreateDelegationRequest,
  DelegationResponse,
  DelegationListResponse,
  // Exchange rate oracle types
  ExchangeRateResponse,
  ConvertAmountRequest,
  ConvertAmountResponse,
  ListRateSourcesResponse,
  SetManualRateRequest,
  SetManualRateResponse,
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

// ===========================================================================
// Input Validation Helpers
// ===========================================================================

/**
 * Validate a DID (Decentralized Identifier) format.
 *
 * ICN DIDs follow the pattern: did:icn:<multibase-encoded-public-key>
 * where the key portion is base58btc encoded (starts with 'z').
 *
 * Accepts both ICN DIDs (strict 3-part format) and other DID methods
 * (for interoperability with external systems).
 *
 * @throws ICNError if the DID format is invalid
 */
function validateDid(did: string, fieldName = 'DID'): void {
  if (!did || typeof did !== 'string') {
    throw new ICNError(`${fieldName} is required`, 400, 'INVALID_DID');
  }

  // Basic format check: did:method:identifier
  const parts = did.split(':');
  if (parts.length < 3 || parts[0] !== 'did') {
    throw new ICNError(`${fieldName} must be a valid DID (did:method:identifier)`, 400, 'INVALID_DID');
  }

  const method = parts[1];

  // ICN-specific validation (strict 3-part format)
  if (method === 'icn') {
    // ICN DIDs must be exactly: did:icn:<key>
    if (parts.length !== 3) {
      throw new ICNError(`${fieldName} must be exactly did:icn:<key> format`, 400, 'INVALID_DID');
    }

    const key = parts[2];
    // Key portion should be base58btc encoded (typically 43-44 chars for Ed25519)
    if (!key || key.length < 10) {
      throw new ICNError(`${fieldName} has invalid key portion`, 400, 'INVALID_DID');
    }

    // Multibase prefix check: ICN DIDs use base58btc encoding (multibase prefix 'z').
    // For backwards compatibility with legacy DIDs, we also accept raw base58 without
    // the 'z' prefix. Base58 charset excludes 0, O, I, l to avoid ambiguity.
    // New DIDs should use multibase format: did:icn:z<base58btc-key>
    if (!key.startsWith('z') && !/^[1-9A-HJ-NP-Za-km-z]+$/.test(key)) {
      throw new ICNError(`${fieldName} has invalid key encoding`, 400, 'INVALID_DID');
    }
  }
  // Other DID methods (did:key, did:web, etc.) - allow more parts
}

/**
 * Validate delegation scope format.
 *
 * Valid formats:
 * - undefined or 'blanket' - blanket delegation
 * - 'domain:<domain-id>' - domain-scoped delegation
 * - 'proposal:<proposal-id>' - proposal-scoped delegation
 *
 * @throws ICNError if the scope format is invalid
 */
function validateDelegationScope(scope: string | undefined): void {
  if (scope === undefined || scope === 'blanket') {
    return; // Valid blanket delegation
  }

  if (typeof scope !== 'string') {
    throw new ICNError('scope must be a string', 400, 'INVALID_SCOPE');
  }

  const validPrefixes = ['domain:', 'proposal:'];
  const hasValidPrefix = validPrefixes.some(prefix => scope.startsWith(prefix));

  if (!hasValidPrefix) {
    throw new ICNError(
      `scope must be 'blanket', 'domain:<id>', or 'proposal:<id>' (got: ${scope})`,
      400,
      'INVALID_SCOPE'
    );
  }

  // Validate that the ID portion is non-empty
  const colonIndex = scope.indexOf(':');
  const id = scope.slice(colonIndex + 1);
  if (!id || id.length === 0) {
    throw new ICNError('scope ID cannot be empty', 400, 'INVALID_SCOPE');
  }
}

/**
 * Validate expiration timestamp.
 *
 * @throws ICNError if the expiration is in the past or too far in the future
 */
function validateExpiration(expiresAt: number | undefined): void {
  if (expiresAt === undefined) {
    return; // Optional field
  }

  if (typeof expiresAt !== 'number' || !Number.isInteger(expiresAt)) {
    throw new ICNError('expires_at must be an integer timestamp', 400, 'INVALID_EXPIRATION');
  }

  const now = Math.floor(Date.now() / 1000);

  if (expiresAt <= now) {
    throw new ICNError('expires_at must be in the future', 400, 'INVALID_EXPIRATION');
  }

  // Max 10 years in the future
  const maxExpiry = now + (10 * 365 * 24 * 60 * 60);
  if (expiresAt > maxExpiry) {
    throw new ICNError('expires_at cannot be more than 10 years in the future', 400, 'INVALID_EXPIRATION');
  }
}

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
        throw new AuthenticationError('Authentication required', ErrorCode.AUTHENTICATION_REQUIRED);
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
        throw createErrorFromResponse(
          response.status,
          errorBody.error || response.statusText,
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
        throw new TimeoutError('Request timeout', this.timeout);
      }
      throw new NetworkError((error as Error).message, ErrorCode.NETWORK_ERROR);
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

  private delete<T>(path: string, body?: unknown, requireAuth = true): Promise<T> {
    return this.request<T>('DELETE', path, body, requireAuth);
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
    const response = await this.post<{ token: string; expires_in: number }>(
      '/auth/verify',
      { did, signature, coop_id: coopId, scopes },
      false
    );
    // Convert expires_in (seconds) to expires_at (timestamp ms)
    return {
      ...response,
      expires_at: Date.now() + response.expires_in * 1000,
    };
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
    const signature = await signer.sign(challenge.nonce);
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
   * Get public statistics for a cooperative (no authentication required)
   *
   * Returns aggregate statistics like total members, transaction count,
   * and hours exchanged. Safe to expose publicly as it only contains
   * aggregate data, not individual transaction details.
   *
   * @example
   * ```typescript
   * const stats = await client.getCoopStats('food-coop');
   * console.log(`${stats.total_members} members, ${stats.transaction_count} transactions`);
   * ```
   */
  async getCoopStats(coopId: string): Promise<CoopStatsResponse> {
    return this.get<CoopStatsResponse>(`/coops/${coopId}/stats`, false);
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

  /**
   * Get member profile
   */
  async getMemberProfile(coopId: string, did: string): Promise<MemberProfile> {
    return this.get<MemberProfile>(`/members/${coopId}/${encodeURIComponent(did)}`);
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
   * Create a cross-currency payment
   *
   * Executes a payment where the sender pays in one currency and the recipient
   * receives in another. Uses the exchange rate oracle for conversion.
   *
   * @param coopId - Cooperative ID where the payment is recorded
   * @param req - Cross-currency payment request
   * @returns Promise resolving to the payment response with conversion details
   *
   * @throws {ValidationError} If amount <= 0, currencies are invalid, or same currency used
   * @throws {AuthorizationError} If authenticated DID doesn't match the sender
   * @throws {ICNError} With `code: 'SLIPPAGE_EXCEEDED'` if converted amount exceeds max_target_amount
   * @throws {ICNError} With `code: 'STALE_RATE'` if exchange rate is stale and stale rates not allowed
   * @throws {ICNError} With `code: 'RATE_EXPIRED'` if prepared transfer expired before execution
   * @throws {ICNError} With `code: 'BUDGET_EXCEEDED'` if sender exceeds budget limits
   * @throws {ICNError} With `code: 'INSUFFICIENT_BALANCE'` if sender has insufficient funds
   *
   * @example
   * ```typescript
   * // Send 10 hours, recipient receives USD equivalent
   * try {
   *   const result = await client.crossPay('my-coop', {
   *     from: 'did:icn:alice...',
   *     to: 'did:icn:bob...',
   *     amount: 10,
   *     from_currency: 'hours',
   *     to_currency: 'USD',
   *     max_target_amount: 260, // Slippage protection
   *   });
   *   console.log(`Bob received ${result.net_target_amount} USD`);
   *   console.log(`Exchange rate: ${result.rate_used}`);
   *   console.log(`Fee charged: ${result.fee_amount} USD`);
   * } catch (e) {
   *   if (e instanceof ValidationError) {
   *     console.error('Invalid request:', e.fieldErrors);
   *   } else if (e instanceof AuthorizationError) {
   *     console.error('Not authorized to send from this account');
   *   } else if (e instanceof ICNError && e.code === 'SLIPPAGE_EXCEEDED') {
   *     console.error('Rate changed too much, try again');
   *   }
   * }
   * ```
   */
  async crossPay(coopId: string, req: CrossPaymentRequest): Promise<CrossPaymentResponse> {
    return this.post<CrossPaymentResponse>(`/ledger/${coopId}/payment/convert`, req);
  }

  /**
   * Get a cross-currency payment quote
   *
   * Returns a preview of what a cross-currency payment would look like without
   * actually executing it. Useful for showing users the expected conversion
   * before committing to a transaction.
   *
   * @param coopId - Cooperative ID where the quote is calculated
   * @param req - Quote request with amount and currencies
   * @returns Promise resolving to the quote with rate, fees, and validity info
   *
   * @throws {ValidationError} If amount <= 0, currencies are invalid, or same currency used
   * @throws {ICNError} With `code: 'ORACLE_NOT_CONFIGURED'` if exchange rate oracle not set up
   * @throws {ICNError} With `code: 'RATE_NOT_AVAILABLE'` if no rate exists for the currency pair
   *
   * @example
   * ```typescript
   * // Get quote for converting 10 hours to USD
   * const quote = await client.getCrossPaymentQuote('my-coop', {
   *   amount: 10,
   *   from_currency: 'hours',
   *   to_currency: 'USD',
   * });
   *
   * console.log(`Rate: ${quote.rate}`);
   * console.log(`Gross amount: ${quote.gross_target_amount} USD`);
   * console.log(`Fee: ${quote.fee_amount} USD`);
   * console.log(`Net amount: ${quote.net_target_amount} USD`);
   * console.log(`Valid until: ${new Date(quote.valid_until * 1000)}`);
   *
   * if (quote.is_stale) {
   *   console.warn('Warning: Rate may be outdated');
   * }
   *
   * // Use quote.net_target_amount as max_target_amount for slippage protection
   * const result = await client.crossPay('my-coop', {
   *   ...paymentDetails,
   *   max_target_amount: Math.floor(quote.net_target_amount * 1.01), // 1% slippage tolerance
   * });
   * ```
   */
  async getCrossPaymentQuote(coopId: string, req: CrossPaymentQuoteRequest): Promise<CrossPaymentQuote> {
    return this.post<CrossPaymentQuote>(`/ledger/${coopId}/payment/convert/quote`, req);
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
  // Vote Delegation
  // ===========================================================================

  /**
   * Create a vote delegation
   *
   * Allows the authenticated user to delegate their voting power to another DID.
   *
   * @example
   * ```typescript
   * // Blanket delegation - delegate can vote on all proposals
   * await client.createDelegation({
   *   delegate: 'did:icn:delegate123',
   *   scope: 'blanket',
   * });
   *
   * // Domain-scoped delegation - delegate can only vote in specific domain
   * await client.createDelegation({
   *   delegate: 'did:icn:delegate123',
   *   scope: 'domain:my-governance-domain',
   *   expires_at: Math.floor(Date.now() / 1000) + 86400, // Expires in 24 hours
   * });
   *
   * // Proposal-scoped delegation - delegate can only vote on specific proposal
   * await client.createDelegation({
   *   delegate: 'did:icn:delegate123',
   *   scope: 'proposal:prop-123',
   * });
   * ```
   */
  async createDelegation(req: CreateDelegationRequest): Promise<DelegationResponse> {
    // Client-side validation
    validateDid(req.delegate, 'delegate');
    validateDelegationScope(req.scope);
    validateExpiration(req.expires_at);

    return this.post<DelegationResponse>('/gov/delegations', req);
  }

  /**
   * List delegations for the authenticated user
   *
   * Returns both delegations given by and received by the user.
   *
   * @param includeRevoked - Whether to include revoked delegations (default: false)
   * @example
   * ```typescript
   * // Get active delegations only
   * const { given, received } = await client.listDelegations();
   * console.log('Delegations given:', given.length);
   * console.log('Delegations received:', received.length);
   *
   * // Include revoked delegations
   * const all = await client.listDelegations(true);
   * ```
   */
  async listDelegations(includeRevoked = false): Promise<DelegationListResponse> {
    const query = includeRevoked ? '?include_revoked=true' : '';
    return this.get<DelegationListResponse>(`/gov/delegations${query}`);
  }

  /**
   * Get a specific delegation by ID
   *
   * Only the delegator or delegate can view a delegation (for privacy).
   */
  async getDelegation(delegationId: string): Promise<DelegationResponse> {
    return this.get<DelegationResponse>(`/gov/delegations/${encodeURIComponent(delegationId)}`);
  }

  /**
   * Revoke a delegation
   *
   * Only the delegator can revoke their delegation.
   *
   * @example
   * ```typescript
   * await client.revokeDelegation('delegation-123');
   * ```
   */
  async revokeDelegation(delegationId: string): Promise<void> {
    return this.delete<void>(`/gov/delegations/${encodeURIComponent(delegationId)}`);
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
    throw new TimeoutError('Task polling timeout');
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
  // Charter (Commons Evolution)
  // ===========================================================================

  /**
   * Create a new charter
   */
  async createCharter(req: CreateCharterRequest): Promise<Charter> {
    return this.post<Charter>('/charter', req);
  }

  /**
   * Get charter by ID
   */
  async getCharter(charterId: string): Promise<Charter> {
    return this.get<Charter>(`/charter/${charterId}`, false);
  }

  /**
   * Get charter by domain ID
   */
  async getCharterByDomain(domainId: string): Promise<Charter> {
    return this.get<Charter>(`/charter/by-domain/${domainId}`, false);
  }

  /**
   * List charters
   */
  async listCharters(orgType?: string, status?: string): Promise<CharterSummary[]> {
    const params = new URLSearchParams();
    if (orgType) params.set('org_type', orgType);
    if (status) params.set('status', status);
    const query = params.toString();
    return this.get<CharterSummary[]>(`/charter${query ? `?${query}` : ''}`, false);
  }

  /**
   * Sign a charter as a founder
   */
  async signCharter(
    charterId: string,
    signature: string,
    role?: string
  ): Promise<CharterSignResponse> {
    return this.post<CharterSignResponse>(`/charter/${charterId}/sign`, { signature, role });
  }

  /**
   * Activate a charter
   */
  async activateCharter(charterId: string): Promise<CharterActionResponse> {
    return this.post<CharterActionResponse>(`/charter/${charterId}/activate`);
  }

  /**
   * Update charter status
   */
  async updateCharterStatus(
    charterId: string,
    status: string,
    reason?: string
  ): Promise<CharterActionResponse> {
    return this.put<CharterActionResponse>(`/charter/${charterId}/status`, { status, reason });
  }

  // ===========================================================================
  // Membership (Commons Evolution)
  // ===========================================================================

  /**
   * Apply for membership in a jurisdiction
   */
  async applyForMembership(
    jurisdictionId: string,
    capabilitiesRequested?: string[]
  ): Promise<CommonsHolder> {
    return this.post<CommonsHolder>('/membership/apply', {
      jurisdiction_id: jurisdictionId,
      capabilities_requested: capabilitiesRequested || [],
    });
  }

  /**
   * Get membership status
   */
  async getMembershipStatus(jurisdictionId: string): Promise<CommonsHolder> {
    return this.get<CommonsHolder>(`/membership/status/${jurisdictionId}`);
  }

  /**
   * List members of a jurisdiction
   */
  async listJurisdictionMembers(
    jurisdictionId: string,
    status?: string
  ): Promise<MemberListResponse> {
    const params = new URLSearchParams();
    if (status) params.set('status', status);
    const query = params.toString();
    return this.get<MemberListResponse>(
      `/membership/list/${jurisdictionId}${query ? `?${query}` : ''}`
    );
  }

  /**
   * Approve a membership application (admin)
   */
  async approveMembership(
    holderId: string,
    jurisdictionId: string
  ): Promise<MembershipActionResponse> {
    return this.post<MembershipActionResponse>('/membership/approve', {
      holder_id: holderId,
      jurisdiction_id: jurisdictionId,
    });
  }

  /**
   * Promote a member (admin)
   */
  async promoteMember(
    holderId: string,
    jurisdictionId: string
  ): Promise<MembershipActionResponse> {
    return this.post<MembershipActionResponse>('/membership/promote', {
      holder_id: holderId,
      jurisdiction_id: jurisdictionId,
    });
  }

  /**
   * Suspend a member (admin)
   */
  async suspendMember(
    holderId: string,
    jurisdictionId: string
  ): Promise<MembershipActionResponse> {
    return this.post<MembershipActionResponse>('/membership/suspend', {
      holder_id: holderId,
      jurisdiction_id: jurisdictionId,
    });
  }

  /**
   * Reinstate a member (admin)
   */
  async reinstateMember(
    holderId: string,
    jurisdictionId: string
  ): Promise<MembershipActionResponse> {
    return this.post<MembershipActionResponse>('/membership/reinstate', {
      holder_id: holderId,
      jurisdiction_id: jurisdictionId,
    });
  }

  /**
   * Exit membership voluntarily
   */
  async exitMembership(
    holderId: string,
    jurisdictionId: string
  ): Promise<MembershipActionResponse> {
    return this.post<MembershipActionResponse>('/membership/exit', {
      holder_id: holderId,
      jurisdiction_id: jurisdictionId,
    });
  }

  /**
   * Grant a capability (admin)
   */
  async grantCapability(
    holderId: string,
    jurisdictionId: string,
    capability: string
  ): Promise<MembershipActionResponse> {
    return this.post<MembershipActionResponse>('/membership/capability/grant', {
      holder_id: holderId,
      jurisdiction_id: jurisdictionId,
      capability,
    });
  }

  /**
   * Revoke a capability (admin)
   */
  async revokeCapability(
    holderId: string,
    jurisdictionId: string,
    capability: string
  ): Promise<MembershipActionResponse> {
    return this.post<MembershipActionResponse>('/membership/capability/revoke', {
      holder_id: holderId,
      jurisdiction_id: jurisdictionId,
      capability,
    });
  }

  /**
   * Check if a member has a capability
   */
  async checkCapability(
    holderId: string,
    jurisdictionId: string,
    capability: string
  ): Promise<CapabilityCheckResponse> {
    const params = new URLSearchParams({
      holder_id: holderId,
      jurisdiction_id: jurisdictionId,
      capability,
    });
    return this.get<CapabilityCheckResponse>(`/membership/capability/check?${params}`);
  }

  /**
   * Add a role to a member (admin)
   */
  async addMemberRole(
    holderId: string,
    jurisdictionId: string,
    role: string
  ): Promise<MembershipActionResponse> {
    return this.post<MembershipActionResponse>('/membership/role/add', {
      holder_id: holderId,
      jurisdiction_id: jurisdictionId,
      role,
    });
  }

  /**
   * Remove a role from a member (admin)
   */
  async removeMemberRole(
    holderId: string,
    jurisdictionId: string,
    role: string
  ): Promise<MembershipActionResponse> {
    return this.post<MembershipActionResponse>('/membership/role/remove', {
      holder_id: holderId,
      jurisdiction_id: jurisdictionId,
      role,
    });
  }

  // ===========================================================================
  // Amendments (Commons Evolution)
  // ===========================================================================

  /**
   * Create a new amendment
   */
  async createAmendment(req: CreateAmendmentRequest): Promise<Amendment> {
    return this.post<Amendment>('/constitutional/amendments', req);
  }

  /**
   * Get amendment by ID
   */
  async getAmendment(amendmentId: string): Promise<Amendment> {
    return this.get<Amendment>(`/constitutional/amendments/${amendmentId}`);
  }

  /**
   * List amendments
   */
  async listAmendments(
    status?: string,
    scope?: string,
    type?: string
  ): Promise<AmendmentListResponse> {
    const params = new URLSearchParams();
    if (status) params.set('status', status);
    if (scope) params.set('scope', scope);
    if (type) params.set('type', type);
    const query = params.toString();
    return this.get<AmendmentListResponse>(
      `/constitutional/amendments${query ? `?${query}` : ''}`
    );
  }

  /**
   * Add a change to a draft amendment
   */
  async addAmendmentChange(
    amendmentId: string,
    change: AddAmendmentChangeRequest
  ): Promise<Amendment> {
    return this.post<Amendment>(`/constitutional/amendments/${amendmentId}/changes`, change);
  }

  /**
   * Submit amendment for review
   */
  async submitAmendment(amendmentId: string): Promise<Amendment> {
    return this.post<Amendment>(`/constitutional/amendments/${amendmentId}/submit`);
  }

  /**
   * Open voting on an amendment
   */
  async openAmendmentVoting(amendmentId: string): Promise<Amendment> {
    return this.post<Amendment>(`/constitutional/amendments/${amendmentId}/vote`);
  }

  /**
   * Ratify an amendment
   */
  async ratifyAmendment(
    amendmentId: string,
    req: RatifyAmendmentRequest
  ): Promise<Amendment> {
    return this.post<Amendment>(`/constitutional/amendments/${amendmentId}/ratify`, req);
  }

  /**
   * Withdraw an amendment
   */
  async withdrawAmendment(amendmentId: string, reason?: string): Promise<Amendment> {
    return this.post<Amendment>(`/constitutional/amendments/${amendmentId}/withdraw`, { reason });
  }

  // ===========================================================================
  // Amendment Voting (UI-friendly)
  // ===========================================================================

  /**
   * Cast a vote on an amendment
   * @param amendmentId - The amendment ID (hex string)
   * @param req - Vote request with choice and optional comment
   */
  async castAmendmentVote(
    amendmentId: string,
    req: CastAmendmentVoteRequest
  ): Promise<CastAmendmentVoteResponse> {
    return this.post<CastAmendmentVoteResponse>(
      `/constitutional/amendments/${amendmentId}/votes`,
      req
    );
  }

  /**
   * List all votes on an amendment
   * @param amendmentId - The amendment ID (hex string)
   */
  async listAmendmentVotes(amendmentId: string): Promise<ListAmendmentVotesResponse> {
    return this.get<ListAmendmentVotesResponse>(
      `/constitutional/amendments/${amendmentId}/votes`
    );
  }

  /**
   * Get voting results for an amendment
   * @param amendmentId - The amendment ID (hex string)
   * @param includeVotes - Whether to include individual votes in response
   */
  async getAmendmentResults(
    amendmentId: string,
    includeVotes?: boolean
  ): Promise<AmendmentVoteResults> {
    const query = includeVotes ? '?include_votes=true' : '';
    return this.get<AmendmentVoteResults>(
      `/constitutional/amendments/${amendmentId}/results${query}`
    );
  }

  /**
   * Get the current user's vote on an amendment
   * @param amendmentId - The amendment ID (hex string)
   */
  async getMyAmendmentVote(amendmentId: string): Promise<MyAmendmentVoteResponse> {
    return this.get<MyAmendmentVoteResponse>(
      `/constitutional/amendments/${amendmentId}/my-vote`
    );
  }

  // ===========================================================================
  // Appeals (Commons Evolution)
  // ===========================================================================

  /**
   * File an appeal
   */
  async fileAppeal(req: FileAppealRequest): Promise<Appeal> {
    return this.post<Appeal>('/constitutional/appeals', req);
  }

  /**
   * Get appeal by ID
   */
  async getAppeal(appealId: string): Promise<Appeal> {
    return this.get<Appeal>(`/constitutional/appeals/${appealId}`);
  }

  /**
   * List appeals
   */
  async listAppeals(
    status?: string,
    scope?: string,
    appellant?: string
  ): Promise<AppealListResponse> {
    const params = new URLSearchParams();
    if (status) params.set('status', status);
    if (scope) params.set('scope', scope);
    if (appellant) params.set('appellant', appellant);
    const query = params.toString();
    return this.get<AppealListResponse>(
      `/constitutional/appeals${query ? `?${query}` : ''}`
    );
  }

  /**
   * Add evidence to an appeal
   */
  async addAppealEvidence(
    appealId: string,
    evidence: AddEvidenceRequest
  ): Promise<Appeal> {
    return this.post<Appeal>(`/constitutional/appeals/${appealId}/evidence`, evidence);
  }

  /**
   * Add response to an appeal
   */
  async addAppealResponse(
    appealId: string,
    response: AddResponseRequest
  ): Promise<Appeal> {
    return this.post<Appeal>(`/constitutional/appeals/${appealId}/respond`, response);
  }

  /**
   * Begin review of an appeal (admin)
   */
  async beginAppealReview(appealId: string): Promise<Appeal> {
    return this.post<Appeal>(`/constitutional/appeals/${appealId}/review`);
  }

  /**
   * Resolve an appeal (admin)
   */
  async resolveAppeal(
    appealId: string,
    resolution: ResolveAppealRequest
  ): Promise<Appeal> {
    return this.post<Appeal>(`/constitutional/appeals/${appealId}/resolve`, resolution);
  }

  /**
   * Withdraw an appeal
   */
  async withdrawAppeal(appealId: string, reason?: string): Promise<Appeal> {
    return this.post<Appeal>(`/constitutional/appeals/${appealId}/withdraw`, { reason });
  }

  // ===========================================================================
  // Identity Resolution
  // ===========================================================================

  /**
   * Resolve a DID and check if it's valid
   *
   * This is a PUBLIC endpoint that allows verification of DIDs.
   *
   * @param did - The DID to resolve
   * @param includeAttestations - Whether to include attestation details
   * @returns DID resolution response with validity and attestation info
   *
   * @example
   * ```typescript
   * const result = await client.resolveDid('did:icn:abc123');
   * if (result.valid) {
   *   console.log('DID is valid, belongs to coop:', result.coop_id);
   * }
   * ```
   */
  async resolveDid(did: string, includeAttestations?: boolean): Promise<DidResolutionResponse> {
    const params = new URLSearchParams();
    if (includeAttestations) {
      params.set('include_attestations', 'true');
    }
    const query = params.toString();
    return this.get<DidResolutionResponse>(
      `/identity/resolve/${encodeURIComponent(did)}${query ? `?${query}` : ''}`,
      false
    );
  }

  /**
   * Check identity service health
   *
   * @returns Health status of the identity service
   */
  async identityHealth(): Promise<IdentityHealthResponse> {
    return this.get<IdentityHealthResponse>('/identity/health', false);
  }

  // ===========================================================================
  // Device Management
  // ===========================================================================

  /**
   * Register a new device for a DID
   *
   * Adds a new device to the authenticated user's DID document.
   * The request must be signed by an existing device with AddDevice capability.
   *
   * @param did - The DID to register the device for (must match authenticated user)
   * @param req - Device registration request
   * @returns The registered device info
   *
   * @example
   * ```typescript
   * const result = await client.registerDevice('did:icn:alice', {
   *   device_id: 'phone-2',
   *   label: 'My iPhone',
   *   public_key: '...hex-encoded Ed25519 public key...',
   *   capabilities: ['sign', 'encrypt'],
   *   signing_device_id: 'device-1',
   *   signature: '...hex-encoded signature...',
   * });
   * console.log('Registered device:', result.device.id);
   * ```
   */
  async registerDevice(did: string, req: RegisterDeviceRequest): Promise<RegisterDeviceResponse> {
    return this.post<RegisterDeviceResponse>(`/devices/${encodeURIComponent(did)}`, req);
  }

  /**
   * List all devices for a DID
   *
   * Returns all devices registered for the authenticated user's DID.
   *
   * @param did - The DID to list devices for (must match authenticated user)
   * @returns List of devices with total count
   *
   * @example
   * ```typescript
   * const result = await client.listDevices('did:icn:alice');
   * console.log(`${result.total} devices registered`);
   * for (const device of result.devices) {
   *   console.log(`- ${device.id}: ${device.label} (revoked: ${device.revoked})`);
   * }
   * ```
   */
  async listDevices(did: string): Promise<ListDevicesResponse> {
    return this.get<ListDevicesResponse>(`/devices/${encodeURIComponent(did)}`);
  }

  /**
   * Get a specific device by ID
   *
   * @param did - The DID the device belongs to (must match authenticated user)
   * @param deviceId - The device ID to retrieve
   * @returns Device information
   *
   * @example
   * ```typescript
   * const device = await client.getDevice('did:icn:alice', 'phone-1');
   * console.log(`Device ${device.id} has capabilities:`, device.capabilities);
   * ```
   */
  async getDevice(did: string, deviceId: string): Promise<DeviceInfo> {
    return this.get<DeviceInfo>(`/devices/${encodeURIComponent(did)}/${encodeURIComponent(deviceId)}`);
  }

  /**
   * Revoke a device
   *
   * Revokes a device from the authenticated user's DID document.
   * The request must be signed by a device with RevokeDevice capability.
   * Cannot revoke the last device with management capabilities.
   *
   * @param did - The DID the device belongs to (must match authenticated user)
   * @param deviceId - The device ID to revoke
   * @param req - Revocation request with signing device and signature
   * @returns Revocation response
   *
   * @example
   * ```typescript
   * const result = await client.revokeDevice('did:icn:alice', 'phone-old', {
   *   signing_device_id: 'device-1',
   *   signature: '...hex-encoded signature...',
   * });
   * console.log('Revoked device:', result.device_id);
   * ```
   */
  async revokeDevice(
    did: string,
    deviceId: string,
    req: RevokeDeviceRequest
  ): Promise<RevokeDeviceResponse> {
    return this.delete<RevokeDeviceResponse>(
      `/devices/${encodeURIComponent(did)}/${encodeURIComponent(deviceId)}`,
      req
    );
  }

  // ===========================================================================
  // Batch Operations
  // ===========================================================================

  /**
   * Execute multiple payments in a batch
   *
   * @example
   * ```typescript
   * const results = await client.batchPay('food-coop', [
   *   { from: 'did:icn:alice', to: 'did:icn:bob', amount: 2.5, currency: 'hours', memo: 'Garden help' },
   *   { from: 'did:icn:alice', to: 'did:icn:carol', amount: 1.0, currency: 'hours', memo: 'Bookkeeping' },
   * ]);
   * console.log(`${results.succeeded} succeeded, ${results.failed} failed`);
   * ```
   */
  async batchPay(coopId: string, payments: PaymentRequest[]): Promise<{
    succeeded: number;
    failed: number;
    results: Array<{ success: boolean; payment?: PaymentResponse; error?: string }>;
  }> {
    const results = await Promise.allSettled(
      payments.map(payment => this.pay(coopId, payment))
    );

    return {
      succeeded: results.filter(r => r.status === 'fulfilled').length,
      failed: results.filter(r => r.status === 'rejected').length,
      results: results.map(r => 
        r.status === 'fulfilled' 
          ? { success: true, payment: r.value }
          : { success: false, error: (r.reason as Error).message }
      ),
    };
  }

  /**
   * Add multiple members to a cooperative in a batch
   *
   * @example
   * ```typescript
   * const results = await client.batchAddMembers('food-coop', [
   *   { did: 'did:icn:bob', role: 'member' },
   *   { did: 'did:icn:carol', role: 'member' },
   * ]);
   * ```
   */
  async batchAddMembers(coopId: string, members: AddMemberRequest[]): Promise<{
    succeeded: number;
    failed: number;
    results: Array<{ success: boolean; member?: Member; error?: string }>;
  }> {
    const results = await Promise.allSettled(
      members.map(member => this.addMember(coopId, member))
    );

    return {
      succeeded: results.filter(r => r.status === 'fulfilled').length,
      failed: results.filter(r => r.status === 'rejected').length,
      results: results.map(r => 
        r.status === 'fulfilled' 
          ? { success: true, member: r.value }
          : { success: false, error: (r.reason as Error).message }
      ),
    };
  }

  /**
   * Update multiple members in a batch
   *
   * @example
   * ```typescript
   * const results = await client.batchUpdateMembers('food-coop', [
   *   { did: 'did:icn:bob', updates: { role: 'admin' } },
   *   { did: 'did:icn:carol', updates: { role: 'admin' } },
   * ]);
   * ```
   */
  async batchUpdateMembers(coopId: string, updates: Array<{ did: string; updates: UpdateMemberRequest }>): Promise<{
    succeeded: number;
    failed: number;
    results: Array<{ success: boolean; member?: Member; error?: string }>;
  }> {
    const results = await Promise.allSettled(
      updates.map(({ did, updates }) => this.updateMember(coopId, did, updates))
    );

    return {
      succeeded: results.filter(r => r.status === 'fulfilled').length,
      failed: results.filter(r => r.status === 'rejected').length,
      results: results.map(r => 
        r.status === 'fulfilled' 
          ? { success: true, member: r.value }
          : { success: false, error: (r.reason as Error).message }
      ),
    };
  }

  // ===========================================================================
  // Exchange Rate Oracle
  // ===========================================================================

  /**
   * Get exchange rate between two currencies
   *
   * @example
   * ```typescript
   * const rate = await client.getExchangeRate('hours', 'USD');
   * console.log(`1 hour = ${rate.rate} USD`);
   * ```
   */
  async getExchangeRate(
    fromCurrency: string,
    toCurrency: string
  ): Promise<ExchangeRateResponse> {
    return this.get<ExchangeRateResponse>(
      `/v1/oracle/rate/${encodeURIComponent(fromCurrency)}/${encodeURIComponent(toCurrency)}`
    );
  }

  /**
   * Convert an amount between currencies
   *
   * @example
   * ```typescript
   * const result = await client.convertAmount({
   *   amount: 10,
   *   from_currency: 'hours',
   *   to_currency: 'USD',
   * });
   * console.log(`10 hours = ${result.converted_amount} USD`);
   * ```
   */
  async convertAmount(
    request: ConvertAmountRequest
  ): Promise<ConvertAmountResponse> {
    return this.post<ConvertAmountResponse>('/v1/oracle/convert', request);
  }

  /**
   * List available rate sources
   *
   * @example
   * ```typescript
   * const { sources } = await client.listRateSources();
   * for (const source of sources) {
   *   console.log(`${source.name} (priority ${source.priority})`);
   * }
   * ```
   */
  async listRateSources(): Promise<ListRateSourcesResponse> {
    return this.get<ListRateSourcesResponse>('/v1/oracle/sources');
  }

  /**
   * Set a manual exchange rate (requires oracle:write scope)
   *
   * @example
   * ```typescript
   * const result = await client.setManualRate({
   *   from_currency: 'hours',
   *   to_currency: 'USD',
   *   rate: 25.0,
   *   note: 'Q1 2025 rate adjustment',
   * });
   * ```
   */
  async setManualRate(
    request: SetManualRateRequest
  ): Promise<SetManualRateResponse> {
    return this.post<SetManualRateResponse>('/v1/oracle/rate', request);
  }

  // ===========================================================================
  // Query Builders
  // ===========================================================================

  /**
   * Create a query builder for transaction history
   *
   * @example
   * ```typescript
   * const history = await client.queryHistory('food-coop')
   *   .fromDid('did:icn:alice')
   *   .limit(50)
   *   .execute();
   * ```
   */
  queryHistory(coopId: string): HistoryQueryBuilder {
    return new HistoryQueryBuilder(this, coopId);
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

    ws.onopen = () => {
      // Send auth message
      if (this.token) {
        ws.send(JSON.stringify({ type: 'Auth', token: this.token }));
      }
      handlers.onOpen?.();
    };

    ws.onmessage = (event: MessageEvent) => {
      try {
        const data = typeof event.data === 'string' ? event.data : event.data.toString();
        const message = JSON.parse(data) as WsMessage;
        handlers.onMessage?.(message);
      } catch (error) {
        handlers.onError?.(error as Error);
      }
    };

    ws.onerror = (event: Event) => {
      handlers.onError?.(new Error('WebSocket error'));
    };

    ws.onclose = () => {
      handlers.onClose?.();
    };

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
  autoBackfill: true,
  gapDetection: true,
};

/**
 * Subscription state for tracking connection status
 */
export type SubscriptionState = 'connecting' | 'connected' | 'reconnecting' | 'disconnected' | 'closed';

/**
 * Managed WebSocket subscription with auto-reconnect, backfill, and gap detection
 *
 * Features:
 * - Automatic reconnection with exponential backoff
 * - Sequence number tracking for reliable event delivery
 * - Automatic backfill request after reconnection
 * - Gap detection to catch missed events
 * - Graceful shutdown handling with server-suggested reconnect delay
 */
export class ICNSubscription {
  private client: ICNClient;
  private coopId: string;
  private handlers: {
    onEvent?: (event: WsMessage) => void;
    onAuthOk?: (did: string, currentSeq: number) => void;
    onError?: (error: Error) => void;
    onReconnect?: (attempt: number) => void;
    onDisconnect?: () => void;
    onShutdown?: (reason: string, reconnectAfterMs: number | null) => void;
    onBackfillComplete?: (count: number) => void;
    onStateChange?: (state: SubscriptionState) => void;
  };
  private options: Required<WebSocketOptions>;
  private ws?: WebSocket;
  private reconnectAttempt = 0;
  private closed = false;
  private reconnectTimer?: ReturnType<typeof setTimeout>;

  // Sequence tracking for reliable delivery
  private lastSeq: number | null = null;
  private expectedSeq: number | null = null;
  private serverSuggestedDelay: number | null = null;
  private _state: SubscriptionState = 'connecting';

  constructor(
    client: ICNClient,
    coopId: string,
    handlers: {
      onEvent?: (event: WsMessage) => void;
      onAuthOk?: (did: string, currentSeq: number) => void;
      onError?: (error: Error) => void;
      onReconnect?: (attempt: number) => void;
      onDisconnect?: () => void;
      onShutdown?: (reason: string, reconnectAfterMs: number | null) => void;
      onBackfillComplete?: (count: number) => void;
      onStateChange?: (state: SubscriptionState) => void;
    },
    options?: WebSocketOptions
  ) {
    this.client = client;
    this.coopId = coopId;
    this.handlers = handlers;
    this.options = { ...DEFAULT_WS_OPTIONS, ...options };
    this.connect();
  }

  /**
   * Get current subscription state
   */
  get state(): SubscriptionState {
    return this._state;
  }

  /**
   * Get the last received sequence number (useful for persistence)
   */
  getLastSeq(): number | null {
    return this.lastSeq;
  }

  /**
   * Set the last sequence number (for resuming after app restart)
   */
  setLastSeq(seq: number): void {
    this.lastSeq = seq;
    this.expectedSeq = seq + 1;
  }

  private setState(state: SubscriptionState): void {
    if (this._state !== state) {
      this._state = state;
      this.handlers.onStateChange?.(state);
    }
  }

  private connect(): void {
    if (this.closed) return;

    this.setState('connecting');

    this.ws = this.client.connectWebSocket(this.coopId, {
      onOpen: () => {
        // Reset reconnect counter on successful connection
        this.reconnectAttempt = 0;
        this.serverSuggestedDelay = null;
      },
      onMessage: (message) => {
        this.handleMessage(message);
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

  private handleMessage(message: WsMessage): void {
    switch (message.type) {
      case 'AuthOk': {
        this.setState('connected');
        const currentSeq = (message as WsAuthOkMessage).current_seq;
        this.handlers.onAuthOk?.((message as WsAuthOkMessage).did, currentSeq);

        // Request backfill if we have a last sequence and auto-backfill is enabled
        if (this.options.autoBackfill && this.lastSeq !== null && this.lastSeq < currentSeq) {
          this.requestBackfill(this.lastSeq);
        } else if (this.expectedSeq === null) {
          // First connection - set expected to current + 1
          this.expectedSeq = currentSeq + 1;
        }
        break;
      }

      case 'Event': {
        const eventMsg = message as WsEventMessage;
        const seq = eventMsg.seq;

        // Gap detection
        if (this.options.gapDetection && this.expectedSeq !== null && seq > this.expectedSeq) {
          // Gap detected - request backfill for missed events
          this.requestBackfill(this.expectedSeq - 1);
        }

        // Update sequence tracking
        this.lastSeq = seq;
        this.expectedSeq = seq + 1;

        // Forward event to handler
        this.handlers.onEvent?.(message);
        break;
      }

      case 'BackfillComplete': {
        const count = (message as WsBackfillCompleteMessage).count;
        this.handlers.onBackfillComplete?.(count);
        break;
      }

      case 'Shutdown': {
        const shutdownMsg = message as WsShutdownMessage;
        this.handlers.onShutdown?.(shutdownMsg.reason, shutdownMsg.reconnect_after_ms);

        // Use server-suggested delay if provided
        if (shutdownMsg.reconnect_after_ms !== null) {
          this.serverSuggestedDelay = shutdownMsg.reconnect_after_ms;
        }
        break;
      }

      case 'Error': {
        this.handlers.onError?.(new Error((message as WsErrorMessage).message));
        break;
      }

      default:
        // Forward other messages (Ping, Pong, etc.)
        this.handlers.onEvent?.(message);
    }
  }

  /**
   * Request backfill of events after a given sequence number
   */
  requestBackfill(afterSeq: number): void {
    if (this.ws?.readyState === WebSocket.OPEN) {
      const backfillMsg: WsBackfillMessage = {
        type: 'Backfill',
        after_seq: afterSeq,
      };
      this.ws.send(JSON.stringify(backfillMsg));
    }
  }

  private scheduleReconnect(): void {
    if (this.closed || !this.options.autoReconnect) {
      this.setState('disconnected');
      return;
    }

    if (this.reconnectAttempt >= this.options.maxReconnectAttempts) {
      this.handlers.onError?.(new Error('Max reconnection attempts reached'));
      this.setState('disconnected');
      return;
    }

    this.setState('reconnecting');
    this.reconnectAttempt++;
    this.handlers.onReconnect?.(this.reconnectAttempt);

    // Use server-suggested delay if available, otherwise calculate with exponential backoff
    let delay: number;
    if (this.serverSuggestedDelay !== null) {
      delay = this.serverSuggestedDelay;
      this.serverSuggestedDelay = null; // Use only once
    } else {
      delay = Math.min(
        this.options.reconnectDelayMs * Math.pow(2, this.reconnectAttempt - 1),
        this.options.maxReconnectDelayMs
      );
    }

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
    this.setState('closed');
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

  /**
   * Manually trigger reconnection (resets attempt counter)
   */
  reconnect(): void {
    if (this.closed) return;
    this.reconnectAttempt = 0;
    if (this.ws) {
      this.ws.close();
    } else {
      this.connect();
    }
  }
}

/**
 * Event filter helpers for WebSocket subscriptions
 */
export class EventFilter {
  /**
   * Filter events by type
   */
  static byType(eventType: string): (message: WsMessage) => boolean {
    return (message) => {
      return message.type === 'Event' && (message as any).event_type === eventType;
    };
  }

  /**
   * Filter events by DID (sender or participant)
   */
  static byDid(did: string): (message: WsMessage) => boolean {
    return (message) => {
      if (message.type !== 'Event') return false;
      const payload = (message as any).payload;
      return payload?.from === did || payload?.to === did || 
             payload?.sender === did || payload?.did === did;
    };
  }

  /**
   * Filter payment events
   */
  static payments(): (message: WsMessage) => boolean {
    return EventFilter.byType('Payment');
  }

  /**
   * Filter proposal events
   */
  static proposals(): (message: WsMessage) => boolean {
    return (message) => {
      if (message.type !== 'Event') return false;
      const eventType = (message as any).event_type;
      return eventType === 'ProposalCreated' || 
             eventType === 'ProposalOpened' || 
             eventType === 'ProposalClosed' ||
             eventType === 'VoteCast';
    };
  }

  /**
   * Filter member events
   */
  static members(): (message: WsMessage) => boolean {
    return (message) => {
      if (message.type !== 'Event') return false;
      const eventType = (message as any).event_type;
      return eventType === 'MemberAdded' || 
             eventType === 'MemberRemoved' || 
             eventType === 'MemberUpdated';
    };
  }

  /**
   * Combine multiple filters with AND logic
   */
  static and(...filters: Array<(message: WsMessage) => boolean>): (message: WsMessage) => boolean {
    return (message) => filters.every(f => f(message));
  }

  /**
   * Combine multiple filters with OR logic
   */
  static or(...filters: Array<(message: WsMessage) => boolean>): (message: WsMessage) => boolean {
    return (message) => filters.some(f => f(message));
  }
}

/**
 * Query builder for transaction history with fluent API
 */
export class HistoryQueryBuilder {
  private client: ICNClient;
  private coopId: string;
  private filters: {
    from_did?: string;
    to_did?: string;
    offset?: number;
    limit?: number;
    min_amount?: number;
    max_amount?: number;
    start_date?: string;
    end_date?: string;
  } = {};

  constructor(client: ICNClient, coopId: string) {
    this.client = client;
    this.coopId = coopId;
  }

  /**
   * Filter by sender DID
   */
  fromDid(did: string): this {
    this.filters.from_did = did;
    return this;
  }

  /**
   * Filter by recipient DID
   */
  toDid(did: string): this {
    this.filters.to_did = did;
    return this;
  }

  /**
   * Set pagination offset
   */
  offset(offset: number): this {
    this.filters.offset = offset;
    return this;
  }

  /**
   * Set result limit
   */
  limit(limit: number): this {
    this.filters.limit = limit;
    return this;
  }

  /**
   * Filter by minimum amount
   */
  minAmount(amount: number): this {
    this.filters.min_amount = amount;
    return this;
  }

  /**
   * Filter by maximum amount
   */
  maxAmount(amount: number): this {
    this.filters.max_amount = amount;
    return this;
  }

  /**
   * Filter by start date (ISO 8601 format)
   */
  startDate(date: string): this {
    this.filters.start_date = date;
    return this;
  }

  /**
   * Filter by end date (ISO 8601 format)
   */
  endDate(date: string): this {
    this.filters.end_date = date;
    return this;
  }

  /**
   * Filter transactions for the last N days
   */
  lastDays(days: number): this {
    const end = new Date();
    const start = new Date();
    start.setDate(start.getDate() - days);
    this.filters.start_date = start.toISOString();
    this.filters.end_date = end.toISOString();
    return this;
  }

  /**
   * Execute the query and return transaction history
   */
  async execute(): Promise<TransactionHistory> {
    return this.client.getHistory(this.coopId, this.filters);
  }
}

/**
 * Create an ICN client
 */
export function createClient(options: ICNClientOptions): ICNClient {
  return new ICNClient(options);
}
