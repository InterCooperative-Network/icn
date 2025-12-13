/**
 * SDIS Steward Types
 *
 * Type definitions for steward dashboard operations.
 */

/**
 * Enrollment session state
 */
export interface Enrollment {
  enrollment_id: string;
  identity_name: string;
  coop_id: string;
  level: 0 | 1 | 2;
  status:
    | 'pending_device_verification'
    | 'pending_steward_vouch'
    | 'ready_for_completion'
    | 'rejected';
  has_steward_vouch: boolean;
  rejected: boolean;
  rejection_reason: string | null;
  rejected_at: string | null;
  created_at: string;
  expires_at: string;
}

/**
 * Vouch history record
 */
export interface VouchRecord {
  enrollment_id: string;
  identity_name: string;
  coop_id: string;
  vouch_statement: string | null;
  vouched_at: string;
  steward_did: string | null;
}

/**
 * Steward statistics
 */
export interface StewardStats {
  total_vouches: number;
  monthly_vouches: number;
  total_rejections: number;
  reputation_score: number;
  avg_response_hours: number;
}

/**
 * Response from GET /pending
 */
export interface PendingEnrollmentsResponse {
  pending_count: number;
  enrollments: Enrollment[];
}

/**
 * Response from POST /vouch/{id}
 */
export interface VouchResponse {
  status: 'vouched';
  enrollment_id: string;
  level: number;
  message: string;
}

/**
 * Response from POST /reject/{id}
 */
export interface RejectResponse {
  status: 'rejected';
  enrollment_id: string;
  reason: string;
  message: string;
}

/**
 * Response from GET /steward/history
 */
export interface VouchHistoryResponse {
  vouches: VouchRecord[];
  total: number;
  limit: number;
  offset: number;
}

/**
 * Filter options for pending enrollments
 */
export interface PendingEnrollmentsFilter {
  coop_id?: string;
  level?: number;
}
