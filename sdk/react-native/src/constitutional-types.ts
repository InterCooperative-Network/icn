/**
 * Constitutional Governance Types for ICN React Native SDK
 *
 * Types for amendments and appeals within the ICN commons.
 *
 * The canonical definitions live in `@icn/client` (the core TypeScript SDK).
 * The React Native client extends `ICNClient`, so its overridden amendment /
 * appeal methods must use the same types as the base class. We re-export the
 * canonical types here so existing imports of `./constitutional-types` keep
 * resolving to the single source of truth, and add the small set of
 * RN-specific aliases/types that the core SDK does not expose.
 */

// ============================================================================
// Canonical types (re-exported from the core SDK)
// ============================================================================

export type {
  // Amendments
  AmendmentType,
  AmendmentScopeType,
  AmendmentStatus,
  ChangeTarget,
  ChangeType,
  AmendmentChange,
  Amendment,
  CreateAmendmentRequest,
  AddAmendmentChangeRequest,
  RatifyAmendmentRequest,
  AmendmentListResponse,
  // Appeals
  AppealStatus,
  AppealOutcome,
  AppealTypeCategory,
  AppealScopeType,
  AppealGroundsType,
  AppealRemedyType,
  EvidenceType,
  ResponseType,
  AppealTypeRequest,
  AppealGroundsRequest,
  FileAppealRequest,
  AddEvidenceRequest,
  AddResponseRequest,
  ResolveAppealRequest,
  Appeal,
  AppealListResponse,
} from '@icn/client';

import type {
  AmendmentType,
  AmendmentStatus,
  AddAmendmentChangeRequest,
  RatifyAmendmentRequest,
} from '@icn/client';

// ============================================================================
// RN-specific aliases (core SDK uses longer canonical names)
// ============================================================================

/** Request to add a change to a draft amendment (alias of the canonical type) */
export type AddChangeRequest = AddAmendmentChangeRequest;

/** Request to ratify an amendment (alias of the canonical type) */
export type RatifyRequest = RatifyAmendmentRequest;

// ============================================================================
// RN-specific types (not exposed by the core SDK)
// ============================================================================

/**
 * Amendment summary (lightweight list projection used by mobile screens)
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
 * Raw amendment change as returned by the gateway, where `change_type` is an
 * unvalidated string. Retained for backwards compatibility with mobile code
 * that reads responses without narrowing to {@link ChangeType}.
 */
export interface AmendmentChangeResponse {
  target: string;
  change_type: string;
  description: string;
  old_value?: string;
  new_value: string;
}
