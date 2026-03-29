//! Gateway-layer jurisdiction authority helpers.
//!
//! Provides the two canonical authority gates used across membership and steward
//! handlers.  Both gates resolve the caller's DID to a commons holder, then
//! check a specific `MembershipCapability` within a given jurisdiction.
//!
//! ## Authority levels
//!
//! | Level | Capability | Used for |
//! |-------|-----------|----------|
//! | Office-holding | `HoldOffice` | Governance mutations (promote, suspend, grant capability, …) |
//! | Member-standing | `Vote` | Member-rights actions (dispute filing, …) |
//!
//! Neither level implies global elevated status.  Both are scoped to the
//! exact jurisdiction supplied by the caller — there is no cross-jurisdiction
//! propagation.

use crate::commons_mgr::CommonsManager;
use crate::error::{GatewayError, Result};
use icn_identity::{Did, JurisdictionId, MembershipCapability, MembershipStatus};

/// Require that `caller_did` holds the `HoldOffice` capability in `jurisdiction`.
///
/// Used to gate jurisdiction-governance mutations: membership promotion/suspension,
/// capability grants, role changes, steward status updates, and similar operations
/// that require a legitimately delegated cooperative office in the target domain.
///
/// Returns `Err(GatewayError::AuthorizationFailed)` when the caller is not a
/// commons holder or lacks `HoldOffice` in the given jurisdiction.
pub(crate) async fn require_office_in_jurisdiction(
    commons_manager: &CommonsManager,
    caller_did: &Did,
    jurisdiction: &JurisdictionId,
) -> Result<()> {
    let caller_holder = commons_manager
        .get_holder_by_did(caller_did)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| {
            GatewayError::AuthorizationFailed("Caller is not a commons holder".to_string())
        })?;

    let holder_id_hex = hex::encode(caller_holder.holder_id);
    let has_authority = commons_manager
        .member_has_capability(
            &holder_id_hex,
            jurisdiction,
            MembershipCapability::HoldOffice,
        )
        .await
        .unwrap_or(false);

    if !has_authority {
        return Err(GatewayError::AuthorizationFailed(format!(
            "Caller does not hold office in '{jurisdiction}' (HoldOffice capability required)"
        )));
    }

    Ok(())
}

/// Require that `caller_did` is a full member (`Member` status) in `jurisdiction`.
///
/// Used to gate member-rights actions that should be available to any full member
/// of a jurisdiction but not to unenrolled outsiders or provisional candidates.
/// Dispute filing is the canonical example: any member may report a concern, but
/// random authenticated users cannot inflate a steward's dispute count.
///
/// This checks membership status directly rather than a specific capability,
/// because capabilities are not automatically granted during the enrollment flow —
/// status is the authoritative marker that full membership was conferred.
///
/// Returns `Err(GatewayError::AuthorizationFailed)` when the caller is not a
/// commons holder or is not a full member of the given jurisdiction.
pub(crate) async fn require_membership_in_jurisdiction(
    commons_manager: &CommonsManager,
    caller_did: &Did,
    jurisdiction: &JurisdictionId,
) -> Result<()> {
    let caller_holder = commons_manager
        .get_holder_by_did(caller_did)
        .await
        .map_err(|e| GatewayError::InternalError(e.to_string()))?
        .ok_or_else(|| {
            GatewayError::AuthorizationFailed("Caller is not a commons holder".to_string())
        })?;

    let is_full_member = caller_holder
        .get_affiliation(jurisdiction)
        .map(|a| a.membership_status == MembershipStatus::Member)
        .unwrap_or(false);

    if !is_full_member {
        return Err(GatewayError::AuthorizationFailed(format!(
            "Caller is not a full member of '{jurisdiction}' (member standing required)"
        )));
    }

    Ok(())
}
