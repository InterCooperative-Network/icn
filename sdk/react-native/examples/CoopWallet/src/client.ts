/**
 * ICN Client Setup
 *
 * Configures the mobile client with secure storage.
 */

import * as SecureStore from 'expo-secure-store';
import { createWallet, createMobileClient, SecureStorage } from '@icn/react-native';

// Expo SecureStore adapter
const secureStorage: SecureStorage = {
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
