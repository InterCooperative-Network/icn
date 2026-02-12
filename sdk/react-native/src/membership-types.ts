/**
 * Membership Types for ICN React Native SDK
 *
 * Types for membership management within jurisdictions.
 */

/**
 * Membership status in a jurisdiction
 */
export type MembershipStatus =
  | 'candidate'
  | 'provisional'
  | 'member'
  | 'suspended'
  | 'exited'
  | 'banned';

/**
 * Membership capabilities
 */
export type MembershipCapability =
  | 'vote'
  | 'propose'
  | 'transact'
  | 'hold_office'
  | 'access_private'
  | 'sponsor';

/**
 * Apply for membership request
 */
export interface ApplyMembershipRequest {
  jurisdiction_id: string;
  capabilities_requested?: MembershipCapability[];
}

/**
 * Membership action request (approve, promote, suspend, etc.)
 */
export interface MembershipActionRequest {
  holder_id: string;
  jurisdiction_id: string;
}

/**
 * Capability grant/revoke request
 */
export interface CapabilityRequest {
  holder_id: string;
  jurisdiction_id: string;
  capability: MembershipCapability;
}

/**
 * Role request
 */
export interface RoleRequest {
  holder_id: string;
  jurisdiction_id: string;
  role: string;
}

/**
 * Ban member request
 */
export interface BanMemberRequest {
  holder_id: string;
  jurisdiction_id: string;
  reason: string;
  evidence_summary?: string;
}

/**
 * Revoke membership request
 */
export interface RevokeMembershipRequest {
  holder_id: string;
  jurisdiction_id: string;
  reason: string;
  appeal_window_days?: number;
  policy_ref?: string;
}

/**
 * Member response
 */
export interface MemberResponse {
  holder_id: string;
  holder_did: string;
  jurisdiction_id: string;
  status: MembershipStatus;
  capabilities: MembershipCapability[];
  roles: string[];
  joined_at: number;
  expires_at?: number;
}

/**
 * Member list response
 */
export interface MemberListResponse {
  members: MemberResponse[];
  total: number;
}

/**
 * Membership action response
 */
export interface MembershipActionResponse {
  status: string;
  holder_id: string;
  jurisdiction_id: string;
  new_status?: MembershipStatus;
  revocation_id?: string;
  appeal_deadline?: number;
  effective_at?: number;
}

/**
 * Capability check response
 */
export interface CapabilityCheckResponse {
  holder_id: string;
  jurisdiction_id: string;
  capability: MembershipCapability;
  has_capability: boolean;
}
