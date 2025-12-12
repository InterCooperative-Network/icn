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

export type MemberRole = 'owner' | 'admin' | 'member';

export interface Member {
  did: string;
  role: MemberRole;
  joined_at: number;
}

export interface Member {
  did: string;
  role: MemberRole;
  joined_at: number;
}

export interface MemberProfile {
  did: string;
  name?: string;
  role: 'Steward' | 'Facilitator' | 'Participant';
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

export type CoopEventType =
  | 'PaymentCreated'
  | 'MemberAdded'
  | 'MemberRemoved'
  | 'RoleUpdated'
  | 'SettingsUpdated'
  | 'GovernanceDomainCreated'
  | 'GovernanceProposalCreated'
  | 'GovernanceProposalOpened'
  | 'GovernanceProposalClosed'
  | 'GovernanceVoteCast'
  | 'ComputeTaskSubmitted'
  | 'ComputeTaskClaimed'
  | 'ComputeTaskCompleted'
  | 'ComputeTaskCancelled';

/** GatewayEvent payload with its type tag */
export interface GatewayEventPayload {
  type: CoopEventType;
  [key: string]: unknown;
}

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
}
