/**
 * ICN TypeScript SDK Types
 *
 * Type definitions for the ICN Gateway API
 */

// ============================================================================
// Authentication
// ============================================================================

export interface ChallengeRequest {
  did: string;
}

export interface ChallengeResponse {
  nonce: string;
  expires_in: number;
}

export interface VerifyRequest {
  did: string;
  signature: string;
  coop_id?: string;
  scopes?: string[];
}

export interface VerifyResponse {
  token: string;
  expires_in: number;
  /** Computed expires_at timestamp (added by SDK) */
  expires_at: number;
}

export interface TokenClaims {
  sub: string;  // DID
  coop_id: string;
  scopes: string[];
  exp: number;
  iat: number;
}

// ============================================================================
// Cooperatives
// ============================================================================

export interface Cooperative {
  id: string;
  name: string;
  owner: string;
  created_at: number;
  settings: CoopSettings;
  treasury_did?: string;
}

export interface CoopSettings {
  description?: string;
  currency?: string;
  credit_limit?: number;
  [key: string]: unknown;
}

export interface CreateCoopRequest {
  id: string;
  name: string;
  settings?: CoopSettings;
}

export interface UpdateCoopRequest {
  name?: string;
  settings?: CoopSettings;
}

/**
 * Public statistics for a cooperative (no authentication required)
 */
export interface CoopStatsResponse {
  /** Cooperative ID */
  coop_id: string;
  /** Cooperative name */
  name: string;
  /** Total number of members */
  total_members: number;
  /** Total hours exchanged across all transactions */
  total_hours_exchanged: number;
  /** Total number of transactions */
  transaction_count: number;
  /** Average transaction size in hours */
  avg_transaction_size: number;
  /** Cooperative creation timestamp */
  created_at: number;
}

/**
 * Canonical role type used across the ICN system.
 * These map to the governance roles in cooperatives.
 */
export type CanonicalRole = 'steward' | 'facilitator' | 'participant';

/**
 * Legacy role type for backwards compatibility.
 * @deprecated Use CanonicalRole instead
 */
export type MemberRole = 'owner' | 'admin' | 'member';

/**
 * Maps legacy roles to canonical roles
 */
export const ROLE_MAP: Record<MemberRole, CanonicalRole> = {
  owner: 'steward',
  admin: 'facilitator',
  member: 'participant',
};

export interface Member {
  did: string;
  role: MemberRole;
  joined_at: number;
}

export interface MemberProfile {
  did: string;
  name?: string;
  /** Role in canonical format */
  role: CanonicalRole;
  joined_at: number;
  balance: number;
  transaction_count: number;
  trust_score?: number;
}

export interface AddMemberRequest {
  did: string;
  role: MemberRole;
}

export interface UpdateMemberRequest {
  role: MemberRole;
}

// ============================================================================
// Ledger
// ============================================================================

export interface Balance {
  did: string;
  balance: number;
  currency: string;
}

export interface PaymentRequest {
  from: string;
  to: string;
  amount: number;
  currency: string;
  memo?: string;
}

export interface PaymentResponse {
  id: string;
  from: string;
  to: string;
  amount: number;
  currency: string;
  memo?: string;
  timestamp: number;
}

// ============================================================================
// Cross-Currency Payments
// ============================================================================

/**
 * Request for a cross-currency payment where sender pays in one currency
 * and recipient receives in another.
 */
export interface CrossPaymentRequest {
  /** Sender DID */
  from: string;
  /** Recipient DID */
  to: string;
  /** Amount to send in source currency */
  amount: number;
  /** Source currency (what sender pays) */
  from_currency: string;
  /** Target currency (what recipient receives) */
  to_currency: string;
  /** Maximum target amount for slippage protection (optional) */
  max_target_amount?: number;
  /** Optional memo */
  memo?: string;
}

/**
 * Response from a cross-currency payment
 */
export interface CrossPaymentResponse {
  /** Transaction hash */
  hash: string;
  /** Sender DID */
  from: string;
  /** Recipient DID */
  to: string;
  /** Amount debited from sender */
  source_amount: number;
  /** Source currency */
  from_currency: string;
  /** Gross amount before fees */
  gross_target_amount: number;
  /** Fee amount deducted */
  fee_amount: number;
  /** Net amount credited to recipient */
  net_target_amount: number;
  /** Target currency */
  to_currency: string;
  /** Exchange rate used */
  rate_used: number;
  /** When the rate was fetched */
  rate_timestamp: number;
  /** Sources that provided the rate */
  rate_sources: string[];
}

/**
 * Request for a cross-currency payment quote (preview without execution)
 */
export interface CrossPaymentQuoteRequest {
  /** Amount to send in source currency */
  amount: number;
  /** Source currency (what sender pays) */
  from_currency: string;
  /** Target currency (what recipient receives) */
  to_currency: string;
}

/**
 * Quote for a cross-currency payment
 */
export interface CrossPaymentQuote {
  /** Amount that would be debited from sender */
  source_amount: number;
  /** Source currency */
  from_currency: string;
  /** Gross amount before fees */
  gross_target_amount: number;
  /** Fee amount that would be deducted */
  fee_amount: number;
  /** Net amount that would be credited to recipient */
  net_target_amount: number;
  /** Target currency */
  to_currency: string;
  /** Exchange rate */
  rate: number;
  /** When the rate was fetched */
  rate_timestamp: number;
  /** Sources that provided the rate */
  rate_sources: string[];
  /** When this quote expires (Unix seconds) */
  valid_until: number;
  /** Whether the rate is considered stale */
  is_stale: boolean;
}

export interface Transaction {
  id: string;
  from: string;
  to: string;
  amount: number;
  currency: string;
  memo?: string;
  timestamp: number;
}

export interface TransactionHistory {
  transactions: Transaction[];
  total: number;
  offset: number;
  limit: number;
}

// ============================================================================
// Governance
// ============================================================================

export interface GovernanceDomain {
  id: string;
  name: string;
  members: string[];
  created_at: number;
}

export interface CreateDomainRequest {
  domain_id: string;
  name: string;
  members: string[];
}

export type ProposalKind = 'text' | 'budget' | 'membership' | 'config_change';
export type ProposalState = 'draft' | 'open' | 'closed';
export type VoteChoice = 'for' | 'against' | 'abstain';

export interface Proposal {
  id: string;
  domain_id: string;
  title: string;
  description?: string;
  kind: ProposalKind;
  state: ProposalState;
  created_by: string;
  created_at: number;
  opened_at?: number;
  closed_at?: number;
}

export interface CreateProposalRequest {
  domain_id: string;
  title: string;
  description?: string;
  kind: ProposalKind;
}

export interface Vote {
  voter: string;
  choice: VoteChoice;
  timestamp: number;
}

export interface VoteTally {
  proposal_id: string;
  votes_for: number;
  votes_against: number;
  votes_abstain: number;
  total_votes: number;
  votes: Vote[];
}

export interface CastVoteRequest {
  choice: VoteChoice;
}

export interface ProposalOutcome {
  accepted: boolean;
  votes_for: number;
  votes_against: number;
  votes_abstain: number;
  quorum_met: boolean;
  approval_met: boolean;
}

// ============================================================================
// Vote Delegation
// ============================================================================

/**
 * Scope of a vote delegation.
 * - "blanket": Delegate can vote on behalf of delegator for all proposals
 * - "domain:<id>": Delegate can only vote in the specified governance domain
 * - "proposal:<id>": Delegate can only vote on the specified proposal
 */
export type DelegationScope = 'blanket' | `domain:${string}` | `proposal:${string}`;

/**
 * Request to create a new vote delegation
 */
export interface CreateDelegationRequest {
  /** DID of the delegate (who receives voting power) */
  delegate: string;
  /** Scope of delegation */
  scope: DelegationScope;
  /** Optional expiry timestamp (Unix seconds) */
  expires_at?: number;
}

/**
 * Response containing delegation details
 */
export interface DelegationResponse {
  /** Unique delegation ID */
  id: string;
  /** DID of the delegator (who gave up voting power) */
  delegator: string;
  /** DID of the delegate (who received voting power) */
  delegate: string;
  /** Scope of delegation */
  scope: string;
  /** Creation timestamp (Unix seconds) */
  created_at: number;
  /** Optional expiry timestamp (Unix seconds) */
  expires_at?: number;
  /** Revocation timestamp (Unix seconds), if revoked */
  revoked_at?: number;
  /** Whether the delegation is currently active */
  is_active: boolean;
}

/**
 * Response containing delegations given and received by a user
 */
export interface DelegationListResponse {
  /** Delegations given by the caller */
  given: DelegationResponse[];
  /** Delegations received by the caller */
  received: DelegationResponse[];
}

// ============================================================================
// Compute
// ============================================================================

/** Code type for compute tasks */
export type CodeType = 'ccl' | 'wasm';

export interface SubmitTaskRequest {
  /** Optional task ID (auto-generated if not provided) */
  task_id?: string;
  /** CCL contract JSON (for code_type: ccl) */
  code?: string;
  /** WASM bytecode as base64 string (for code_type: wasm) */
  wasm_bytes?: string;
  /** Blake3 hash of a previously uploaded WASM module (hex string) */
  wasm_hash?: string;
  /** Code type: "ccl" (default) or "wasm" */
  code_type?: CodeType;
  /** Input arguments */
  inputs?: Record<string, unknown>;
  /** Maximum fuel for execution (default 10000) */
  fuel_limit?: number;
  /** Task priority: "low", "normal", "high", or "critical" (default: "normal") */
  priority?: string;
  /** Deadline in milliseconds from now */
  deadline_ms?: number;
  /** Payment rate per 1000 fuel */
  payment_rate?: number;
  /** Payment currency (default credits) */
  payment_currency?: string;
}

export interface SubmitTaskResponse {
  task_id: string;
  task_hash: string;
}

export type ComputeTaskState = 'pending' | 'claimed' | 'completed' | 'failed' | 'cancelled';
export type ComputeOutcome = 'success' | 'failed' | 'out_of_fuel' | 'timeout' | 'cancelled';

export interface ComputeResult {
  outcome: ComputeOutcome;
  output?: unknown;
  error?: string;
  fuel_used: number;
  duration_ms: number;
}

export interface ComputeTaskStatus {
  task_hash: string;
  status: ComputeTaskState;
  executor?: string;
  result?: ComputeResult;
}

export interface CancelTaskRequest {
  /** Cancellation reason (optional) */
  reason?: string;
}

export interface CancelTaskResponse {
  task_hash: string;
  status: string;
  reason: string;
}

// ============================================================================
// WebSocket Events
// ============================================================================

export type WsMessageType =
  | 'Auth'
  | 'AuthOk'
  | 'Event'
  | 'Error'
  | 'Ping'
  | 'Pong';

export interface WsAuthMessage {
  type: 'Auth';
  token: string;
}

export interface WsAuthOkMessage {
  type: 'AuthOk';
  did: string;
  current_seq: number;
}

export interface WsBackfillMessage {
  type: 'Backfill';
  after_seq: number;
}

export interface WsBackfillCompleteMessage {
  type: 'BackfillComplete';
  count: number;
}

export interface WsErrorMessage {
  type: 'Error';
  message: string;
}

export interface WsShutdownMessage {
  type: 'Shutdown';
  reason: string;
  reconnect_after_ms: number | null;
}

// ============================================================================
// Gateway Event Payloads (Discriminated Union)
// ============================================================================

export interface PaymentCreatedEvent {
  type: 'PaymentCreated';
  coop_id: string;
  hash: string;
  from: string;
  to: string;
  amount: number;
  currency: string;
}

export interface CrossPaymentCreatedEvent {
  type: 'CrossPaymentCreated';
  coop_id: string;
  hash: string;
  from: string;
  to: string;
  source_amount: number;
  from_currency: string;
  target_amount: number;
  to_currency: string;
  rate: number;
}

export interface MemberAddedEvent {
  type: 'MemberAdded';
  coop_id: string;
  did: string;
  role: CanonicalRole;
}

export interface MemberRemovedEvent {
  type: 'MemberRemoved';
  coop_id: string;
  did: string;
}

export interface RoleUpdatedEvent {
  type: 'RoleUpdated';
  coop_id: string;
  did: string;
  new_role: CanonicalRole;
}

export interface SettingsUpdatedEvent {
  type: 'SettingsUpdated';
  coop_id: string;
}

export interface GovernanceDomainCreatedEvent {
  type: 'GovernanceDomainCreated';
  domain_id: string;
  name: string;
  creator: string;
}

export interface GovernanceProposalCreatedEvent {
  type: 'GovernanceProposalCreated';
  proposal_id: string;
  domain_id: string;
  proposer: string;
  title: string;
  payload_type: 'text' | 'budget' | 'membership' | 'config_change';
}

export interface GovernanceProposalOpenedEvent {
  type: 'GovernanceProposalOpened';
  proposal_id: string;
  domain_id: string;
  closes_at: number;
}

export interface GovernanceProposalClosedEvent {
  type: 'GovernanceProposalClosed';
  proposal_id: string;
  domain_id: string;
  outcome: 'accepted' | 'rejected' | 'no_quorum';
}

export interface GovernanceVoteCastEvent {
  type: 'GovernanceVoteCast';
  proposal_id: string;
  domain_id: string;
  voter: string;
  choice: 'for' | 'against' | 'abstain';
}

export interface ComputeTaskSubmittedEvent {
  type: 'ComputeTaskSubmitted';
  task_id: string;
  task_hash: string;
  submitter: string;
  fuel_limit: number;
}

export interface ComputeTaskClaimedEvent {
  type: 'ComputeTaskClaimed';
  task_hash: string;
  executor: string;
}

export interface ComputeTaskCompletedEvent {
  type: 'ComputeTaskCompleted';
  task_hash: string;
  executor: string;
  outcome: 'success' | 'failed' | 'out_of_fuel' | 'timeout';
  fuel_used: number;
  duration_ms: number;
}

export interface ComputeTaskCancelledEvent {
  type: 'ComputeTaskCancelled';
  task_hash: string;
  submitter: string;
  reason: string;
}

export interface ShutdownEvent {
  type: 'Shutdown';
}

/**
 * Discriminated union of all gateway event payloads.
 * Use the `type` field to narrow the type in event handlers.
 */
export type GatewayEventPayload =
  | PaymentCreatedEvent
  | CrossPaymentCreatedEvent
  | MemberAddedEvent
  | MemberRemovedEvent
  | RoleUpdatedEvent
  | SettingsUpdatedEvent
  | GovernanceDomainCreatedEvent
  | GovernanceProposalCreatedEvent
  | GovernanceProposalOpenedEvent
  | GovernanceProposalClosedEvent
  | GovernanceVoteCastEvent
  | ComputeTaskSubmittedEvent
  | ComputeTaskClaimedEvent
  | ComputeTaskCompletedEvent
  | ComputeTaskCancelledEvent
  | ShutdownEvent;

/**
 * Extract the type string from GatewayEventPayload
 */
export type CoopEventType = GatewayEventPayload['type'];

export interface WsEventMessage {
  type: 'Event';
  seq: number;
  event: GatewayEventPayload;
}

export type WsMessage =
  | WsAuthMessage
  | WsAuthOkMessage
  | WsBackfillMessage
  | WsBackfillCompleteMessage
  | WsEventMessage
  | WsErrorMessage
  | WsShutdownMessage
  | { type: 'Ping' }
  | { type: 'Pong' };

// ============================================================================
// Health & Status
// ============================================================================

export interface HealthResponse {
  status: string;
  network_peers?: number;
  gossip_entries?: number;
  ledger_entries?: number;
}

// ============================================================================
// Errors
// ============================================================================

export interface ApiError {
  error: string;
  code?: string;
  details?: unknown;
}

/**
 * Error codes for ICN API errors
 */
export enum ErrorCode {
  // Authentication errors
  TOKEN_EXPIRED = 'TOKEN_EXPIRED',
  INVALID_CREDENTIALS = 'INVALID_CREDENTIALS',
  INVALID_TOKEN = 'INVALID_TOKEN',
  AUTHENTICATION_REQUIRED = 'AUTHENTICATION_REQUIRED',

  // Authorization errors
  INSUFFICIENT_PERMISSIONS = 'INSUFFICIENT_PERMISSIONS',
  FORBIDDEN = 'FORBIDDEN',

  // Validation errors
  VALIDATION_FAILED = 'VALIDATION_FAILED',
  INVALID_DID = 'INVALID_DID',
  INVALID_SCOPE = 'INVALID_SCOPE',
  INVALID_EXPIRATION = 'INVALID_EXPIRATION',
  INVALID_REQUEST = 'INVALID_REQUEST',

  // Resource errors
  NOT_FOUND = 'NOT_FOUND',
  ALREADY_EXISTS = 'ALREADY_EXISTS',
  CONFLICT = 'CONFLICT',

  // Rate limiting
  RATE_LIMITED = 'RATE_LIMITED',

  // Network errors
  NETWORK_ERROR = 'NETWORK_ERROR',
  TIMEOUT = 'TIMEOUT',
  CONNECTION_FAILED = 'CONNECTION_FAILED',

  // Server errors
  INTERNAL_ERROR = 'INTERNAL_ERROR',
  SERVICE_UNAVAILABLE = 'SERVICE_UNAVAILABLE',
}

/**
 * Base error class for all ICN SDK errors
 */
export class ICNError extends Error {
  public readonly statusCode: number;
  public readonly code?: string;
  public readonly details?: unknown;

  constructor(message: string, statusCode: number, code?: string, details?: unknown) {
    super(message);
    this.name = 'ICNError';
    this.statusCode = statusCode;
    this.code = code;
    this.details = details;
  }

  /**
   * Check if this error is retryable
   */
  isRetryable(): boolean {
    return (
      this.statusCode >= 500 ||
      this.statusCode === 408 ||
      this.statusCode === 429 ||
      this.code === ErrorCode.TIMEOUT ||
      this.code === ErrorCode.CONNECTION_FAILED
    );
  }
}

// ============================================================================
// Authentication Errors (401)
// ============================================================================

/**
 * Authentication error - credentials invalid or missing
 */
export class AuthenticationError extends ICNError {
  constructor(message: string, code?: string, details?: unknown) {
    super(message, 401, code || ErrorCode.AUTHENTICATION_REQUIRED, details);
    this.name = 'AuthenticationError';
  }
}

/**
 * Token has expired and needs to be refreshed
 */
export class TokenExpiredError extends AuthenticationError {
  constructor(message = 'Token has expired', details?: unknown) {
    super(message, ErrorCode.TOKEN_EXPIRED, details);
    this.name = 'TokenExpiredError';
  }
}

/**
 * Invalid credentials provided
 */
export class InvalidCredentialsError extends AuthenticationError {
  constructor(message = 'Invalid credentials', details?: unknown) {
    super(message, ErrorCode.INVALID_CREDENTIALS, details);
    this.name = 'InvalidCredentialsError';
  }
}

// ============================================================================
// Authorization Errors (403)
// ============================================================================

/**
 * Authorization error - insufficient permissions
 */
export class AuthorizationError extends ICNError {
  constructor(message: string, code?: string, details?: unknown) {
    super(message, 403, code || ErrorCode.FORBIDDEN, details);
    this.name = 'AuthorizationError';
  }
}

/**
 * User lacks required permissions for the operation
 */
export class InsufficientPermissionsError extends AuthorizationError {
  public readonly requiredPermissions?: string[];

  constructor(message = 'Insufficient permissions', requiredPermissions?: string[], details?: unknown) {
    super(message, ErrorCode.INSUFFICIENT_PERMISSIONS, details);
    this.name = 'InsufficientPermissionsError';
    this.requiredPermissions = requiredPermissions;
  }
}

// ============================================================================
// Validation Errors (400)
// ============================================================================

/**
 * Validation error with field-level details
 */
export class ValidationError extends ICNError {
  public readonly fields: Record<string, string[]>;

  constructor(message: string, fields: Record<string, string[]> = {}, details?: unknown) {
    super(message, 400, ErrorCode.VALIDATION_FAILED, details);
    this.name = 'ValidationError';
    this.fields = fields;
  }

  /**
   * Get all error messages for a specific field
   */
  getFieldErrors(field: string): string[] {
    return this.fields[field] || [];
  }

  /**
   * Check if a specific field has errors
   */
  hasFieldError(field: string): boolean {
    return field in this.fields && this.fields[field].length > 0;
  }
}

// ============================================================================
// Resource Errors (404, 409)
// ============================================================================

/**
 * Resource not found
 */
export class NotFoundError extends ICNError {
  public readonly resourceType?: string;
  public readonly resourceId?: string;

  constructor(message: string, resourceType?: string, resourceId?: string, details?: unknown) {
    super(message, 404, ErrorCode.NOT_FOUND, details);
    this.name = 'NotFoundError';
    this.resourceType = resourceType;
    this.resourceId = resourceId;
  }
}

/**
 * Resource already exists (conflict)
 */
export class ConflictError extends ICNError {
  constructor(message: string, code?: string, details?: unknown) {
    super(message, 409, code || ErrorCode.CONFLICT, details);
    this.name = 'ConflictError';
  }
}

// ============================================================================
// Rate Limiting Errors (429)
// ============================================================================

/**
 * Rate limit exceeded
 */
export class RateLimitError extends ICNError {
  public readonly retryAfter?: number;
  public readonly limit?: number;
  public readonly remaining?: number;

  constructor(
    message = 'Rate limit exceeded',
    retryAfter?: number,
    limit?: number,
    remaining?: number,
    details?: unknown
  ) {
    super(message, 429, ErrorCode.RATE_LIMITED, details);
    this.name = 'RateLimitError';
    this.retryAfter = retryAfter;
    this.limit = limit;
    this.remaining = remaining;
  }

  isRetryable(): boolean {
    return true;
  }
}

// ============================================================================
// Network Errors (0, 408)
// ============================================================================

/**
 * Network-level error
 */
export class NetworkError extends ICNError {
  constructor(message: string, code?: string, details?: unknown, statusCode = 0) {
    super(message, statusCode, code || ErrorCode.NETWORK_ERROR, details);
    this.name = 'NetworkError';
  }

  isRetryable(): boolean {
    return true;
  }
}

/**
 * Request timeout
 */
export class TimeoutError extends NetworkError {
  public readonly timeoutMs?: number;

  constructor(message = 'Request timeout', timeoutMs?: number, details?: unknown) {
    super(message, ErrorCode.TIMEOUT, details, 408);
    this.name = 'TimeoutError';
    this.timeoutMs = timeoutMs;
  }
}

/**
 * Connection failed
 */
export class ConnectionError extends NetworkError {
  constructor(message = 'Connection failed', details?: unknown) {
    super(message, ErrorCode.CONNECTION_FAILED, details);
    this.name = 'ConnectionError';
  }
}

// ============================================================================
// Server Errors (500+)
// ============================================================================

/**
 * Internal server error
 */
export class ServerError extends ICNError {
  constructor(message: string, statusCode = 500, code?: string, details?: unknown) {
    super(message, statusCode, code || ErrorCode.INTERNAL_ERROR, details);
    this.name = 'ServerError';
  }

  isRetryable(): boolean {
    return true;
  }
}

// ============================================================================
// Error Factory
// ============================================================================

/**
 * Create a typed error from an API response
 */
export function createErrorFromResponse(
  statusCode: number,
  message: string,
  code?: string,
  details?: unknown
): ICNError {
  switch (statusCode) {
    case 400:
      if (code === ErrorCode.VALIDATION_FAILED) {
        const fields = (details as { fields?: Record<string, string[]> })?.fields || {};
        return new ValidationError(message, fields, details);
      }
      return new ICNError(message, statusCode, code, details);

    case 401:
      if (code === ErrorCode.TOKEN_EXPIRED) {
        return new TokenExpiredError(message, details);
      }
      if (code === ErrorCode.INVALID_CREDENTIALS) {
        return new InvalidCredentialsError(message, details);
      }
      return new AuthenticationError(message, code, details);

    case 403:
      if (code === ErrorCode.INSUFFICIENT_PERMISSIONS) {
        const permissions = (details as { required_permissions?: string[] })?.required_permissions;
        return new InsufficientPermissionsError(message, permissions, details);
      }
      return new AuthorizationError(message, code, details);

    case 404:
      const resourceType = (details as { resource_type?: string })?.resource_type;
      const resourceId = (details as { resource_id?: string })?.resource_id;
      return new NotFoundError(message, resourceType, resourceId, details);

    case 408:
      return new TimeoutError(message, undefined, details);

    case 409:
      return new ConflictError(message, code, details);

    case 429:
      const retryAfter = (details as { retry_after?: number })?.retry_after;
      const limit = (details as { limit?: number })?.limit;
      const remaining = (details as { remaining?: number })?.remaining;
      return new RateLimitError(message, retryAfter, limit, remaining, details);

    default:
      if (statusCode >= 500) {
        return new ServerError(message, statusCode, code, details);
      }
      return new ICNError(message, statusCode, code, details);
  }
}

// ============================================================================
// Client Options
// ============================================================================

export interface RetryOptions {
  /** Maximum number of retry attempts (default: 3) */
  maxRetries?: number;
  /** Initial delay in milliseconds (default: 1000) */
  initialDelayMs?: number;
  /** Maximum delay in milliseconds (default: 10000) */
  maxDelayMs?: number;
  /** Exponential backoff multiplier (default: 2) */
  backoffMultiplier?: number;
  /** Jitter factor (0-1) to randomize delay (default: 0.1) */
  jitterFactor?: number;
  /** HTTP status codes that should trigger a retry (default: [408, 429, 500, 502, 503, 504]) */
  retryableStatuses?: number[];
}

export interface ICNClientOptions {
  /** Gateway API base URL */
  baseUrl: string;
  /** JWT token for authentication */
  token?: string;
  /** Request timeout in milliseconds (default: 30000) */
  timeout?: number;
  /** Custom fetch implementation */
  fetch?: typeof fetch;
  /** Retry configuration for failed requests */
  retry?: RetryOptions;
  /** Enable automatic token refresh (default: false) */
  autoRefresh?: boolean;
  /** Refresh token before it expires (in seconds, default: 60) */
  refreshBeforeExpiry?: number;
}

export interface SignatureProvider {
  /** Sign a challenge and return the signature as hex string */
  sign(challenge: string): Promise<string>;
}

export interface WebSocketOptions {
  /** Auto-reconnect on disconnect (default: true) */
  autoReconnect?: boolean;
  /** Maximum reconnection attempts (default: 10) */
  maxReconnectAttempts?: number;
  /** Initial reconnect delay in ms (default: 1000) */
  reconnectDelayMs?: number;
  /** Maximum reconnect delay in ms (default: 30000) */
  maxReconnectDelayMs?: number;
  /** Auto-request backfill after reconnect (default: true) */
  autoBackfill?: boolean;
  /** Detect gaps in sequence numbers and request backfill (default: true) */
  gapDetection?: boolean;
}

// ============================================================================
// Commons Evolution: Charter
// ============================================================================

export type OrgType = 'cooperative' | 'community' | 'federation';
export type CharterStatus = 'draft' | 'active' | 'suspended' | 'dissolved';

export interface CharterSummary {
  charter_id: string;
  domain_id: string;
  name: string;
  org_type: OrgType;
  status: CharterStatus;
  founder_count: number;
  created_at: number;
}

export interface Founder {
  did: string;
  role?: string;
  timestamp: number;
}

export interface Charter {
  charter_id: string;
  domain_id: string;
  name: string;
  description?: string;
  org_type: OrgType;
  status: CharterStatus;
  founders: Founder[];
  created_at: number;
  bootstrap_endpoints: string[];
}

export interface CreateCharterRequest {
  domain_id: string;
  name: string;
  description?: string;
  org_type: OrgType;
  bootstrap_endpoints?: string[];
}

export interface SignCharterRequest {
  role?: string;
  signature: string;
}

export interface CharterSignResponse {
  status: string;
  charter_id: string;
  total_founders: number;
  ready_for_activation: boolean;
  founders_needed: number;
}

export interface CharterActionResponse {
  status: string;
  charter_id: string;
  new_status?: string;
}

// ============================================================================
// Commons Evolution: Membership
// ============================================================================

export type MembershipStatus =
  | 'candidate'
  | 'provisional'
  | 'member'
  | 'suspended'
  | 'exited'
  | 'banned';

export type MembershipCapability =
  | 'vote'
  | 'propose'
  | 'transact'
  | 'hold_office'
  | 'access_private'
  | 'sponsor';

export interface ApplyMembershipRequest {
  jurisdiction_id: string;
  capabilities_requested?: MembershipCapability[];
}

export interface MembershipActionRequest {
  holder_id: string;
  jurisdiction_id: string;
}

export interface CapabilityRequest {
  holder_id: string;
  jurisdiction_id: string;
  capability: MembershipCapability;
}

export interface RoleActionRequest {
  holder_id: string;
  jurisdiction_id: string;
  role: string;
}

export interface CommonsHolder {
  holder_id: string;
  holder_did: string;
  jurisdiction_id: string;
  status: MembershipStatus;
  capabilities: MembershipCapability[];
  roles: string[];
  joined_at: number;
  expires_at?: number;
}

export interface MemberListResponse {
  members: CommonsHolder[];
  total: number;
}

export interface MembershipActionResponse {
  status: string;
  holder_id: string;
  jurisdiction_id: string;
  new_status?: MembershipStatus;
  revocation_id?: string;
  appeal_deadline?: number;
  effective_at?: number;
}

export interface CapabilityCheckResponse {
  holder_id: string;
  jurisdiction_id: string;
  capability: MembershipCapability;
  has_capability: boolean;
}

// ============================================================================
// Commons Evolution: Amendments
// ============================================================================

export type AmendmentType = 'charter' | 'constitutional' | 'policy' | 'economic' | 'governance';
export type AmendmentScopeType = 'jurisdiction' | 'federation' | 'network';
export type AmendmentStatus =
  | 'draft'
  | 'under_review'
  | 'voting'
  | 'ratifying'
  | 'adopted'
  | 'rejected'
  | 'withdrawn';

export type ChangeTarget =
  | 'governance_rules'
  | 'membership_policy'
  | 'economic_policy'
  | 'dispute_policy'
  | 'commons_rights'
  | 'steward_requirements'
  | 'network_parameters'
  | string;

export type ChangeType = 'add' | 'modify' | 'remove' | 'replace';

export interface AmendmentChange {
  target: ChangeTarget;
  change_type: ChangeType;
  description: string;
  old_value?: string;
  new_value: string;
}

export interface Amendment {
  id: string;
  amendment_type: string;
  scope: string;
  status: string;
  title: string;
  description: string;
  proposer: string;
  sponsors: string[];
  changes: AmendmentChange[];
  ratifications_count: number;
  approvals_count: number;
  created_at: number;
  updated_at: number;
}

export interface CreateAmendmentRequest {
  amendment_type: AmendmentType;
  scope_type: AmendmentScopeType;
  scope_id?: string;
  title: string;
  description: string;
  charter_id?: string;
  changes: AmendmentChange[];
}

export interface AddAmendmentChangeRequest {
  target: ChangeTarget;
  change_type: ChangeType;
  description: string;
  old_value?: string;
  new_value: string;
}

export interface RatifyAmendmentRequest {
  ratifier_id: string;
  ratifier_type: 'commons_holder' | 'jurisdiction' | 'federation';
  approved: boolean;
  comment?: string;
  signature: string;
}

export interface AmendmentListResponse {
  amendments: Amendment[];
  count: number;
}

// ============================================================================
// Commons Evolution: Amendment Voting (UI-friendly)
// ============================================================================

/** Vote choice for amendments */
export type AmendmentVoteChoice = 'approve' | 'reject' | 'abstain';

/** Request to cast a vote on an amendment */
export interface CastAmendmentVoteRequest {
  vote: AmendmentVoteChoice;
  comment?: string;
  weight?: number;
}

/** Individual vote response */
export interface AmendmentVoteResponse {
  voter: string;
  vote: string;
  weight?: number;
  comment?: string;
  timestamp: number;
  voted_at: string;
}

/** Vote results for an amendment */
export interface AmendmentVoteResults {
  amendment_id: string;
  title: string;
  status: string;
  total_votes: number;
  approve_count: number;
  reject_count: number;
  abstain_count: number;
  approval_percentage: number;
  quorum_required: number;
  quorum_achieved: boolean;
  approval_threshold: number;
  has_passed: boolean;
  voting_ends_at?: number;
  time_remaining_secs?: number;
  votes?: AmendmentVoteResponse[];
}

/** User's vote status on an amendment */
export interface MyAmendmentVoteResponse {
  amendment_id: string;
  has_voted: boolean;
  vote?: AmendmentVoteResponse;
  can_vote: boolean;
  reason?: string;
}

/** List of votes on an amendment */
export interface ListAmendmentVotesResponse {
  amendment_id: string;
  total_votes: number;
  votes: AmendmentVoteResponse[];
}

/** Response from casting a vote */
export interface CastAmendmentVoteResponse {
  success: boolean;
  vote: string;
  total_votes: number;
}

// ============================================================================
// Commons Evolution: Appeals
// ============================================================================

export type AppealStatus =
  | 'filed'
  | 'under_review'
  | 'awaiting_response'
  | 'in_hearing'
  | 'resolved'
  | 'withdrawn';

export type AppealOutcome = 'upheld' | 'denied' | 'partially_upheld' | 'remanded';

export type AppealTypeCategory =
  | 'revocation'
  | 'suspension'
  | 'governance_decision'
  | 'dispute_resolution'
  | 'membership_denial'
  | 'steward_action'
  | 'other';

export type AppealScopeType = 'jurisdiction' | 'federation' | 'network';

export type AppealGroundsType =
  | 'procedural_error'
  | 'new_evidence'
  | 'exceeded_authority'
  | 'rights_violation'
  | 'factual_error'
  | 'bias'
  | 'disproportionate_penalty'
  | 'other';

export type AppealRemedyType = 'reverse' | 'reinstate' | 'modify' | 'compensation' | 'custom' | 'none';

export type EvidenceType =
  | 'document'
  | 'transaction'
  | 'communication'
  | 'witness_statement'
  | 'expert_opinion'
  | 'technical'
  | 'other';

export type ResponseType = 'initial_response' | 'reply' | 'comment' | 'question' | 'clarification';

export interface AppealTypeRequest {
  category: AppealTypeCategory;
  revocation_id?: string;
  target_id?: string;
  proposal_id?: string;
  dispute_id?: string;
  steward_did?: string;
  details?: string;
}

export interface AppealGroundsRequest {
  ground_type: AppealGroundsType;
  description: string;
}

export interface FileAppealRequest {
  appeal_type: AppealTypeRequest;
  scope_type: AppealScopeType;
  scope_id?: string;
  grounds: AppealGroundsRequest[];
  statement: string;
  requested_remedy: AppealRemedyType;
  remedy_details?: string;
  respondent_did?: string;
  original_decision_ref?: string;
}

export interface AddEvidenceRequest {
  evidence_type: EvidenceType;
  description: string;
  content_hash?: string;
  uri?: string;
}

export interface AddResponseRequest {
  response_type: ResponseType;
  content: string;
}

export interface ResolveAppealRequest {
  outcome: AppealOutcome;
  reason: string;
  remedy?: AppealRemedyType;
  remedy_details?: string;
}

export interface Appeal {
  id: string;
  appeal_type: string;
  scope: string;
  status: string;
  appellant: string;
  respondent?: string;
  grounds: string[];
  statement: string;
  requested_remedy: string;
  evidence_count: number;
  responses_count: number;
  created_at: number;
  updated_at: number;
}

export interface AppealListResponse {
  appeals: Appeal[];
  count: number;
}

// ============================================================================
// Identity Resolution
// ============================================================================

/** Response from DID resolution */
export interface DidResolutionResponse {
  /** The resolved DID string */
  did: string;
  /** Whether the DID is valid and parseable */
  valid: boolean;
  /** The cooperative ID this DID belongs to (if known) */
  coop_id?: string;
  /** Whether the DID has attestations in this cooperative */
  has_attestations: boolean;
  /** Number of valid attestations (if include_attestations was requested) */
  attestation_count?: number;
}

/** Identity service health check response */
export interface IdentityHealthResponse {
  status: string;
  service: string;
}

// ============================================================================
// Device Management
// ============================================================================

/** Device capability types */
export type DeviceCapability =
  | 'sign'
  | 'add_device'
  | 'revoke_device'
  | 'rotate_key'
  | 'recover'
  | 'encrypt';

/** Device information */
export interface DeviceInfo {
  /** Unique device identifier */
  id: string;
  /** Human-readable label */
  label: string;
  /** Key type (e.g., "Ed25519") */
  key_type: string;
  /** Capabilities granted to this device */
  capabilities: string[];
  /** Timestamp when device was added */
  added_at: number;
  /** Whether the device has been revoked */
  revoked: boolean;
}

/** Request to register a new device */
export interface RegisterDeviceRequest {
  /** Unique device identifier (e.g., "phone-1", "laptop-work") */
  device_id: string;
  /** Human-readable label */
  label: string;
  /** Ed25519 public key (hex-encoded) */
  public_key: string;
  /** Optional X25519 encryption public key (hex-encoded) */
  encryption_public_key?: string;
  /** Capabilities to grant: "sign", "add_device", "revoke_device", "rotate_key", "encrypt" */
  capabilities: string[];
  /** Device ID of the device signing this request (must have AddDevice capability) */
  signing_device_id: string;
  /** Hex-encoded signature of the registration request */
  signature: string;
}

/** Response from device registration */
export interface RegisterDeviceResponse {
  /** The registered device info */
  device: DeviceInfo;
  /** Success message */
  message: string;
}

/** Response from listing devices */
export interface ListDevicesResponse {
  /** List of devices */
  devices: DeviceInfo[];
  /** Total count */
  total: number;
}

/** Request to revoke a device */
export interface RevokeDeviceRequest {
  /** Device ID of the device signing this request (must have RevokeDevice capability) */
  signing_device_id: string;
  /** Hex-encoded signature */
  signature: string;
}

/** Response from device revocation */
export interface RevokeDeviceResponse {
  /** Success message */
  message: string;
  /** The revoked device ID */
  device_id: string;
}

// ============================================================================
// Exchange Rate Oracle
// ============================================================================

/**
 * Exchange rate between two currencies
 */
export interface ExchangeRateResponse {
  /** Source currency code */
  from_currency: string;
  /** Target currency code */
  to_currency: string;
  /** Exchange rate (target per 1 source) */
  rate: number;
  /** Inverse rate (source per 1 target) */
  inverse_rate: number;
  /** Sources that contributed to this rate */
  sources: string[];
  /** Whether the rate is stale (older than staleness threshold) */
  is_stale: boolean;
  /** When the rate was last aggregated (Unix timestamp) */
  aggregated_at: number;
  /** Remaining TTL in seconds */
  remaining_ttl: number;
}

/**
 * Request to convert an amount between currencies
 */
export interface ConvertAmountRequest {
  /** Amount to convert (in smallest unit) */
  amount: number;
  /** Source currency code */
  from_currency: string;
  /** Target currency code */
  to_currency: string;
}

/**
 * Response from currency conversion
 */
export interface ConvertAmountResponse {
  /** Original amount */
  original_amount: number;
  /** Converted amount */
  converted_amount: number;
  /** Source currency */
  from_currency: string;
  /** Target currency */
  to_currency: string;
  /** Rate used for conversion */
  rate_used: number;
}

/**
 * Information about a rate source
 */
export interface RateSourceInfo {
  /** Source identifier */
  source_id: string;
  /** Human-readable name */
  name: string;
  /** Priority (lower = higher priority) */
  priority: number;
  /** Whether the source is healthy */
  is_healthy: boolean;
}

/**
 * Response listing rate sources
 */
export interface ListRateSourcesResponse {
  /** Available rate sources */
  sources: RateSourceInfo[];
}

/**
 * Request to set a manual exchange rate
 */
export interface SetManualRateRequest {
  /** Source currency code */
  from_currency: string;
  /** Target currency code */
  to_currency: string;
  /** Exchange rate (target per 1 source) */
  rate: number;
  /** Optional note/reason for the rate */
  note?: string;
}

/**
 * Response from setting a manual rate
 */
export interface SetManualRateResponse {
  /** Currency pair key (from:to) */
  pair: string;
  /** Rate that was set */
  rate: number;
  /** Who set the rate */
  set_by: string;
  /** When the rate was set (Unix timestamp) */
  set_at: number;
}

// ============================================================================
// WASM Module Management
// ============================================================================

/** Metadata for a deployed WASM module */
export interface WasmModuleMetadata {
  /** Blake3 hash of the WASM bytes (hex-encoded) */
  hash: string;
  /** Human-readable name */
  name: string;
  /** Module version */
  version: string;
  /** Size in bytes */
  size: number;
  /** DID of the deployer */
  deployed_by: string;
  /** Deployment timestamp (Unix seconds) */
  deployed_at: number;
}

/** Request to upload a WASM module */
export interface UploadWasmRequest {
  /** WASM bytecode as base64 string */
  wasm_bytes: string;
  /** Human-readable module name */
  name: string;
  /** Module version */
  version: string;
}

/** Response from uploading a WASM module */
export interface UploadWasmResponse {
  /** Blake3 hash of the WASM bytes (hex-encoded) */
  hash: string;
  /** Size in bytes */
  size: number;
  /** Module name */
  name: string;
}

/** Paginated list of WASM modules */
export interface WasmModuleListResponse {
  /** List of module metadata */
  modules: WasmModuleMetadata[];
  /** Total number of modules */
  total: number;
  /** Offset used */
  offset: number;
  /** Limit used */
  limit: number;
}

// ============================================================================
// Treasury
// ============================================================================

/** Treasury status response */
export interface TreasuryStatus {
  /** Cooperative ID */
  coop_id: string;
  /** Treasury DID */
  treasury_did: string;
  /** Current balance */
  balance: number;
  /** Currency */
  currency: string;
  /** Number of pending spend proposals */
  pending_proposals: number;
}

/** Treasury balance response */
export interface TreasuryBalance {
  /** Cooperative ID */
  coop_id: string;
  /** Current balance */
  balance: number;
  /** Currency */
  currency: string;
}

/** Request to propose a treasury spend */
export interface ProposeTreasurySpendRequest {
  /** Amount to spend */
  amount: number;
  /** Recipient DID */
  recipient: string;
  /** Currency */
  currency: string;
  /** Memo/reason for the spend */
  memo: string;
}

/** Response from proposing a treasury spend */
export interface ProposeTreasurySpendResponse {
  /** ID of the created governance proposal */
  proposal_id: string;
  /** Domain ID for the governance proposal */
  domain_id: string;
  /** Status message */
  status: string;
}

// ============================================================================
// Service Discovery / Naming
// ============================================================================

/** Scope level for service visibility */
export type ScopeLevel = 'local' | 'org' | 'federation' | 'commons';

/** Service type descriptor */
export interface ServiceTypeDescriptor {
  /** Service type name (e.g., "ledger", "compute") */
  name: string;
  /** Service version */
  version: string;
}

/** Network endpoint address */
export interface ServiceAddress {
  /** Protocol (e.g., "https", "grpc") */
  protocol: string;
  /** Hostname */
  host: string;
  /** Port number */
  port: number;
}

/** Service endpoint as returned by discovery */
export interface ServiceEndpointInfo {
  /** Unique service ID */
  service_id: string;
  /** Provider DID */
  provider: string;
  /** Service type */
  service_type: ServiceTypeDescriptor;
  /** Network endpoints */
  addresses: ServiceAddress[];
  /** Capabilities offered */
  capabilities: string[];
  /** Scope visibility */
  scope: ScopeLevel;
  /** TTL in seconds */
  ttl_secs: number;
  /** Created timestamp (Unix seconds) */
  created_at: number;
}

/** Request to announce a service */
export interface AnnounceServiceRequest {
  /** Unique service ID */
  service_id: string;
  /** Service type */
  service_type: ServiceTypeDescriptor;
  /** Network endpoints */
  addresses: ServiceAddress[];
  /** Capabilities offered */
  capabilities: string[];
  /** Scope visibility */
  scope: ScopeLevel;
  /** TTL in seconds (default: 3600) */
  ttl_secs?: number;
}

/** Response from announcing a service */
export interface AnnounceServiceResponse {
  /** Service ID that was registered */
  service_id: string;
  /** Status message */
  status: string;
}

/** Request to discover services */
export interface DiscoverServicesRequest {
  /** Scope level to search within */
  scope?: ScopeLevel;
  /** Filter by service type name */
  service_type?: string;
  /** Required capabilities */
  required_capabilities?: string[];
}

/** Response from service discovery */
export interface DiscoverServicesResponse {
  /** Matching service endpoints */
  services: ServiceEndpointInfo[];
  /** Total results */
  total: number;
}
