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
} from './types';

export * from './types';

/**
 * ICN Gateway API Client
 */
export class ICNClient {
  private baseUrl: string;
  private token?: string;
  private timeout: number;
  private fetchImpl: typeof fetch;

  constructor(options: ICNClientOptions) {
    this.baseUrl = options.baseUrl.replace(/\/$/, '');
    this.token = options.token;
    this.timeout = options.timeout ?? 30000;
    this.fetchImpl = options.fetch ?? globalThis.fetch;
  }

  /**
   * Set the JWT token for authenticated requests
   */
  setToken(token: string): void {
    this.token = token;
  }

  /**
   * Clear the JWT token
   */
  clearToken(): void {
    this.token = undefined;
  }

  /**
   * Check if client has a token set
   */
  hasToken(): boolean {
    return !!this.token;
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
    const url = `${this.baseUrl}/v1${path}`;
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
    this.setToken(auth.token);
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
  // WebSocket
  // ===========================================================================

  /**
   * Connect to WebSocket for real-time events
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
}

/**
 * Create an ICN client
 */
export function createClient(options: ICNClientOptions): ICNClient {
  return new ICNClient(options);
}
