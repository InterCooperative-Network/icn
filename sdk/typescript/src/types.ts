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
  challenge: string;
  expires_at: number;
}

export interface VerifyRequest {
  did: string;
  signature: string;
  coop_id?: string;
  scopes?: string[];
}

export interface VerifyResponse {
  token: string;
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
  | 'GovernanceVoteCast';

export interface WsEventMessage {
  type: 'Event';
  event_type: CoopEventType;
  payload: unknown;
}

export type WsMessage =
  | WsAuthMessage
  | WsAuthOkMessage
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

export interface ICNClientOptions {
  /** Gateway API base URL */
  baseUrl: string;
  /** JWT token for authentication */
  token?: string;
  /** Request timeout in milliseconds */
  timeout?: number;
  /** Custom fetch implementation */
  fetch?: typeof fetch;
}

export interface SignatureProvider {
  /** Sign a challenge and return the signature as hex string */
  sign(challenge: string): Promise<string>;
}
