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
