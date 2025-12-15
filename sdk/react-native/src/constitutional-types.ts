/**
 * Constitutional Governance Types for ICN React Native SDK
 *
 * Types for amendments and appeals within the ICN commons.
 */

// ============================================================================
// Amendment Types
// ============================================================================

/**
 * Amendment type
 */
export type AmendmentType = 'charter' | 'constitutional' | 'policy' | 'economic' | 'governance';

/**
 * Amendment scope type
 */
export type AmendmentScopeType = 'jurisdiction' | 'federation' | 'network';

/**
 * Amendment status
 */
export type AmendmentStatus =
  | 'draft'
  | 'under_review'
  | 'voting'
  | 'ratifying'
  | 'adopted'
  | 'rejected'
  | 'withdrawn';

/**
 * Change target
 */
export type ChangeTarget =
  | 'governance_rules'
  | 'membership_policy'
  | 'economic_policy'
  | 'dispute_policy'
  | 'commons_rights'
  | 'steward_requirements'
  | 'network_parameters'
  | string; // For custom targets

/**
 * Change type
 */
export type ChangeType = 'add' | 'modify' | 'remove' | 'replace';

/**
 * Amendment change
 */
export interface AmendmentChange {
  target: ChangeTarget;
  change_type: ChangeType;
  description: string;
  old_value?: string;
  new_value: string;
}

/**
 * Amendment summary
 */
export interface AmendmentSummary {
  id: string;
  amendment_type: AmendmentType;
  scope: string;
  status: AmendmentStatus;
  title: string;
  proposer: string;
  created_at: number;
}

/**
 * Amendment detail
 */
export interface Amendment {
  id: string;
  amendment_type: string;
  scope: string;
  status: string;
  title: string;
  description: string;
  proposer: string;
  sponsors: string[];
  changes: AmendmentChangeResponse[];
  ratifications_count: number;
  approvals_count: number;
  created_at: number;
  updated_at: number;
}

export interface AmendmentChangeResponse {
  target: string;
  change_type: string;
  description: string;
  old_value?: string;
  new_value: string;
}

/**
 * Create amendment request
 */
export interface CreateAmendmentRequest {
  amendment_type: AmendmentType;
  scope_type: AmendmentScopeType;
  scope_id?: string;
  title: string;
  description: string;
  charter_id?: string;
  changes: AmendmentChange[];
}

/**
 * Add change request
 */
export interface AddChangeRequest {
  target: ChangeTarget;
  change_type: ChangeType;
  description: string;
  old_value?: string;
  new_value: string;
}

/**
 * Ratify amendment request
 */
export interface RatifyRequest {
  ratifier_id: string;
  ratifier_type: 'commons_holder' | 'jurisdiction' | 'federation';
  approved: boolean;
  comment?: string;
  signature: string;
}

/**
 * Amendment list response
 */
export interface AmendmentListResponse {
  amendments: Amendment[];
  count: number;
}

// ============================================================================
// Appeal Types
// ============================================================================

/**
 * Appeal status
 */
export type AppealStatus =
  | 'filed'
  | 'under_review'
  | 'awaiting_response'
  | 'in_hearing'
  | 'resolved'
  | 'withdrawn';

/**
 * Appeal outcome
 */
export type AppealOutcome = 'upheld' | 'denied' | 'partially_upheld' | 'remanded';

/**
 * Appeal type category
 */
export type AppealTypeCategory =
  | 'revocation'
  | 'suspension'
  | 'governance_decision'
  | 'dispute_resolution'
  | 'membership_denial'
  | 'steward_action'
  | 'other';

/**
 * Appeal scope type
 */
export type AppealScopeType = 'jurisdiction' | 'federation' | 'network';

/**
 * Appeal grounds type
 */
export type AppealGroundsType =
  | 'procedural_error'
  | 'new_evidence'
  | 'exceeded_authority'
  | 'rights_violation'
  | 'factual_error'
  | 'bias'
  | 'disproportionate_penalty'
  | 'other';

/**
 * Appeal remedy type
 */
export type AppealRemedyType = 'reverse' | 'reinstate' | 'modify' | 'compensation' | 'custom' | 'none';

/**
 * Evidence type
 */
export type EvidenceType =
  | 'document'
  | 'transaction'
  | 'communication'
  | 'witness_statement'
  | 'expert_opinion'
  | 'technical'
  | 'other';

/**
 * Response type
 */
export type ResponseType = 'initial_response' | 'reply' | 'comment' | 'question' | 'clarification';

/**
 * Appeal type request
 */
export interface AppealTypeRequest {
  category: AppealTypeCategory;
  revocation_id?: string;
  target_id?: string;
  proposal_id?: string;
  dispute_id?: string;
  steward_did?: string;
  details?: string;
}

/**
 * Appeal grounds request
 */
export interface AppealGroundsRequest {
  ground_type: AppealGroundsType;
  description: string;
}

/**
 * File appeal request
 */
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

/**
 * Add evidence request
 */
export interface AddEvidenceRequest {
  evidence_type: EvidenceType;
  description: string;
  content_hash?: string;
  uri?: string;
}

/**
 * Add response request
 */
export interface AddResponseRequest {
  response_type: ResponseType;
  content: string;
}

/**
 * Resolve appeal request
 */
export interface ResolveAppealRequest {
  outcome: AppealOutcome;
  reason: string;
  remedy?: AppealRemedyType;
  remedy_details?: string;
}

/**
 * Appeal summary
 */
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

/**
 * Appeal list response
 */
export interface AppealListResponse {
  appeals: Appeal[];
  count: number;
}
