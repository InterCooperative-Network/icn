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
} from './types';

const TOKEN_KEY = '@icn/auth_token';
const DID_KEY = '@icn/did';
const COOP_KEY = '@icn/coop_id';
const EXPIRES_KEY = '@icn/expires_at';

/**
 * Mobile-optimized ICN client with persistent auth and wallet integration
 */
export class ICNMobileClient extends ICNClient {
  private wallet?: ICNWallet;
  private storage?: SecureStorage;
  private _authState: AuthState = {
    isAuthenticated: false,
    did: null,
    coopId: null,
    expiresAt: null,
  };
  private authListeners: Set<EventListener<AuthState>> = new Set();
  private wsSocket: WebSocket | null = null;
  private wsState: WebSocketState = 'disconnected';
  private wsListeners: Map<string, Set<EventListener<WsMessage>>> = new Map();
  private wsStateListeners: Set<EventListener<WebSocketState>> = new Set();
  private reconnectAttempts = 0;
  private maxReconnectAttempts = 5;
  private reconnectDelay = 1000;
  private intentionalDisconnect = false;

  constructor(options: ICNMobileClientOptions) {
    // Create base client options
    const baseOptions: ICNClientOptions = {
      baseUrl: options.baseUrl,
      timeout: options.timeout,
    };

    super(baseOptions);

    this.wallet = options.wallet;
    this.storage = options.storage;
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
    } catch (error) {
      console.warn('Failed to load persisted auth state:', error);
    }
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
    if (this.wsSocket) {
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

    setTimeout(() => {
      if (this.wsState === 'disconnected') {
        this.createWebSocket(coopId);
      }
    }, delay);
  }

  private notifyEventListeners(message: WsMessage): void {
    // For Event messages, extract the actual event type from the nested payload
    // WsEventMessage has { type: 'Event', event: { type: 'PaymentCreated', ... } }
    let eventType = message.type;
    if (message.type === 'Event' && 'event' in message && message.event?.type) {
      eventType = message.event.type;
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
