/**
 * React Native ICN SDK Types
 */

// Re-export all types from the core SDK
export * from '@icn/client';

/**
 * Secure storage interface for managing keys on mobile devices.
 * Implementations should use iOS Keychain / Android Keystore.
 */
export interface SecureStorage {
  /**
   * Store a value securely
   * @param key - Storage key
   * @param value - Value to store (will be encrypted at rest)
   */
  setItem(key: string, value: string): Promise<void>;

  /**
   * Retrieve a securely stored value
   * @param key - Storage key
   * @returns The stored value, or null if not found
   */
  getItem(key: string): Promise<string | null>;

  /**
   * Remove a securely stored value
   * @param key - Storage key
   */
  removeItem(key: string): Promise<void>;

  /**
   * Check if a key exists in secure storage
   * @param key - Storage key
   */
  hasItem(key: string): Promise<boolean>;
}

/**
 * Key pair for signing operations
 */
export interface KeyPair {
  /** Public key in hex format */
  publicKey: string;
  /** DID derived from the public key */
  did: string;
}

/**
 * Wallet interface for managing identity and signing
 */
export interface ICNWallet {
  /**
   * Generate a new key pair
   */
  generateKeyPair(): Promise<KeyPair>;

  /**
   * Import an existing key pair from a seed phrase or private key
   */
  importKeyPair(privateKey: string): Promise<KeyPair>;

  /**
   * Get the current key pair (if one exists)
   */
  getKeyPair(): Promise<KeyPair | null>;

  /**
   * Delete the stored key pair
   */
  deleteKeyPair(): Promise<void>;

  /**
   * Sign a message with the stored private key
   * @param message - Message to sign (typically a challenge)
   * @returns Base64-encoded signature
   */
  sign(message: string): Promise<string>;

  /**
   * Check if a key pair is stored
   */
  hasKeyPair(): Promise<boolean>;
}

/**
 * QR code data for payments
 */
export interface PaymentQRData {
  /** Target DID to pay */
  to: string;
  /** Amount to pay */
  amount: number;
  /** Optional memo */
  memo?: string;
  /** Cooperative ID */
  coopId: string;
}

/**
 * Options for the mobile ICN client
 */
export interface ICNMobileClientOptions {
  /** Gateway base URL */
  baseUrl: string;
  /** Optional wallet for automatic signing */
  wallet?: ICNWallet;
  /** Request timeout in milliseconds */
  timeout?: number;
  /** Optional secure storage for caching */
  storage?: SecureStorage;
}

/**
 * Authentication state
 */
export interface AuthState {
  /** Whether the user is authenticated */
  isAuthenticated: boolean;
  /** Current DID */
  did: string | null;
  /** Current cooperative ID */
  coopId: string | null;
  /** Token expiration time */
  expiresAt: number | null;
}

/**
 * WebSocket connection state
 */
export type WebSocketState = 'connecting' | 'connected' | 'disconnected' | 'error';

/**
 * Event listener for real-time events
 */
export type EventListener<T> = (event: T) => void;

/**
 * Unsubscribe function
 */
export type Unsubscribe = () => void;

/**
 * WebSocket message from the gateway
 */
export interface WsMessage {
  type: string;
  data?: unknown;
  [key: string]: unknown;
}

/**
 * Network connection state
 */
export type NetworkState = 'online' | 'offline' | 'slow';

/**
 * Queued operation for offline support
 */
export interface QueuedOperation {
  /** Unique ID for the operation */
  id: string;
  /** Type of operation */
  type: 'payment' | 'vote' | 'proposal';
  /** Operation data */
  data: unknown;
  /** Timestamp when queued */
  queuedAt: number;
  /** Number of retry attempts */
  retries: number;
  /** Status of the operation */
  status: 'pending' | 'processing' | 'failed' | 'completed';
  /** Error message if failed */
  error?: string;
}

/**
 * Error types for better error handling
 */
export enum ICNErrorType {
  Network = 'network',
  Authentication = 'authentication',
  Validation = 'validation',
  NotFound = 'not_found',
  RateLimit = 'rate_limit',
  ServerError = 'server_error',
  Unknown = 'unknown',
}

/**
 * Structured error for better handling
 */
export interface ICNError {
  type: ICNErrorType;
  message: string;
  userMessage: string;
  retryable: boolean;
  originalError?: Error;
}
