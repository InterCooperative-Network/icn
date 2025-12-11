/**
 * ICN Client Setup
 *
 * Configures the mobile client with secure storage.
 */

import { Platform } from 'react-native';
import { createWallet, createMobileClient, SecureStorage } from '@icn/react-native';

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

// Use web storage on web, native secure store on mobile
const secureStorage: SecureStorage = Platform.OS === 'web' ? webStorage : webStorage;

// Create wallet for key management
export const wallet = createWallet(secureStorage);

// Create mobile client
// Replace with your ICN gateway URL
const GATEWAY_URL = 'https://icn.mycoop.org';

export const client = createMobileClient({
  baseUrl: GATEWAY_URL,
  wallet,
  storage: secureStorage,
});

// Initialize client (call on app startup)
export async function initializeClient(): Promise<void> {
  // Ensure wallet has a key pair
  if (!(await wallet.hasKeyPair())) {
    await wallet.generateKeyPair();
  }

  // Load persisted auth state
  await client.initialize();
}
