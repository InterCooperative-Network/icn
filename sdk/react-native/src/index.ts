/**
 * ICN React Native SDK
 *
 * Mobile-first SDK for the InterCooperative Network.
 *
 * @example
 * ```typescript
 * import {
 *   createMobileClient,
 *   createKeyring,
 *   useAuth,
 *   useBalance,
 * } from '@icn/react-native';
 *
 * // Create a Device Keyring with secure storage
 * const keyring = createKeyring(secureStorage);
 * await keyring.generateKeyPair();
 *
 * // Create client
 * const client = createMobileClient({
 *   baseUrl: 'https://icn.mycoop.org',
 *   keyring, // canonical option; the legacy `wallet` option is still accepted
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

// Device Keyring — local key custody + signing (holds no value).
// Canonical: ICNKeyringImpl / createKeyring. Legacy (retained): ICNWalletImpl / createWallet.
// Only the legacy factory createWallet is @deprecated; ICNWalletImpl shares the class symbol with ICNKeyringImpl and is not deprecated.
export {
  ICNWalletImpl,
  createWallet,
  ICNKeyringImpl,
  createKeyring,
} from './wallet';

// Hybrid Post-Quantum Device Keyring (Ed25519 + ML-DSA-65).
// Canonical: HybridKeyring / createHybridKeyring. Legacy (retained): HybridWallet / createHybridWallet.
// Only the legacy factory createHybridWallet is @deprecated; HybridWallet shares the class symbol with HybridKeyring and is not deprecated.
export {
  HybridWallet,
  createHybridWallet,
  getHybridCryptoInfo,
  HybridKeyring,
  createHybridKeyring,
} from './hybrid-wallet';
export type { HybridKeyPairInfo, HybridSignOptions } from './hybrid-wallet';

// Hybrid Cryptography Utilities
export {
  HybridCrypto,
  ML_DSA_SIZES,
  isHybridCryptoSupported,
} from './hybrid-crypto';
export type {
  HybridKeyPair,
  HybridPublicKey,
  HybridSecretKey,
  HybridSignature,
  HybridCryptoOptions,
} from './hybrid-crypto';

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
  // Enrollment QR
  parseEnrollmentQR,
  isEnrollmentQR,
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

// Membership Types
// Canonical membership types flow through `./types` (re-exported from @icn/client);
// only re-export the RN-specific additions here to avoid duplicate-export ambiguity.
export type {
  BanMemberRequest,
  MemberResponse,
  RevokeMembershipRequest,
  RoleRequest,
} from './membership-types';

// Membership React Hooks
export {
  useMembership,
  useJurisdictionMembers,
  useMembershipAdmin,
  useCapabilities,
  useRoles,
} from './membership-hooks';

// Charter Types
// Canonical charter types flow through `./types` (re-exported from @icn/client);
// only re-export the RN-specific additions here to avoid duplicate-export ambiguity.
export type { UpdateCharterStatusRequest } from './charter-types';

// Charter React Hooks
export {
  useCharter,
  useCharters,
  useCreateCharter,
  useCharterSignature,
  useCharterStatus,
} from './charter-hooks';

// Constitutional Governance Types (Amendments & Appeals)
// Canonical amendment/appeal types flow through `./types` (re-exported from
// @icn/client); only re-export the RN-specific aliases/additions here.
export type {
  AddChangeRequest,
  RatifyRequest,
  AmendmentSummary,
  AmendmentChangeResponse,
} from './constitutional-types';

// Constitutional Governance React Hooks
export {
  // Amendment hooks
  useAmendment,
  useAmendments,
  useAmendmentActions,
  useAmendmentVoting,
  useAmendmentResults,
  // Appeal hooks
  useAppeal,
  useAppeals,
  useAppealActions,
  useAppealAdmin,
} from './constitutional-hooks';

// Constitutional Governance Vote Types
export type {
  AmendmentVoteChoice,
  AmendmentVote,
  AmendmentResults,
} from './constitutional-hooks';

// Device Management Types
export type {
  DeviceCapability,
  DeviceInfo,
  RegisterDeviceRequest,
  RegisterDeviceResponse,
  ListDevicesResponse,
  DeviceAPI,
} from './device-hooks';

// Device Management React Hooks
export {
  createDeviceAPI,
  useDevices,
  useDeviceSigning,
} from './device-hooks';

// Notification Types
export type {
  InAppNotification,
  NotificationListResponse,
  NotificationCountResponse,
  ListNotificationsOptions,
} from './notification-hooks';

// Notification React Hooks
export {
  createNotificationAPI,
  useNotifications,
  useUnreadCount,
} from './notification-hooks';

// Governance Dashboard Types
export type {
  AmendmentsBreakdown,
  AppealsBreakdown,
  ActivityEvent,
  GovernanceDashboard,
  ParticipationStats,
} from './governance-dashboard-hooks';

// Governance Dashboard React Hooks
export {
  createGovernanceDashboardAPI,
  useGovernanceDashboard,
  useGovernanceParticipation,
  useGovernanceOverview,
} from './governance-dashboard-hooks';

// Economic Feature Types
export type {
  PaymentFrequency,
  RecurringStatus,
  RecurringPayment,
  CreateRecurringPaymentRequest,
  EscrowStatus,
  EscrowCondition,
  Escrow,
  CreateEscrowRequest,
  BudgetPeriod,
  BudgetStatus,
  Budget,
  CreateBudgetRequest,
} from './economic-hooks';

// Economic Feature React Hooks
export {
  createEconomicAPI,
  useRecurringPayments,
  useEscrow,
  useBudgets,
  useBudget,
} from './economic-hooks';

// Re-export core SDK types for convenience
export {
  ICNClient,
  ICNError,
  createClient,
} from '@icn/client';
