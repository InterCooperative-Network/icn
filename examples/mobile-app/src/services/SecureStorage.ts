/**
 * Secure Storage Service
 *
 * Uses expo-secure-store for sensitive credential storage.
 * Credentials are encrypted using the device's secure enclave (iOS Keychain / Android Keystore).
 *
 * Security features:
 * - Hardware-backed encryption on supported devices
 * - Credentials are not included in backups
 * - Automatic encryption at rest
 */

import * as SecureStore from 'expo-secure-store';
import AsyncStorage from '@react-native-async-storage/async-storage';

// Keys for secure storage
const SECURE_KEYS = {
  AUTH_TOKEN: 'icn_auth_token',
  API_URL: 'icn_api_url',
  DID: 'icn_did',
  COOP_ID: 'icn_coop_id',
} as const;

// Keys for non-sensitive storage (preferences, etc.)
const ASYNC_KEYS = {
  ONBOARDING_COMPLETE: 'icn_onboarding_complete',
  THEME_PREFERENCE: 'icn_theme',
  NOTIFICATION_SETTINGS: 'icn_notifications',
} as const;

/**
 * Options for SecureStore
 * - keychainAccessible: Controls when the keychain item is accessible
 *   WHEN_UNLOCKED: Only when device is unlocked (most secure for tokens)
 */
const secureStoreOptions: SecureStore.SecureStoreOptions = {
  keychainAccessible: SecureStore.WHEN_UNLOCKED,
};

/**
 * Secure Storage API
 *
 * All sensitive credentials (tokens, API URLs, DIDs) are stored securely.
 */
export const SecureStorage = {
  /**
   * Store the authentication token securely
   */
  async setAuthToken(token: string): Promise<void> {
    await SecureStore.setItemAsync(SECURE_KEYS.AUTH_TOKEN, token, secureStoreOptions);
  },

  /**
   * Retrieve the authentication token
   */
  async getAuthToken(): Promise<string | null> {
    return SecureStore.getItemAsync(SECURE_KEYS.AUTH_TOKEN);
  },

  /**
   * Remove the authentication token
   */
  async removeAuthToken(): Promise<void> {
    await SecureStore.deleteItemAsync(SECURE_KEYS.AUTH_TOKEN);
  },

  /**
   * Store the API URL securely
   */
  async setApiUrl(url: string): Promise<void> {
    await SecureStore.setItemAsync(SECURE_KEYS.API_URL, url, secureStoreOptions);
  },

  /**
   * Retrieve the API URL
   */
  async getApiUrl(): Promise<string | null> {
    return SecureStore.getItemAsync(SECURE_KEYS.API_URL);
  },

  /**
   * Store the user's DID securely
   */
  async setDid(did: string): Promise<void> {
    await SecureStore.setItemAsync(SECURE_KEYS.DID, did, secureStoreOptions);
  },

  /**
   * Retrieve the user's DID
   */
  async getDid(): Promise<string | null> {
    return SecureStore.getItemAsync(SECURE_KEYS.DID);
  },

  /**
   * Store the cooperative ID securely
   */
  async setCoopId(coopId: string): Promise<void> {
    await SecureStore.setItemAsync(SECURE_KEYS.COOP_ID, coopId, secureStoreOptions);
  },

  /**
   * Retrieve the cooperative ID
   */
  async getCoopId(): Promise<string | null> {
    return SecureStore.getItemAsync(SECURE_KEYS.COOP_ID);
  },

  /**
   * Store all authentication credentials at once
   */
  async setCredentials(credentials: {
    token: string;
    apiUrl: string;
    did?: string;
    coopId?: string;
  }): Promise<void> {
    await Promise.all([
      this.setAuthToken(credentials.token),
      this.setApiUrl(credentials.apiUrl),
      credentials.did ? this.setDid(credentials.did) : Promise.resolve(),
      credentials.coopId ? this.setCoopId(credentials.coopId) : Promise.resolve(),
    ]);
  },

  /**
   * Retrieve all stored credentials
   */
  async getCredentials(): Promise<{
    token: string | null;
    apiUrl: string | null;
    did: string | null;
    coopId: string | null;
  }> {
    const [token, apiUrl, did, coopId] = await Promise.all([
      this.getAuthToken(),
      this.getApiUrl(),
      this.getDid(),
      this.getCoopId(),
    ]);
    return { token, apiUrl, did, coopId };
  },

  /**
   * Clear all secure credentials (for logout)
   */
  async clearCredentials(): Promise<void> {
    await Promise.all([
      SecureStore.deleteItemAsync(SECURE_KEYS.AUTH_TOKEN),
      SecureStore.deleteItemAsync(SECURE_KEYS.API_URL),
      SecureStore.deleteItemAsync(SECURE_KEYS.DID),
      SecureStore.deleteItemAsync(SECURE_KEYS.COOP_ID),
    ]);
  },

  /**
   * Check if user has stored credentials
   */
  async hasCredentials(): Promise<boolean> {
    const token = await this.getAuthToken();
    return token !== null;
  },

  /**
   * Migrate from AsyncStorage to SecureStore (one-time migration)
   * Call this on app startup to migrate old credentials
   */
  async migrateFromAsyncStorage(): Promise<boolean> {
    try {
      // Check if migration already done
      const migrationDone = await AsyncStorage.getItem('icn_migration_to_secure_done');
      if (migrationDone === 'true') {
        return false; // Already migrated
      }

      // Get old credentials from AsyncStorage
      const oldToken = await AsyncStorage.getItem('auth_token');
      const oldApiUrl = await AsyncStorage.getItem('api_url');

      // If there are old credentials, migrate them
      if (oldToken) {
        await this.setAuthToken(oldToken);
        await AsyncStorage.removeItem('auth_token');
      }
      if (oldApiUrl) {
        await this.setApiUrl(oldApiUrl);
        await AsyncStorage.removeItem('api_url');
      }

      // Mark migration as complete
      await AsyncStorage.setItem('icn_migration_to_secure_done', 'true');

      return oldToken !== null; // Return true if we migrated credentials
    } catch (error) {
      console.error('Migration from AsyncStorage failed:', error);
      return false;
    }
  },
};

/**
 * Non-sensitive storage for preferences
 */
export const PreferenceStorage = {
  async setOnboardingComplete(complete: boolean): Promise<void> {
    await AsyncStorage.setItem(ASYNC_KEYS.ONBOARDING_COMPLETE, complete ? 'true' : 'false');
  },

  async isOnboardingComplete(): Promise<boolean> {
    const value = await AsyncStorage.getItem(ASYNC_KEYS.ONBOARDING_COMPLETE);
    return value === 'true';
  },

  async setTheme(theme: 'light' | 'dark' | 'system'): Promise<void> {
    await AsyncStorage.setItem(ASYNC_KEYS.THEME_PREFERENCE, theme);
  },

  async getTheme(): Promise<'light' | 'dark' | 'system'> {
    const value = await AsyncStorage.getItem(ASYNC_KEYS.THEME_PREFERENCE);
    return (value as 'light' | 'dark' | 'system') || 'system';
  },
};

export default SecureStorage;
