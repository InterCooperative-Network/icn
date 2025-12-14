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

// Queue Manager
export { QueueManager } from './queue-manager';

// Error utilities
export { parseError, createError, isNetworkError, isAuthError } from './error-utils';

// Wallet
export { ICNWalletImpl, createWallet } from './wallet';

// QR Code utilities
export {
  // Universal QR parsing
  parseQR,
  detectQRType,
  isICNQR,
  // Payment QR
  generatePaymentQR,
  parsePaymentQR,
  isPaymentQR,
  generateReceiveQR,
  // Contact QR
  generateContactQR,
  parseContactQR,
  isContactQR,
  // Join/Coop QR
  generateJoinQR,
  parseJoinQR,
  isJoinQR,
} from './qr';
export type { QRType, ContactQRData, JoinQRData, ParsedQRData } from './qr';

// React Hooks
export {
  useAuth,
  useRealtime,
  useEvent,
  useBalance,
  useTransactions,
  useCoop,
  useProposals,
  useDomains,
  usePayment,
  useMemberProfile,
  useNetworkState,
  useQueue,
  useTrustScore,
  useTrustNetwork,
  useTrustAttestation,
  SimplePaymentRequest,
} from './hooks';

// Types
export * from './types';

// SDIS Types
export * from './sdis-types';

// SDIS QR Code utilities
export {
  generateSdisQR,
  parseSdisQR,
  isSdisQR,
  isIcnQR,
  getIcnQRType,
  formatTimeRemaining,
  isProofExpired,
  formatVerificationResult,
} from './sdis-qr';
export type { ParsedSdisQR } from './sdis-qr';

// SDIS React Hooks
export {
  useSdisProof,
  useSdisVerifier,
  useSdisHealth,
  useSdisHistory,
  useSdisProofWithHistory,
  useSdisVerifierWithHistory,
} from './sdis-hooks';
export type { HistoryEntry } from './sdis-hooks';

// Steward Types
export * from './steward-types';

// Steward React Hooks
export {
  usePendingEnrollments,
  useVouch,
  useReject,
  useStewardStats,
  useVouchHistory,
  useEnrollmentDetail,
} from './steward-hooks';

// Re-export core SDK types for convenience
export {
  ICNClient,
  ICNError,
  createClient,
} from '@icn/client';
