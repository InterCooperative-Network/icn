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

// ============================================================================
// Enrollment Client Types (for enrollees)
// ============================================================================

/**
 * Request to start an enrollment
 */
export interface StartEnrollmentRequest {
  identity_name: string;
  coop_id: string;
}

/**
 * Response from starting an enrollment
 */
export interface StartEnrollmentResponse {
  enrollment_id: string;
  verification_code: string;
  qr_code: string;
  expires_at: string;
}

/**
 * Device proof for Level 1 verification
 */
export interface DeviceProof {
  /** Ephemeral DID generated on the device */
  ephemeral_did: string;
  /** Hex-encoded Ed25519 signature over the enrollment_id */
  signature: string;
}

/**
 * Request for device verification (Level 1)
 */
export interface VerifyDeviceRequest {
  enrollment_id: string;
  device_proof: DeviceProof;
}

/**
 * Response from device verification
 */
export interface VerifyDeviceResponse {
  status: 'verified';
  level: 1;
  message: string;
}

/**
 * Device info for enrollment completion
 */
export interface DeviceInfo {
  device_type: string;
  os: string;
  app_version: string;
}

/**
 * Request to complete enrollment
 */
export interface CompleteEnrollmentRequest {
  enrollment_id: string;
  ephemeral_did: string;
  /** Hex-encoded signature over "complete:{enrollment_id}" */
  ephemeral_signature: string;
  device_info: DeviceInfo;
}

/**
 * Response from completing enrollment
 */
export interface CompleteEnrollmentResponse {
  did: string;
  recovery_codes: string[];
  auth_token: string;
}

/**
 * Parsed enrollment QR code data
 */
export interface EnrollmentQRData {
  type: 'icn-enrollment';
  enrollment_id: string;
  challenge: string;
  gateway_url: string;
}
