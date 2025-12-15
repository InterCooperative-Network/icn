/**
 * Charter Types for ICN React Native SDK
 *
 * Types for commons charter management.
 */

/**
 * Organization type
 */
export type OrgType = 'cooperative' | 'community' | 'federation';

/**
 * Charter status
 */
export type CharterStatus = 'draft' | 'active' | 'suspended' | 'dissolved';

/**
 * Charter summary (for list views)
 */
export interface CharterSummary {
  charter_id: string;
  domain_id: string;
  name: string;
  org_type: OrgType;
  status: CharterStatus;
  founder_count: number;
  created_at: number;
}

/**
 * Founder signature info
 */
export interface Founder {
  did: string;
  role?: string;
  timestamp: number;
}

/**
 * Charter detail
 */
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

/**
 * Create charter request
 */
export interface CreateCharterRequest {
  domain_id: string;
  name: string;
  description?: string;
  org_type: OrgType;
  bootstrap_endpoints?: string[];
}

/**
 * Sign charter request
 */
export interface SignCharterRequest {
  role?: string;
  signature: string;
}

/**
 * Update charter status request
 */
export interface UpdateCharterStatusRequest {
  status: CharterStatus;
  reason?: string;
}

/**
 * Charter sign response
 */
export interface CharterSignResponse {
  status: string;
  charter_id: string;
  total_founders: number;
  ready_for_activation: boolean;
  founders_needed: number;
}

/**
 * Charter action response
 */
export interface CharterActionResponse {
  status: string;
  charter_id: string;
  new_status?: string;
}
