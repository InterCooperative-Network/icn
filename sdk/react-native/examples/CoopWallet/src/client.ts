/**
 * ICN Client Setup
 *
 * Configures the mobile client with secure storage.
 */

import { Platform } from 'react-native';
import { createWallet, createMobileClient, SecureStorage, ICNWallet, ICNMobileClient } from '@icn/react-native';
import { GATEWAY_URL, APP_CONFIG } from './config';

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

// Initialize client (call on app startup)
export async function initializeClient(): Promise<void> {
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
