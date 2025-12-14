/**
 * ICN Mobile Client
 *
 * React Native wrapper around the core ICN client with mobile-specific features.
 */

import { ICNClient, ICNClientOptions, SignatureProvider } from '@icn/client';
import {
  ICNMobileClientOptions,
  ICNWallet,
  AuthState,
  SecureStorage,
  WebSocketState,
  EventListener,
  Unsubscribe,
  WsMessage,
  NetworkState,
  QueuedOperation,
} from './types';
import {
  GenerateProofRequest,
  EphemeralProof,
  VerifyResult,
  SdisHealth,
  ProofType,
} from './sdis-types';
import { QueueManager } from './queue-manager';
import { parseError, createError, isNetworkError } from './error-utils';

// SecureStore keys must be alphanumeric with periods, underscores, or hyphens only (no @ or slashes)
const TOKEN_KEY = 'icn_auth_token';
const DID_KEY = 'icn_auth_did';
const COOP_KEY = 'icn_coop_id';
const EXPIRES_KEY = 'icn_expires_at';

/**
 * Mobile-optimized ICN client with persistent auth and wallet integration
 */
export class ICNMobileClient extends ICNClient {
  private wallet?: ICNWallet;
  private storage?: SecureStorage;
  private queueManager: QueueManager;
  private _authState: AuthState = {
    isAuthenticated: false,
    did: null,
    coopId: null,
    expiresAt: null,
  };
  private _networkState: NetworkState = 'online';
  private authListeners: Set<EventListener<AuthState>> = new Set();
  private networkListeners: Set<EventListener<NetworkState>> = new Set();
  private wsSocket: WebSocket | null = null;
  private wsState: WebSocketState = 'disconnected';
  private wsListeners: Map<string, Set<EventListener<WsMessage>>> = new Map();
  private wsStateListeners: Set<EventListener<WebSocketState>> = new Set();
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 5;
  private reconnectDelay = 1000;
  private intentionalDisconnect = false;
  private reconnectTimeoutId: ReturnType<typeof setTimeout> | null = null;

  constructor(options: ICNMobileClientOptions) {
    // Create base client options
    const baseOptions: ICNClientOptions = {
      baseUrl: options.baseUrl,
      timeout: options.timeout,
    };

    super(baseOptions);

    this.wallet = options.wallet;
    this.storage = options.storage;
    this.queueManager = new QueueManager(options.storage);
    
    // Setup network state monitoring if available
    this.setupNetworkMonitoring();
  }

  /**
   * Get current authentication state
   */
  get authState(): AuthState {
    return { ...this._authState };
  }

  /**
   * Get current WebSocket connection state
   */
  get connectionState(): WebSocketState {
    return this.wsState;
  }

  /**
   * Get current network state
   */
  get networkState(): NetworkState {
    return this._networkState;
  }

  /**
   * Get operation queue
   */
  get queue(): QueuedOperation[] {
    return this.queueManager.getQueue();
  }

  /**
   * Get count of pending operations
   */
  get pendingOperations(): number {
    return this.queueManager.getPendingCount();
  }

  /**
   * Initialize the client by loading persisted auth state
   */
  async initialize(): Promise<void> {
    if (!this.storage) return;

    try {
      const [token, did, coopId, expiresStr] = await Promise.all([
        this.storage.getItem(TOKEN_KEY),
        this.storage.getItem(DID_KEY),
        this.storage.getItem(COOP_KEY),
        this.storage.getItem(EXPIRES_KEY),
      ]);

      if (token && did) {
        const expiresAt = expiresStr ? parseInt(expiresStr, 10) : null;

        // Check if token is expired
        if (expiresAt && Date.now() > expiresAt) {
          await this.clearAuth();
          return;
        }

        this.setToken(token);
        this.updateAuthState({
          isAuthenticated: true,
          did,
          coopId,
          expiresAt,
        });
      }

      // Initialize queue manager
      await this.queueManager.initialize();
    } catch (error) {
      console.warn('Failed to load persisted auth state:', error);
    }
  }

  /**
   * Setup network state monitoring
   */
  private setupNetworkMonitoring(): void {
    // Try to access @react-native-community/netinfo if available
    try {
      // @ts-ignore - dynamic import
      const NetInfo = require('@react-native-community/netinfo');
      
      NetInfo.addEventListener((state: any) => {
        const newState: NetworkState = state.isConnected
          ? state.isInternetReachable === false
            ? 'slow'
            : 'online'
          : 'offline';

        if (newState !== this._networkState) {
          this._networkState = newState;
          this.notifyNetworkListeners();

          // Process queue when coming back online
          if (newState === 'online') {
            this.processQueue();
          }
        }
      });
    } catch (error) {
      // NetInfo not available - use basic fetch-based detection
      this.startBasicNetworkMonitoring();
    }
  }

  /**
   * Basic network monitoring using periodic checks
   */
  private startBasicNetworkMonitoring(): void {
    setInterval(async () => {
      try {
        const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
        const controller = new AbortController();
        const timeout = setTimeout(() => controller.abort(), 5000);

        await fetch(`${baseUrl}/v1/health`, {
          signal: controller.signal,
        });
        
        clearTimeout(timeout);

        if (this._networkState !== 'online') {
          this._networkState = 'online';
          this.notifyNetworkListeners();
          this.processQueue();
        }
      } catch {
        if (this._networkState !== 'offline') {
          this._networkState = 'offline';
          this.notifyNetworkListeners();
        }
      }
    }, 30000); // Check every 30 seconds
  }

  /**
   * Process queued operations
   */
  async processQueue(): Promise<void> {
    if (this._networkState === 'offline') return;

    await this.queueManager.processQueue(async (op) => {
      switch (op.type) {
        case 'payment':
          // Re-execute payment
          const paymentData = op.data as any;
          await this.pay(paymentData.coopId, paymentData.request);
          break;
        case 'vote':
          // Re-execute vote
          const voteData = op.data as any;
          await this.vote(voteData.proposalId, voteData.request);
          break;
        // Add other operation types as needed
        default:
          throw new Error(`Unknown operation type: ${op.type}`);
      }
    });
  }

  /**
   * Authenticate using the configured wallet
   */
  async login(coopId?: string, scopes?: string[]): Promise<AuthState> {
    if (!this.wallet) {
      throw new Error('No wallet configured. Set wallet in options or use loginWithSignature.');
    }

    const keyPair = await this.wallet.getKeyPair();
    if (!keyPair) {
      throw new Error('No key pair in wallet. Generate or import a key pair first.');
    }

    // Create signature provider from wallet
    const signer: SignatureProvider = {
      sign: (message: string) => this.wallet!.sign(message),
    };

    // Authenticate
    const result = await this.authenticate(keyPair.did, signer, coopId, scopes);

    // Use server's expiration time
    const expiresAt = result.expires_at;

    // Persist auth state
    await this.persistAuth(result.token, keyPair.did, coopId || null, expiresAt);

    // Update state
    this.updateAuthState({
      isAuthenticated: true,
      did: keyPair.did,
      coopId: coopId || null,
      expiresAt,
    });

    return this.authState;
  }

  /**
   * Authenticate with a custom signature provider
   */
  async loginWithSignature(
    did: string,
    signer: SignatureProvider,
    coopId?: string,
    scopes?: string[]
  ): Promise<AuthState> {
    const result = await this.authenticate(did, signer, coopId, scopes);

    // Use server's expiration time
    const expiresAt = result.expires_at;
    await this.persistAuth(result.token, did, coopId || null, expiresAt);

    this.updateAuthState({
      isAuthenticated: true,
      did,
      coopId: coopId || null,
      expiresAt,
    });

    return this.authState;
  }

  /**
   * Log out and clear persisted auth
   */
  async logout(): Promise<void> {
    this.clearToken();
    await this.clearAuth();
    this.disconnectWebSocket();

    this.updateAuthState({
      isAuthenticated: false,
      did: null,
      coopId: null,
      expiresAt: null,
    });
  }

  /**
   * Subscribe to auth state changes
   */
  onAuthStateChange(listener: EventListener<AuthState>): Unsubscribe {
    this.authListeners.add(listener);
    return () => this.authListeners.delete(listener);
  }

  /**
   * Subscribe to network state changes
   */
  onNetworkStateChange(listener: EventListener<NetworkState>): Unsubscribe {
    this.networkListeners.add(listener);
    return () => this.networkListeners.delete(listener);
  }

  /**
   * Subscribe to queue changes
   */
  onQueueChange(listener: (queue: QueuedOperation[]) => void): Unsubscribe {
    return this.queueManager.onChange(listener);
  }

  /**
   * Manually queue an operation for later execution
   */
  async queueOperation(type: QueuedOperation['type'], data: unknown): Promise<string> {
    return this.queueManager.enqueue({ type, data });
  }

  /**
   * Clear failed operations from queue
   */
  async clearFailedOperations(): Promise<void> {
    await this.queueManager.clearFailed();
  }

  // ===========================================================================
  // WebSocket with auto-reconnect
  // ===========================================================================

  /**
   * Connect to WebSocket for real-time events
   */
  connectRealtime(coopId?: string): void {
    const targetCoopId = coopId || this._authState.coopId;
    if (!targetCoopId) {
      throw new Error('No coop ID provided and none in auth state');
    }

    this.intentionalDisconnect = false;
    this.disconnectWebSocket();
    this.createWebSocket(targetCoopId);
  }

  /**
   * Disconnect WebSocket
   */
  disconnectRealtime(): void {
    this.intentionalDisconnect = true;
    this.disconnectWebSocket();
  }

  /**
   * Subscribe to WebSocket events
   */
  onEvent(eventType: string, listener: EventListener<WsMessage>): Unsubscribe {
    if (!this.wsListeners.has(eventType)) {
      this.wsListeners.set(eventType, new Set());
    }
    this.wsListeners.get(eventType)!.add(listener);

    return () => {
      const listeners = this.wsListeners.get(eventType);
      if (listeners) {
        listeners.delete(listener);
      }
    };
  }

  /**
   * Subscribe to all events
   */
  onAnyEvent(listener: EventListener<WsMessage>): Unsubscribe {
    return this.onEvent('*', listener);
  }

  /**
   * Subscribe to connection state changes
   */
  onConnectionStateChange(listener: EventListener<WebSocketState>): Unsubscribe {
    this.wsStateListeners.add(listener);
    return () => this.wsStateListeners.delete(listener);
  }

  // ===========================================================================
  // Private methods
  // ===========================================================================

  private async persistAuth(
    token: string,
    did: string,
    coopId: string | null,
    expiresAt: number
  ): Promise<void> {
    if (!this.storage) return;

    await Promise.all([
      this.storage.setItem(TOKEN_KEY, token),
      this.storage.setItem(DID_KEY, did),
      coopId ? this.storage.setItem(COOP_KEY, coopId) : this.storage.removeItem(COOP_KEY),
      this.storage.setItem(EXPIRES_KEY, expiresAt.toString()),
    ]);
  }

  private async clearAuth(): Promise<void> {
    if (!this.storage) return;

    await Promise.all([
      this.storage.removeItem(TOKEN_KEY),
      this.storage.removeItem(DID_KEY),
      this.storage.removeItem(COOP_KEY),
      this.storage.removeItem(EXPIRES_KEY),
    ]);
  }

  private updateAuthState(state: AuthState): void {
    this._authState = state;
    this.authListeners.forEach((listener) => listener(state));
  }

  private notifyNetworkListeners(): void {
    this.networkListeners.forEach((listener) => listener(this._networkState));
  }

  private createWebSocket(coopId: string): void {
    const wsUrl = this.getWebSocketUrl(coopId);
    this.setWsState('connecting');

    try {
      // Use native WebSocket (available in React Native)
      this.wsSocket = new WebSocket(wsUrl);

      this.wsSocket.onopen = () => {
        this.reconnectAttempts = 0;
        // Authenticate the connection
        if (this.hasToken()) {
          this.wsSocket?.send(JSON.stringify({
            type: 'Auth',
            token: this.getToken(),
          }));
        }
        this.setWsState('connected');
      };

      this.wsSocket.onmessage = (event) => {
        try {
          const message = JSON.parse(event.data) as WsMessage;
          this.notifyEventListeners(message);
        } catch (error) {
          console.warn('Failed to parse WebSocket message:', error);
        }
      };

      this.wsSocket.onerror = () => {
        this.setWsState('error');
      };

      this.wsSocket.onclose = () => {
        this.setWsState('disconnected');
        // Only attempt reconnect if this wasn't an intentional disconnect
        if (!this.intentionalDisconnect) {
          this.attemptReconnect(coopId);
        }
      };
    } catch (error) {
      this.setWsState('error');
      throw error;
    }
  }

  private disconnectWebSocket(): void {
    // Clear any pending reconnect timeout
    if (this.reconnectTimeoutId !== null) {
      clearTimeout(this.reconnectTimeoutId);
      this.reconnectTimeoutId = null;
    }

    if (this.wsSocket) {
      // Clear event handlers before closing to prevent callbacks
      this.wsSocket.onopen = null;
      this.wsSocket.onmessage = null;
      this.wsSocket.onerror = null;
      this.wsSocket.onclose = null;
      this.wsSocket.close();
      this.wsSocket = null;
    }
    this.setWsState('disconnected');
    this.reconnectAttempts = 0;
  }

  private setWsState(state: WebSocketState): void {
    this.wsState = state;
    this.wsStateListeners.forEach((listener) => listener(state));
  }

  private attemptReconnect(coopId: string): void {
    if (this.reconnectAttempts >= this.maxReconnectAttempts) {
      console.warn('Max reconnect attempts reached');
      return;
    }

    this.reconnectAttempts++;
    const delay = this.reconnectDelay * Math.pow(2, this.reconnectAttempts - 1);

    this.reconnectTimeoutId = setTimeout(() => {
      this.reconnectTimeoutId = null;
      if (this.wsState === 'disconnected') {
        this.createWebSocket(coopId);
      }
    }, delay);
  }

  private notifyEventListeners(message: WsMessage): void {
    // For Event messages, extract the actual event type from the nested payload
    // WsEventMessage has { type: 'Event', event: { type: 'PaymentCreated', ... } }
    let eventType: string = message.type;
    if (message.type === 'Event' && 'event' in message && typeof message.event === 'object' && message.event && 'type' in message.event) {
      eventType = (message.event as { type: string }).type;
    }

    // Notify specific event listeners (e.g., 'PaymentCreated', 'MemberAdded')
    const listeners = this.wsListeners.get(eventType);
    if (listeners) {
      listeners.forEach((listener) => listener(message));
    }

    // Also notify listeners registered for the wrapper type (e.g., 'Event')
    if (eventType !== message.type) {
      const wrapperListeners = this.wsListeners.get(message.type);
      if (wrapperListeners) {
        wrapperListeners.forEach((listener) => listener(message));
      }
    }

    // Notify wildcard listeners
    const wildcardListeners = this.wsListeners.get('*');
    if (wildcardListeners) {
      wildcardListeners.forEach((listener) => listener(message));
    }
  }

  // ===========================================================================
  // SDIS Methods
  // ===========================================================================

  /**
   * Generate an ephemeral proof for SDIS verification
   *
   * @param request - Proof generation parameters
   * @returns Generated proof with QR data
   */
  async generateEphemeralProof(request: GenerateProofRequest): Promise<EphemeralProof> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/sdis/ephemeral/generate`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify({
        proof_type: this.formatProofTypeForApi(request.proof_type),
        validity_secs: request.validity_secs ?? 3600,
        channels: request.channels ?? [],
      }),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to generate proof: ${error}`);
    }

    return response.json();
  }

  /**
   * Verify a proof at Level 1 (QR scan only)
   *
   * Fast verification using only the QR code data.
   * No network required beyond this API call.
   *
   * @param qrData - Base64-encoded QR data
   * @returns Verification result
   */
  async verifyLevel1(qrData: string): Promise<VerifyResult> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/sdis/verify/level1`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({ qr_data: qrData }),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Verification failed: ${error}`);
    }

    return response.json();
  }

  /**
   * Verify a proof at Level 2 (with binding)
   *
   * Enhanced verification using both QR data and binding.
   * Binding can be provided or retrieved from server cache.
   *
   * @param qrData - Base64-encoded QR data
   * @param binding - Optional base64-encoded binding data
   * @returns Verification result
   */
  async verifyLevel2(qrData: string, binding?: string): Promise<VerifyResult> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/sdis/verify/level2`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        qr_data: qrData,
        binding: binding,
      }),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Verification failed: ${error}`);
    }

    return response.json();
  }

  /**
   * Check SDIS service health
   */
  async getSdisHealth(): Promise<SdisHealth> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/sdis/health`, {
      method: 'GET',
    });

    if (!response.ok) {
      throw new Error('SDIS service unavailable');
    }

    return response.json();
  }

  // ===========================================================================
  // Steward Methods
  // ===========================================================================

  /**
   * Get pending enrollments for steward review
   *
   * @param filter - Optional filters for coop_id or level
   * @returns Pending enrollments response
   */
  async getPendingEnrollments(filter?: {
    coop_id?: string;
    level?: number;
  }): Promise<import('./steward-types').PendingEnrollmentsResponse> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const params = new URLSearchParams();
    if (filter?.coop_id) params.set('coop_id', filter.coop_id);
    if (filter?.level !== undefined) params.set('level', filter.level.toString());

    const url = `${baseUrl}/v1/sdis/pending${params.toString() ? `?${params}` : ''}`;
    const response = await fetch(url, {
      method: 'GET',
      headers: {
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to get pending enrollments: ${error}`);
    }

    return response.json();
  }

  /**
   * Get enrollment status by ID
   *
   * @param enrollmentId - Enrollment ID to check
   * @returns Enrollment details
   */
  async getEnrollmentStatus(
    enrollmentId: string
  ): Promise<import('./steward-types').Enrollment> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/sdis/status/${enrollmentId}`, {
      method: 'GET',
      headers: {
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to get enrollment status: ${error}`);
    }

    return response.json();
  }

  /**
   * Submit a vouch for an enrollment
   *
   * @param enrollmentId - Enrollment ID to vouch for
   * @param statement - Vouch statement explaining verification
   * @returns Vouch response
   */
  async submitVouch(
    enrollmentId: string,
    statement: string
  ): Promise<import('./steward-types').VouchResponse> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/sdis/vouch/${enrollmentId}`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify({
        vouch_statement: statement,
        steward_did: this._authState.did,
      }),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to submit vouch: ${error}`);
    }

    return response.json();
  }

  /**
   * Reject an enrollment
   *
   * @param enrollmentId - Enrollment ID to reject
   * @param reason - Reason for rejection
   * @returns Reject response
   */
  async rejectEnrollment(
    enrollmentId: string,
    reason: string
  ): Promise<import('./steward-types').RejectResponse> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/sdis/reject/${enrollmentId}`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify({
        reason,
        steward_did: this._authState.did,
      }),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to reject enrollment: ${error}`);
    }

    return response.json();
  }

  /**
   * Get steward statistics
   *
   * @returns Steward stats including vouch counts and reputation
   */
  async getStewardStats(): Promise<import('./steward-types').StewardStats> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/sdis/steward/stats`, {
      method: 'GET',
      headers: {
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to get steward stats: ${error}`);
    }

    return response.json();
  }

  /**
   * Get vouch history
   *
   * @param limit - Maximum number of records to return (default 50)
   * @param offset - Number of records to skip (default 0)
   * @returns Vouch history response
   */
  async getVouchHistory(
    limit = 50,
    offset = 0
  ): Promise<import('./steward-types').VouchHistoryResponse> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(
      `${baseUrl}/v1/sdis/steward/history?limit=${limit}&offset=${offset}`,
      {
        method: 'GET',
        headers: {
          ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
        },
      }
    );

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to get vouch history: ${error}`);
    }

    return response.json();
  }

  // ===========================================================================
  // Member Methods
  // ===========================================================================

  /**
   * Get member profile information
   *
   * @param coopId - Cooperative ID
   * @param did - Member DID to lookup
   * @returns Member profile with balance, transaction count, and role
   */
  async getMemberProfile(coopId: string, did: string): Promise<import('@icn/client').MemberProfile> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/members/${coopId}/${did}`, {
      method: 'GET',
      headers: {
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to get member profile: ${error}`);
    }

    return response.json();
  }

  // ===========================================================================
  // Trust Graph Methods
  // ===========================================================================

  /**
   * Get trust score for a DID
   * 
   * Returns the trust score and classification from the authenticated user's perspective.
   * Requires authentication.
   * 
   * @param did - Target DID to get trust score for
   * @returns Trust score response with score and classification
   */
  async getTrustScore(did: string): Promise<{
    did: string;
    trust_score: number;
    trust_class: 'Isolated' | 'Known' | 'Partner' | 'Federated';
  }> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/trust/${did}`, {
      method: 'GET',
      headers: {
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to get trust score: ${error}`);
    }

    return response.json();
  }

  /**
   * Get trust edges from a DID
   * 
   * Returns all outgoing trust edges (attestations) from the specified DID.
   * 
   * @param did - DID to get trust edges from
   * @returns Array of trust edges
   */
  async getTrustEdges(did: string): Promise<Array<{
    from: string;
    to: string;
    score: number;
    created_at: number;
    labels?: string[];
  }>> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/trust/${did}/edges`, {
      method: 'GET',
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to get trust edges: ${error}`);
    }

    return response.json();
  }

  /**
   * Create a trust attestation
   * 
   * Creates a trust edge from the authenticated user to another DID.
   * Score should be between 0.0 (no trust) and 1.0 (full trust).
   * Requires authentication.
   * 
   * @param to - Target DID to attest
   * @param score - Trust score (0.0 - 1.0)
   * @param memo - Optional reason/memo for the attestation
   */
  async createTrustAttestation(
    to: string,
    score: number,
    memo?: string
  ): Promise<{ success: boolean; message: string }> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/trust/attest`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify({ to, score, memo }),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to create attestation: ${error}`);
    }

    return response.json();
  }

  /**
   * Alias for createTrustAttestation with different parameter names (for hook compatibility)
   */
  async attestTrust(targetDid: string, score: number, context?: string): Promise<void> {
    await this.createTrustAttestation(targetDid, score, context);
  }

  /**
   * Get trust network for visualization
   * 
   * Returns nodes and edges in the trust network around a DID,
   * useful for visualizing trust relationships.
   * 
   * @param did - Center DID for the network
   * @param depth - Maximum distance to explore (default: 2, max: 5)
   * @returns Trust network with nodes and edges
   */
  async getTrustNetwork(
    did: string,
    depth: number = 2
  ): Promise<{
    nodes: Array<{
      did: string;
      trust_score: number;
      distance: number;
    }>;
    edges: Array<{
      from: string;
      to: string;
      score: number;
      created_at: number;
      labels?: string[];
    }>;
  }> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(
      `${baseUrl}/v1/trust/${did}/network?depth=${depth}`,
      {
        method: 'GET',
      }
    );

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to get trust network: ${error}`);
    }

    return response.json();
  }

  /**
   * Format proof type for API request
   */
  private formatProofTypeForApi(proofType: ProofType): object {
    switch (proofType.type) {
      case 'age':
        return { type: 'age', threshold: proofType.threshold };
      case 'citizenship':
        return { type: 'citizenship', country_code: proofType.country_code };
      case 'membership':
        return { type: 'membership' };
      case 'non_revocation':
        return { type: 'non_revocation' };
      case 'custom':
        return { type: 'custom', circuit_id: proofType.circuit_id };
      default:
        return { type: 'unknown' };
    }
  }

  private getWebSocketUrl(coopId: string): string {
    // Get base URL from the parent class (we need to access private field)
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const wsUrl = baseUrl.replace(/^http/, 'ws');
    return `${wsUrl}/v1/ws/${coopId}`;
  }

  private getToken(): string | undefined {
    // Access the private token field
    return (this as unknown as { token?: string }).token;
  }
}

/**
 * Create a mobile ICN client
 */
export function createMobileClient(options: ICNMobileClientOptions): ICNMobileClient {
  return new ICNMobileClient(options);
}
