/**
 * CoopWallet Configuration
 *
 * Configure the gateway URL and other settings here.
 */

// Gateway URL - Change this based on your deployment:
// - Development (local): 'http://localhost:8080'
// - Homelab staging:     'http://10.8.10.40:30080'
// - Production:          'https://api.icn.zone'
export const GATEWAY_URL = 'http://10.8.10.40:30080';

// App configuration
export const APP_CONFIG = {
  // Timeout for API requests in milliseconds
  requestTimeout: 30000,

  // Enable debug logging
  debug: __DEV__,

  // App name displayed in various places
  appName: 'CoopWallet',

  // Version for display
  version: '1.0.0',
};
