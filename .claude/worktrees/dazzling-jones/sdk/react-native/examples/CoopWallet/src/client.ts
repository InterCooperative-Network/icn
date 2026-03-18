/**
 * ICN Client Setup
 *
 * Configures the mobile client with secure storage.
 * Includes retry logic and comprehensive error handling.
 */

import { Platform } from 'react-native';
import { createWallet, createMobileClient, SecureStorage, ICNWallet, ICNMobileClient } from '@icn/react-native';
import { GATEWAY_URL, APP_CONFIG } from './config';

/** Client initialization state */
export enum ClientState {
  Uninitialized = 'uninitialized',
  Initializing = 'initializing',
  Ready = 'ready',
  Failed = 'failed',
}

/** Error info for initialization failures */
export interface InitError {
  message: string;
  code: string;
  retriable: boolean;
  details?: string;
}

/** Current client state */
export let clientState: ClientState = ClientState.Uninitialized;

/** Last initialization error */
export let lastInitError: InitError | null = null;

/** Initialization attempt count */
let initAttempts = 0;
const MAX_INIT_ATTEMPTS = 3;
const RETRY_DELAY_MS = 1000;

// Web localStorage adapter (fallback for web)
const webStorage: SecureStorage = {
  async setItem(key: string, value: string): Promise<void> {
    localStorage.setItem(key, value);
  },
  async getItem(key: string): Promise<string | null> {
    return localStorage.getItem(key);
  },
  async removeItem(key: string): Promise<void> {
    localStorage.removeItem(key);
  },
  async hasItem(key: string): Promise<boolean> {
    return localStorage.getItem(key) !== null;
  },
};

// Native SecureStore adapter
const getNativeStorage = async (): Promise<SecureStorage> => {
  const SecureStore = await import('expo-secure-store');
  return {
    async setItem(key: string, value: string): Promise<void> {
      await SecureStore.setItemAsync(key, value);
    },
    async getItem(key: string): Promise<string | null> {
      return SecureStore.getItemAsync(key);
    },
    async removeItem(key: string): Promise<void> {
      await SecureStore.deleteItemAsync(key);
    },
    async hasItem(key: string): Promise<boolean> {
      const value = await SecureStore.getItemAsync(key);
      return value !== null;
    },
  };
};

// Storage, wallet, and client are initialized asynchronously
let secureStorage: SecureStorage | null = null;
export let wallet: ICNWallet | null = null;
export let client: ICNMobileClient | null = null;

/**
 * Classify an error to determine if it's retriable.
 */
function classifyError(error: unknown): InitError {
  const err = error as Error;
  const message = err?.message || 'Unknown error';

  // Network errors - retriable
  if (message.includes('Network') || message.includes('fetch') || message.includes('timeout')) {
    return {
      message: 'Network connection failed',
      code: 'NETWORK_ERROR',
      retriable: true,
      details: message,
    };
  }

  // Storage errors - might be retriable
  if (message.includes('Storage') || message.includes('SecureStore')) {
    return {
      message: 'Secure storage error',
      code: 'STORAGE_ERROR',
      retriable: true,
      details: message,
    };
  }

  // Crypto/key errors - not retriable without reset
  if (message.includes('key') || message.includes('crypto') || message.includes('Invalid')) {
    return {
      message: 'Cryptographic error',
      code: 'CRYPTO_ERROR',
      retriable: false,
      details: message,
    };
  }

  // Default - retriable
  return {
    message: 'Initialization failed',
    code: 'INIT_ERROR',
    retriable: true,
    details: message,
  };
}

/**
 * Sleep for a specified duration.
 */
function sleep(ms: number): Promise<void> {
  return new Promise(resolve => setTimeout(resolve, ms));
}

/**
 * Internal initialization logic.
 */
async function doInitialize(): Promise<void> {
  if (APP_CONFIG.debug) {
    console.log('Initializing client...');
    console.log('Platform:', Platform.OS);
    console.log('Gateway URL:', GATEWAY_URL);
  }

  // Get platform-appropriate storage
  if (Platform.OS === 'web') {
    secureStorage = webStorage;
  } else {
    // Use native secure storage on iOS/Android
    secureStorage = await getNativeStorage();
  }
  if (APP_CONFIG.debug) console.log('Storage initialized');

  // Create wallet for key management
  wallet = createWallet(secureStorage);
  if (APP_CONFIG.debug) console.log('Wallet created');

  // Ensure wallet has a key pair
  if (!(await wallet.hasKeyPair())) {
    if (APP_CONFIG.debug) console.log('Generating new key pair...');
    const keyPair = await wallet.generateKeyPair();
    if (APP_CONFIG.debug) console.log('Key pair generated, DID:', keyPair.did);
  } else {
    const keyPair = await wallet.getKeyPair();
    if (APP_CONFIG.debug) console.log('Existing key pair found, DID:', keyPair?.did);
  }

  // Create mobile client
  client = createMobileClient({
    baseUrl: GATEWAY_URL,
    wallet,
    storage: secureStorage,
  });
  if (APP_CONFIG.debug) console.log('Mobile client created');

  // Load persisted auth state
  await client.initialize();
  if (APP_CONFIG.debug) console.log('Client initialized, auth state:', client.authState);
}

/**
 * Initialize client with retry logic (call on app startup).
 * Returns true if initialization succeeded, false otherwise.
 */
export async function initializeClient(): Promise<boolean> {
  // Prevent concurrent initialization
  if (clientState === ClientState.Initializing) {
    console.warn('Client initialization already in progress');
    return false;
  }

  // If already ready, return success
  if (clientState === ClientState.Ready && client) {
    return true;
  }

  clientState = ClientState.Initializing;
  lastInitError = null;

  while (initAttempts < MAX_INIT_ATTEMPTS) {
    initAttempts++;

    try {
      if (APP_CONFIG.debug) {
        console.log(`Initialization attempt ${initAttempts}/${MAX_INIT_ATTEMPTS}`);
      }

      await doInitialize();

      // Success!
      clientState = ClientState.Ready;
      lastInitError = null;
      initAttempts = 0; // Reset for future retries
      return true;

    } catch (error) {
      console.error(`Initialization attempt ${initAttempts} failed:`, error);
      lastInitError = classifyError(error);

      // If error is not retriable, don't retry
      if (!lastInitError.retriable) {
        break;
      }

      // Wait before retrying (exponential backoff)
      if (initAttempts < MAX_INIT_ATTEMPTS) {
        const delay = RETRY_DELAY_MS * Math.pow(2, initAttempts - 1);
        console.log(`Retrying in ${delay}ms...`);
        await sleep(delay);
      }
    }
  }

  // All retries failed
  clientState = ClientState.Failed;
  console.error('Client initialization failed after all retries');
  return false;
}

/**
 * Force retry initialization (resets attempt counter).
 */
export async function retryInitialization(): Promise<boolean> {
  initAttempts = 0;
  clientState = ClientState.Uninitialized;
  return initializeClient();
}

/**
 * Reset client state completely (for wallet reset scenarios).
 */
export function resetClientState(): void {
  clientState = ClientState.Uninitialized;
  lastInitError = null;
  initAttempts = 0;
  wallet = null;
  client = null;
  secureStorage = null;
}

/**
 * Check if client is ready to use.
 */
export function isClientReady(): boolean {
  return clientState === ClientState.Ready && client !== null;
}
