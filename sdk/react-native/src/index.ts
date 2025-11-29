/**
 * ICN React Native SDK
 *
 * Mobile-first SDK for the InterCooperative Network.
 *
 * @example
 * ```typescript
 * import {
 *   createMobileClient,
 *   createWallet,
 *   useAuth,
 *   useBalance,
 * } from '@icn/react-native';
 *
 * // Create wallet with secure storage
 * const wallet = createWallet(secureStorage);
 * await wallet.generateKeyPair();
 *
 * // Create client
 * const client = createMobileClient({
 *   baseUrl: 'https://icn.mycoop.org',
 *   wallet,
 *   storage: secureStorage,
 * });
 *
 * // Initialize (loads persisted auth)
 * await client.initialize();
 *
 * // Use in React components
 * function App() {
 *   const { isAuthenticated, login } = useAuth(client);
 *   const { balance } = useBalance(client, 'my-coop', did);
 *
 *   return (
 *     <View>
 *       <Text>Balance: {balance?.balance}</Text>
 *     </View>
 *   );
 * }
 * ```
 *
 * @packageDocumentation
 */

// Client
export { ICNMobileClient, createMobileClient } from './client';

// Wallet
export { ICNWalletImpl, createWallet } from './wallet';

// QR Code utilities
export {
  generatePaymentQR,
  parsePaymentQR,
  isPaymentQR,
  generateReceiveQR,
} from './qr';

// React Hooks
export {
  useAuth,
  useRealtime,
  useEvent,
  useBalance,
  useCoop,
  useProposals,
  useDomains,
  usePayment,
} from './hooks';

// Types
export * from './types';

// Re-export core SDK types for convenience
export {
  ICNClient,
  ICNError,
  createClient,
} from '@icn/client';
