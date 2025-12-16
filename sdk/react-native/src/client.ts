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
  // Enrollment Methods (for enrollees)
  // ===========================================================================

  /**
   * Start an enrollment process
   *
   * This creates a new enrollment session and returns a QR code for device verification.
   *
   * @param identityName - Display name for the identity
   * @param coopId - Cooperative to join
   * @returns Enrollment response with QR code data
   */
  async startEnrollment(
    identityName: string,
    coopId: string
  ): Promise<import('./steward-types').StartEnrollmentResponse> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/enrollment/start`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        identity_name: identityName,
        coop_id: coopId,
      }),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to start enrollment: ${error}`);
    }

    return response.json();
  }

  /**
   * Submit device proof for Level 1 verification
   *
   * The device proof contains:
   * - ephemeral_did: A DID created on this device
   * - signature: Hex-encoded Ed25519 signature over the enrollment_id
   *
   * @param enrollmentId - Enrollment ID from startEnrollment or QR scan
   * @param deviceProof - Device proof with ephemeral DID and signature
   * @returns Verification response
   */
  async verifyDevice(
    enrollmentId: string,
    deviceProof: import('./steward-types').DeviceProof
  ): Promise<import('./steward-types').VerifyDeviceResponse> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/verify/level1`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        enrollment_id: enrollmentId,
        device_proof: deviceProof,
      }),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Device verification failed: ${error}`);
    }

    return response.json();
  }

  /**
   * Complete enrollment after steward vouch
   *
   * This finalizes the enrollment and returns:
   * - The new DID (same as ephemeral_did)
   * - Recovery codes for backup
   * - Auth token for immediate use
   *
   * @param enrollmentId - Enrollment ID
   * @param ephemeralDid - The ephemeral DID from device verification
   * @param ephemeralSignature - Hex-encoded signature over "complete:{enrollmentId}"
   * @param deviceInfo - Information about the device
   * @returns Completion response with DID, recovery codes, and auth token
   */
  async completeEnrollment(
    enrollmentId: string,
    ephemeralDid: string,
    ephemeralSignature: string,
    deviceInfo: import('./steward-types').DeviceInfo
  ): Promise<import('./steward-types').CompleteEnrollmentResponse> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/enrollment/complete`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
      },
      body: JSON.stringify({
        enrollment_id: enrollmentId,
        ephemeral_did: ephemeralDid,
        ephemeral_signature: ephemeralSignature,
        device_info: deviceInfo,
      }),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Enrollment completion failed: ${error}`);
    }

    const result = await response.json();

    // Store the auth token and update state
    if (result.auth_token && this.storage) {
      const token = result.auth_token.replace(/^Bearer\s+/i, '');
      this.setToken(token);

      // Persist auth state - enrollment doesn't return expiry, use 1 hour default
      const expiresAt = Date.now() + 3600000;
      await this.persistAuth(token, result.did, deviceInfo.os, expiresAt);

      this.updateAuthState({
        isAuthenticated: true,
        did: result.did,
        coopId: null, // Will be set when user selects coop
        expiresAt,
      });
    }

    return result;
  }

  /**
   * Helper to create device proof using the wallet
   *
   * @param enrollmentId - Enrollment ID to sign
   * @returns Device proof ready for verifyDevice()
   */
  async createDeviceProof(
    enrollmentId: string
  ): Promise<import('./steward-types').DeviceProof> {
    if (!this.wallet) {
      throw new Error('No wallet configured');
    }

    // Get or generate ephemeral keypair
    let keyPair = await this.wallet.getKeyPair();
    if (!keyPair) {
      keyPair = await this.wallet.generateKeyPair();
    }

    // Sign the enrollment_id
    const signature = await this.wallet.sign(enrollmentId);

    return {
      ephemeral_did: keyPair.did,
      signature,
    };
  }

  /**
   * Helper to create completion signature using the wallet
   *
   * @param enrollmentId - Enrollment ID
   * @returns Hex-encoded signature for completeEnrollment()
   */
  async createCompletionSignature(enrollmentId: string): Promise<string> {
    if (!this.wallet) {
      throw new Error('No wallet configured');
    }

    const message = `complete:${enrollmentId}`;
    return this.wallet.sign(message);
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

  // ===========================================================================
  // Constitutional Governance Methods (Amendments & Appeals)
  // ===========================================================================

  /**
   * Create a new amendment
   *
   * @param request - Amendment creation request
   * @returns Created amendment
   */
  async createAmendment(
    request: import('./constitutional-types').CreateAmendmentRequest
  ): Promise<import('./constitutional-types').Amendment> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/constitutional/amendments`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify(request),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to create amendment: ${error}`);
    }

    return response.json();
  }

  /**
   * Get amendment by ID
   *
   * @param amendmentId - Amendment ID (hex)
   * @returns Amendment details
   */
  async getAmendment(amendmentId: string): Promise<import('./constitutional-types').Amendment> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/constitutional/amendments/${amendmentId}`, {
      method: 'GET',
      headers: {
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to get amendment: ${error}`);
    }

    return response.json();
  }

  /**
   * List amendments with optional filters
   *
   * @param status - Optional status filter
   * @param scope - Optional scope filter
   * @param type - Optional type filter
   * @returns Amendment list
   */
  async listAmendments(
    status?: string,
    scope?: string,
    type?: string
  ): Promise<import('./constitutional-types').AmendmentListResponse> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const params = new URLSearchParams();
    if (status) params.set('status', status);
    if (scope) params.set('scope', scope);
    if (type) params.set('type', type);

    const url = `${baseUrl}/v1/constitutional/amendments${params.toString() ? `?${params}` : ''}`;
    const response = await fetch(url, {
      method: 'GET',
      headers: {
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to list amendments: ${error}`);
    }

    return response.json();
  }

  /**
   * Add a change to a draft amendment
   *
   * @param amendmentId - Amendment ID (hex)
   * @param change - Change to add
   * @returns Updated amendment
   */
  async addAmendmentChange(
    amendmentId: string,
    change: import('./constitutional-types').AddChangeRequest
  ): Promise<import('./constitutional-types').Amendment> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/constitutional/amendments/${amendmentId}/changes`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify(change),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to add change: ${error}`);
    }

    return response.json();
  }

  /**
   * Submit amendment for review
   *
   * @param amendmentId - Amendment ID (hex)
   * @returns Updated amendment
   */
  async submitAmendment(amendmentId: string): Promise<import('./constitutional-types').Amendment> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/constitutional/amendments/${amendmentId}/submit`, {
      method: 'POST',
      headers: {
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to submit amendment: ${error}`);
    }

    return response.json();
  }

  /**
   * Open voting on an amendment
   *
   * @param amendmentId - Amendment ID (hex)
   * @returns Updated amendment
   */
  async openAmendmentVoting(amendmentId: string): Promise<import('./constitutional-types').Amendment> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/constitutional/amendments/${amendmentId}/vote`, {
      method: 'POST',
      headers: {
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to open voting: ${error}`);
    }

    return response.json();
  }

  /**
   * Ratify an amendment
   *
   * @param amendmentId - Amendment ID (hex)
   * @param request - Ratification request
   * @returns Updated amendment
   */
  async ratifyAmendment(
    amendmentId: string,
    request: import('./constitutional-types').RatifyRequest
  ): Promise<import('./constitutional-types').Amendment> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/constitutional/amendments/${amendmentId}/ratify`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify(request),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to ratify amendment: ${error}`);
    }

    return response.json();
  }

  /**
   * Withdraw an amendment
   *
   * @param amendmentId - Amendment ID (hex)
   * @param reason - Reason for withdrawal
   * @returns Updated amendment
   */
  async withdrawAmendment(
    amendmentId: string,
    reason?: string
  ): Promise<import('./constitutional-types').Amendment> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/constitutional/amendments/${amendmentId}/withdraw`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify({ reason }),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to withdraw amendment: ${error}`);
    }

    return response.json();
  }

  /**
   * File an appeal
   *
   * @param request - Appeal filing request
   * @returns Filed appeal
   */
  async fileAppeal(
    request: import('./constitutional-types').FileAppealRequest
  ): Promise<import('./constitutional-types').Appeal> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/constitutional/appeals`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify(request),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to file appeal: ${error}`);
    }

    return response.json();
  }

  /**
   * Get appeal by ID
   *
   * @param appealId - Appeal ID (hex)
   * @returns Appeal details
   */
  async getAppeal(appealId: string): Promise<import('./constitutional-types').Appeal> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/constitutional/appeals/${appealId}`, {
      method: 'GET',
      headers: {
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to get appeal: ${error}`);
    }

    return response.json();
  }

  /**
   * List appeals with optional filters
   *
   * @param status - Optional status filter
   * @param scope - Optional scope filter
   * @param appellant - Optional appellant filter
   * @returns Appeal list
   */
  async listAppeals(
    status?: string,
    scope?: string,
    appellant?: string
  ): Promise<import('./constitutional-types').AppealListResponse> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const params = new URLSearchParams();
    if (status) params.set('status', status);
    if (scope) params.set('scope', scope);
    if (appellant) params.set('appellant', appellant);

    const url = `${baseUrl}/v1/constitutional/appeals${params.toString() ? `?${params}` : ''}`;
    const response = await fetch(url, {
      method: 'GET',
      headers: {
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to list appeals: ${error}`);
    }

    return response.json();
  }

  /**
   * Add evidence to an appeal
   *
   * @param appealId - Appeal ID (hex)
   * @param evidence - Evidence to add
   * @returns Updated appeal
   */
  async addAppealEvidence(
    appealId: string,
    evidence: import('./constitutional-types').AddEvidenceRequest
  ): Promise<import('./constitutional-types').Appeal> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/constitutional/appeals/${appealId}/evidence`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify(evidence),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to add evidence: ${error}`);
    }

    return response.json();
  }

  /**
   * Add response to an appeal
   *
   * @param appealId - Appeal ID (hex)
   * @param appealResponse - Response to add
   * @returns Updated appeal
   */
  async addAppealResponse(
    appealId: string,
    appealResponse: import('./constitutional-types').AddResponseRequest
  ): Promise<import('./constitutional-types').Appeal> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/constitutional/appeals/${appealId}/respond`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify(appealResponse),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to add response: ${error}`);
    }

    return response.json();
  }

  /**
   * Begin review of an appeal (admin)
   *
   * @param appealId - Appeal ID (hex)
   * @returns Updated appeal
   */
  async beginAppealReview(appealId: string): Promise<import('./constitutional-types').Appeal> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/constitutional/appeals/${appealId}/review`, {
      method: 'POST',
      headers: {
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to begin review: ${error}`);
    }

    return response.json();
  }

  /**
   * Resolve an appeal (admin)
   *
   * @param appealId - Appeal ID (hex)
   * @param resolution - Resolution request
   * @returns Updated appeal
   */
  async resolveAppeal(
    appealId: string,
    resolution: import('./constitutional-types').ResolveAppealRequest
  ): Promise<import('./constitutional-types').Appeal> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/constitutional/appeals/${appealId}/resolve`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify(resolution),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to resolve appeal: ${error}`);
    }

    return response.json();
  }

  /**
   * Withdraw an appeal
   *
   * @param appealId - Appeal ID (hex)
   * @param reason - Reason for withdrawal
   * @returns Updated appeal
   */
  async withdrawAppeal(
    appealId: string,
    reason?: string
  ): Promise<import('./constitutional-types').Appeal> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/constitutional/appeals/${appealId}/withdraw`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify({ reason }),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to withdraw appeal: ${error}`);
    }

    return response.json();
  }

  // ===========================================================================
  // Charter Methods
  // ===========================================================================

  /**
   * Create a new charter
   *
   * @param request - Charter creation request
   * @returns Created charter
   */
  async createCharter(
    request: import('./charter-types').CreateCharterRequest
  ): Promise<import('./charter-types').Charter> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/charter`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify(request),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to create charter: ${error}`);
    }

    return response.json();
  }

  /**
   * Get charter by ID
   *
   * @param charterId - Charter ID (hex)
   * @returns Charter details
   */
  async getCharter(charterId: string): Promise<import('./charter-types').Charter> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/charter/${charterId}`, {
      method: 'GET',
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to get charter: ${error}`);
    }

    return response.json();
  }

  /**
   * Get charter by domain ID
   *
   * @param domainId - Domain ID (e.g., "coop:my-coop")
   * @returns Charter details
   */
  async getCharterByDomain(domainId: string): Promise<import('./charter-types').Charter> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/charter/by-domain/${domainId}`, {
      method: 'GET',
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to get charter: ${error}`);
    }

    return response.json();
  }

  /**
   * List charters with optional filters
   *
   * @param orgType - Optional org type filter
   * @param status - Optional status filter
   * @returns List of charter summaries
   */
  async listCharters(
    orgType?: string,
    status?: string
  ): Promise<import('./charter-types').CharterSummary[]> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const params = new URLSearchParams();
    if (orgType) params.set('org_type', orgType);
    if (status) params.set('status', status);

    const url = `${baseUrl}/v1/charter${params.toString() ? `?${params}` : ''}`;
    const response = await fetch(url, {
      method: 'GET',
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to list charters: ${error}`);
    }

    return response.json();
  }

  /**
   * Sign a charter as a founder
   *
   * @param charterId - Charter ID (hex)
   * @param signature - Signature hex
   * @param role - Optional role
   * @returns Sign response with founder count
   */
  async signCharter(
    charterId: string,
    signature: string,
    role?: string
  ): Promise<import('./charter-types').CharterSignResponse> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/charter/${charterId}/sign`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify({ signature, role }),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to sign charter: ${error}`);
    }

    return response.json();
  }

  /**
   * Activate a charter (requires minimum founders)
   *
   * @param charterId - Charter ID (hex)
   * @returns Action response
   */
  async activateCharter(
    charterId: string
  ): Promise<import('./charter-types').CharterActionResponse> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/charter/${charterId}/activate`, {
      method: 'POST',
      headers: {
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to activate charter: ${error}`);
    }

    return response.json();
  }

  /**
   * Update charter status
   *
   * @param charterId - Charter ID (hex)
   * @param status - New status
   * @param reason - Optional reason (required for suspend)
   * @returns Action response
   */
  async updateCharterStatus(
    charterId: string,
    status: string,
    reason?: string
  ): Promise<import('./charter-types').CharterActionResponse> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/charter/${charterId}/status`, {
      method: 'PUT',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify({ status, reason }),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to update charter status: ${error}`);
    }

    return response.json();
  }

  // ===========================================================================
  // Membership Methods
  // ===========================================================================

  /**
   * Apply for membership in a jurisdiction
   *
   * @param jurisdictionId - The jurisdiction to join
   * @param capabilitiesRequested - Optional capabilities to request
   * @returns Membership response with initial status
   */
  async applyForMembership(
    jurisdictionId: string,
    capabilitiesRequested?: string[]
  ): Promise<import('./membership-types').MemberResponse> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/membership/apply`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify({
        jurisdiction_id: jurisdictionId,
        capabilities_requested: capabilitiesRequested || [],
      }),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to apply for membership: ${error}`);
    }

    return response.json();
  }

  /**
   * Get membership status in a jurisdiction
   *
   * @param jurisdictionId - The jurisdiction to check
   * @returns Member response with current status
   */
  async getMembershipStatus(
    jurisdictionId: string
  ): Promise<import('./membership-types').MemberResponse> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/membership/status/${jurisdictionId}`, {
      method: 'GET',
      headers: {
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to get membership status: ${error}`);
    }

    return response.json();
  }

  /**
   * List members of a jurisdiction
   *
   * @param jurisdictionId - The jurisdiction to list members from
   * @param status - Optional filter by membership status
   * @returns List of members
   */
  async listJurisdictionMembers(
    jurisdictionId: string,
    status?: string
  ): Promise<import('./membership-types').MemberListResponse> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const params = new URLSearchParams();
    if (status) params.set('status', status);

    const url = `${baseUrl}/v1/membership/list/${jurisdictionId}${params.toString() ? `?${params}` : ''}`;
    const response = await fetch(url, {
      method: 'GET',
      headers: {
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to list members: ${error}`);
    }

    return response.json();
  }

  /**
   * Approve a membership application (admin)
   *
   * @param holderId - The holder ID to approve
   * @param jurisdictionId - The jurisdiction
   * @returns Action response
   */
  async approveMembership(
    holderId: string,
    jurisdictionId: string
  ): Promise<import('./membership-types').MembershipActionResponse> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/membership/approve`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify({
        holder_id: holderId,
        jurisdiction_id: jurisdictionId,
      }),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to approve membership: ${error}`);
    }

    return response.json();
  }

  /**
   * Promote a provisional member to full member (admin)
   *
   * @param holderId - The holder ID to promote
   * @param jurisdictionId - The jurisdiction
   * @returns Action response
   */
  async promoteMember(
    holderId: string,
    jurisdictionId: string
  ): Promise<import('./membership-types').MembershipActionResponse> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/membership/promote`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify({
        holder_id: holderId,
        jurisdiction_id: jurisdictionId,
      }),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to promote member: ${error}`);
    }

    return response.json();
  }

  /**
   * Suspend a member (admin)
   *
   * @param holderId - The holder ID to suspend
   * @param jurisdictionId - The jurisdiction
   * @returns Action response
   */
  async suspendMember(
    holderId: string,
    jurisdictionId: string
  ): Promise<import('./membership-types').MembershipActionResponse> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/membership/suspend`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify({
        holder_id: holderId,
        jurisdiction_id: jurisdictionId,
      }),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to suspend member: ${error}`);
    }

    return response.json();
  }

  /**
   * Reinstate a suspended member (admin)
   *
   * @param holderId - The holder ID to reinstate
   * @param jurisdictionId - The jurisdiction
   * @returns Action response
   */
  async reinstateMember(
    holderId: string,
    jurisdictionId: string
  ): Promise<import('./membership-types').MembershipActionResponse> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/membership/reinstate`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify({
        holder_id: holderId,
        jurisdiction_id: jurisdictionId,
      }),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to reinstate member: ${error}`);
    }

    return response.json();
  }

  /**
   * Voluntarily exit a jurisdiction
   *
   * @param holderId - Your holder ID
   * @param jurisdictionId - The jurisdiction to leave
   * @returns Action response
   */
  async exitMembership(
    holderId: string,
    jurisdictionId: string
  ): Promise<import('./membership-types').MembershipActionResponse> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/membership/exit`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify({
        holder_id: holderId,
        jurisdiction_id: jurisdictionId,
      }),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to exit membership: ${error}`);
    }

    return response.json();
  }

  /**
   * Grant a capability to a member (admin)
   *
   * @param holderId - The holder ID
   * @param jurisdictionId - The jurisdiction
   * @param capability - The capability to grant
   * @returns Action response
   */
  async grantCapability(
    holderId: string,
    jurisdictionId: string,
    capability: string
  ): Promise<import('./membership-types').MembershipActionResponse> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/membership/capability/grant`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify({
        holder_id: holderId,
        jurisdiction_id: jurisdictionId,
        capability,
      }),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to grant capability: ${error}`);
    }

    return response.json();
  }

  /**
   * Revoke a capability from a member (admin)
   *
   * @param holderId - The holder ID
   * @param jurisdictionId - The jurisdiction
   * @param capability - The capability to revoke
   * @returns Action response
   */
  async revokeCapability(
    holderId: string,
    jurisdictionId: string,
    capability: string
  ): Promise<import('./membership-types').MembershipActionResponse> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/membership/capability/revoke`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify({
        holder_id: holderId,
        jurisdiction_id: jurisdictionId,
        capability,
      }),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to revoke capability: ${error}`);
    }

    return response.json();
  }

  /**
   * Check if a member has a capability
   *
   * @param holderId - The holder ID
   * @param jurisdictionId - The jurisdiction
   * @param capability - The capability to check
   * @returns Capability check response
   */
  async checkCapability(
    holderId: string,
    jurisdictionId: string,
    capability: string
  ): Promise<import('./membership-types').CapabilityCheckResponse> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const params = new URLSearchParams({
      holder_id: holderId,
      jurisdiction_id: jurisdictionId,
      capability,
    });

    const response = await fetch(`${baseUrl}/v1/membership/capability/check?${params}`, {
      method: 'GET',
      headers: {
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to check capability: ${error}`);
    }

    return response.json();
  }

  /**
   * Add a role to a member (admin)
   *
   * @param holderId - The holder ID
   * @param jurisdictionId - The jurisdiction
   * @param role - The role to add
   * @returns Action response
   */
  async addMemberRole(
    holderId: string,
    jurisdictionId: string,
    role: string
  ): Promise<import('./membership-types').MembershipActionResponse> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/membership/role/add`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify({
        holder_id: holderId,
        jurisdiction_id: jurisdictionId,
        role,
      }),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to add role: ${error}`);
    }

    return response.json();
  }

  /**
   * Remove a role from a member (admin)
   *
   * @param holderId - The holder ID
   * @param jurisdictionId - The jurisdiction
   * @param role - The role to remove
   * @returns Action response
   */
  async removeMemberRole(
    holderId: string,
    jurisdictionId: string,
    role: string
  ): Promise<import('./membership-types').MembershipActionResponse> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/membership/role/remove`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify({
        holder_id: holderId,
        jurisdiction_id: jurisdictionId,
        role,
      }),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to remove role: ${error}`);
    }

    return response.json();
  }

  // ============================================================================
  // Notification APIs
  // ============================================================================

  /**
   * List notifications for the authenticated user
   *
   * @param options - Optional filter options (limit, offset, unread_only)
   * @returns List of notifications with unread count
   */
  async listNotifications(options?: {
    limit?: number;
    offset?: number;
    unread_only?: boolean;
  }): Promise<import('./notification-hooks').NotificationListResponse> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const params = new URLSearchParams();
    if (options?.limit) params.set('limit', options.limit.toString());
    if (options?.offset) params.set('offset', options.offset.toString());
    if (options?.unread_only) params.set('unread_only', 'true');

    const url = `${baseUrl}/v1/notifications${params.toString() ? `?${params}` : ''}`;
    const response = await fetch(url, {
      method: 'GET',
      headers: {
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to list notifications: ${error}`);
    }

    return response.json();
  }

  /**
   * Get notification count (total and unread)
   *
   * @returns Notification counts
   */
  async getNotificationCount(): Promise<import('./notification-hooks').NotificationCountResponse> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/notifications/count`, {
      method: 'GET',
      headers: {
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to get notification count: ${error}`);
    }

    return response.json();
  }

  /**
   * Mark a notification as read
   *
   * @param notificationId - The notification ID
   * @returns Updated notification
   */
  async markNotificationRead(
    notificationId: string
  ): Promise<import('./notification-hooks').InAppNotification> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/notifications/${notificationId}/read`, {
      method: 'POST',
      headers: {
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to mark notification as read: ${error}`);
    }

    return response.json();
  }

  /**
   * Delete a notification
   *
   * @param notificationId - The notification ID
   */
  async deleteNotification(notificationId: string): Promise<void> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/notifications/${notificationId}`, {
      method: 'DELETE',
      headers: {
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to delete notification: ${error}`);
    }
  }

  // ============================================================================
  // Governance Dashboard APIs
  // ============================================================================

  /**
   * Get governance dashboard for a domain
   *
   * @param domainId - The governance domain ID
   * @returns Dashboard with amendments/appeals stats and recent activity
   */
  async getGovernanceDashboard(
    domainId: string
  ): Promise<import('./governance-dashboard-hooks').GovernanceDashboard> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(
      `${baseUrl}/v1/governance/${encodeURIComponent(domainId)}/dashboard`,
      {
        method: 'GET',
        headers: {
          ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
        },
      }
    );

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to get governance dashboard: ${error}`);
    }

    return response.json();
  }

  /**
   * Get participation stats for a domain
   *
   * @param domainId - The governance domain ID
   * @returns Participation statistics (voters, rates, averages)
   */
  async getGovernanceParticipation(
    domainId: string
  ): Promise<import('./governance-dashboard-hooks').ParticipationStats> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(
      `${baseUrl}/v1/governance/${encodeURIComponent(domainId)}/participation`,
      {
        method: 'GET',
        headers: {
          ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
        },
      }
    );

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to get participation stats: ${error}`);
    }

    return response.json();
  }

  // ============================================================================
  // Economic APIs (Recurring Payments, Escrow, Budgets)
  // ============================================================================

  /**
   * List recurring payments
   *
   * @param coopId - The cooperative ID
   * @param status - Optional filter by status (active, paused, completed, cancelled)
   * @returns List of recurring payments
   */
  async listRecurringPayments(
    coopId: string,
    status?: string
  ): Promise<import('./economic-hooks').RecurringPayment[]> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const params = new URLSearchParams();
    if (status) params.set('status', status);

    const url = `${baseUrl}/v1/ledger/${coopId}/recurring${params.toString() ? `?${params}` : ''}`;
    const response = await fetch(url, {
      method: 'GET',
      headers: {
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to list recurring payments: ${error}`);
    }

    return response.json();
  }

  /**
   * Create a recurring payment
   *
   * @param coopId - The cooperative ID
   * @param request - The recurring payment request
   * @returns Created recurring payment
   */
  async createRecurringPayment(
    coopId: string,
    request: import('./economic-hooks').CreateRecurringPaymentRequest
  ): Promise<import('./economic-hooks').RecurringPayment> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/ledger/${coopId}/recurring`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify(request),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to create recurring payment: ${error}`);
    }

    return response.json();
  }

  /**
   * Update recurring payment status (pause, resume, cancel)
   *
   * @param coopId - The cooperative ID
   * @param paymentId - The recurring payment ID
   * @param action - Action to perform: pause, resume, or cancel
   * @returns Updated recurring payment
   */
  async updateRecurringPaymentStatus(
    coopId: string,
    paymentId: string,
    action: 'pause' | 'resume' | 'cancel'
  ): Promise<import('./economic-hooks').RecurringPayment> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/ledger/${coopId}/recurring/${paymentId}/${action}`, {
      method: 'POST',
      headers: {
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to ${action} recurring payment: ${error}`);
    }

    return response.json();
  }

  /**
   * List escrow holds
   *
   * @param coopId - The cooperative ID
   * @param status - Optional filter by status (pending, released, refunded, expired)
   * @returns List of escrow holds
   */
  async listEscrows(
    coopId: string,
    status?: string
  ): Promise<import('./economic-hooks').Escrow[]> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const params = new URLSearchParams();
    if (status) params.set('status', status);

    const url = `${baseUrl}/v1/ledger/${coopId}/escrow${params.toString() ? `?${params}` : ''}`;
    const response = await fetch(url, {
      method: 'GET',
      headers: {
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to list escrows: ${error}`);
    }

    return response.json();
  }

  /**
   * Create an escrow hold
   *
   * @param coopId - The cooperative ID
   * @param request - The escrow request
   * @returns Created escrow
   */
  async createEscrow(
    coopId: string,
    request: import('./economic-hooks').CreateEscrowRequest
  ): Promise<import('./economic-hooks').Escrow> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/ledger/${coopId}/escrow`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify(request),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to create escrow: ${error}`);
    }

    return response.json();
  }

  /**
   * Release escrow funds to recipient
   *
   * @param coopId - The cooperative ID
   * @param escrowId - The escrow ID
   * @returns Release result with transaction hash
   */
  async releaseEscrow(
    coopId: string,
    escrowId: string
  ): Promise<{ escrow: import('./economic-hooks').Escrow; transaction_hash: string }> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/ledger/${coopId}/escrow/${escrowId}/release`, {
      method: 'POST',
      headers: {
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to release escrow: ${error}`);
    }

    return response.json();
  }

  /**
   * Refund escrow funds to creator
   *
   * @param coopId - The cooperative ID
   * @param escrowId - The escrow ID
   * @returns Refund result with transaction hash
   */
  async refundEscrow(
    coopId: string,
    escrowId: string
  ): Promise<{ escrow: import('./economic-hooks').Escrow; transaction_hash: string }> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/ledger/${coopId}/escrow/${escrowId}/refund`, {
      method: 'POST',
      headers: {
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to refund escrow: ${error}`);
    }

    return response.json();
  }

  /**
   * List budgets
   *
   * @param coopId - The cooperative ID
   * @returns List of budgets
   */
  async listBudgets(coopId: string): Promise<import('./economic-hooks').Budget[]> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/ledger/${coopId}/budgets`, {
      method: 'GET',
      headers: {
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to list budgets: ${error}`);
    }

    return response.json();
  }

  /**
   * Get a specific budget
   *
   * @param coopId - The cooperative ID
   * @param budgetId - The budget ID
   * @returns Budget details
   */
  async getBudget(coopId: string, budgetId: string): Promise<import('./economic-hooks').Budget> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/ledger/${coopId}/budgets/${budgetId}`, {
      method: 'GET',
      headers: {
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to get budget: ${error}`);
    }

    return response.json();
  }

  /**
   * Create a budget
   *
   * @param coopId - The cooperative ID
   * @param request - The budget request
   * @returns Created budget
   */
  async createBudget(
    coopId: string,
    request: import('./economic-hooks').CreateBudgetRequest
  ): Promise<import('./economic-hooks').Budget> {
    const baseUrl = (this as unknown as { baseUrl: string }).baseUrl;
    const response = await fetch(`${baseUrl}/v1/ledger/${coopId}/budgets`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        ...(this.hasToken() ? { Authorization: `Bearer ${this.getToken()}` } : {}),
      },
      body: JSON.stringify(request),
    });

    if (!response.ok) {
      const error = await response.text();
      throw new Error(`Failed to create budget: ${error}`);
    }

    return response.json();
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
