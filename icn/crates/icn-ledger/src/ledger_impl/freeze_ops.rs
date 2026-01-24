//! Freeze operations for the ledger
//!
//! This module contains member freeze operations extracted from the main Ledger
//! implementation. These functions handle:
//! - Freezing member accounts
//! - Unfreezing member accounts
//! - Checking freeze status
//! - Managing freeze records
//! - Cleaning up expired freezes
//!
//! # Member Freezing
//!
//! Freezing is an emergency governance action that blocks all ledger transactions
//! involving a specific member account. This is used for:
//! - Compromised accounts
//! - Suspected fraud or abuse
//! - Pending investigations
//! - Legal compliance requirements
//!
//! Freezes can be temporary (with expiration) or indefinite. They can be initiated
//! through governance proposals with metadata tracking (proposal ID, actor).
//!
//! # Testing
//!
//! Tests for these functions are located in the main `ledger.rs` test module
//! (`crates/icn-ledger/src/ledger.rs`). The tests use the public `Ledger` API
//! which delegates to these functions.
//!
//! Key test functions:
//! - `test_freeze_member_blocks_transactions_as_author`
//! - `test_freeze_member_blocks_transactions_as_participant`
//! - `test_unfreeze_member_allows_transactions`
//! - `test_freeze_with_metadata`
//! - `test_freeze_preserves_balance`

use crate::freeze::FrozenMember;
use crate::ledger::Ledger;
use icn_identity::Did;
use tracing::info;

/// Freeze a member account
///
/// This is an emergency action that blocks all ledger transactions involving
/// the specified member. Requires super-majority governance approval.
///
/// # Arguments
/// * `ledger` - The ledger instance (mutable to update freeze manager)
/// * `did` - The DID of the member to freeze
/// * `reason` - The reason for freezing (for audit trail)
/// * `duration_seconds` - Optional duration in seconds (None = indefinite)
pub(crate) fn freeze_member(
    ledger: &mut Ledger,
    did: Did,
    reason: String,
    duration_seconds: Option<u64>,
) {
    info!(
        did = %did,
        reason = %reason,
        duration = ?duration_seconds,
        "Freezing member account"
    );
    ledger
        .freeze_manager
        .freeze(did.clone(), reason.clone(), duration_seconds);

    // Emit freeze event
    if let Some(ref emitter) = ledger.event_emitter {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        emitter.emit_member_frozen(&did, reason, None, None, duration_seconds, now);
    }
}

/// Freeze a member with full metadata (for governance integration)
///
/// This variant includes governance-specific metadata like proposal ID
/// and the actor who initiated the freeze.
///
/// # Arguments
/// * `ledger` - The ledger instance (mutable)
/// * `did` - The DID of the member to freeze
/// * `reason` - The reason for freezing
/// * `duration_seconds` - Optional duration in seconds
/// * `proposal_id` - Optional governance proposal ID
/// * `frozen_by` - Optional DID of the actor who initiated the freeze
pub(crate) fn freeze_member_with_metadata(
    ledger: &mut Ledger,
    did: Did,
    reason: String,
    duration_seconds: Option<u64>,
    proposal_id: Option<String>,
    frozen_by: Option<Did>,
) {
    info!(
        did = %did,
        reason = %reason,
        proposal = ?proposal_id,
        "Freezing member account via governance"
    );
    ledger.freeze_manager.freeze_with_metadata(
        did.clone(),
        reason.clone(),
        duration_seconds,
        proposal_id.clone(),
        frozen_by.clone(),
    );

    // Emit freeze event
    if let Some(ref emitter) = ledger.event_emitter {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        emitter.emit_member_frozen(
            &did,
            reason,
            frozen_by.as_ref(),
            proposal_id,
            duration_seconds,
            now,
        );
    }
}

/// Unfreeze a member account
///
/// This is also an emergency action requiring super-majority approval,
/// unless the freeze has expired.
///
/// # Arguments
/// * `ledger` - The ledger instance (mutable)
/// * `did` - The DID of the member to unfreeze
/// * `reason` - The reason for unfreezing (for audit trail)
///
/// # Returns
/// The previous freeze record if the member was frozen, None otherwise
pub(crate) fn unfreeze_member(
    ledger: &mut Ledger,
    did: &Did,
    reason: String,
) -> Option<FrozenMember> {
    info!(
        did = %did,
        reason = %reason,
        "Unfreezing member account"
    );
    let result = ledger.freeze_manager.unfreeze(did, reason.clone());

    // Emit unfreeze event if member was unfrozen
    if result.is_some() {
        if let Some(ref emitter) = ledger.event_emitter {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            emitter.emit_member_unfrozen(did, reason, None, None, now);
        }
    }

    result
}

/// Unfreeze a member with full metadata (for governance integration)
///
/// This variant includes governance-specific metadata.
///
/// # Arguments
/// * `ledger` - The ledger instance (mutable)
/// * `did` - The DID of the member to unfreeze
/// * `reason` - The reason for unfreezing
/// * `proposal_id` - Optional governance proposal ID
/// * `unfrozen_by` - Optional DID of the actor who initiated the unfreeze
///
/// # Returns
/// The previous freeze record if the member was frozen, None otherwise
pub(crate) fn unfreeze_member_with_metadata(
    ledger: &mut Ledger,
    did: &Did,
    reason: String,
    proposal_id: Option<String>,
    unfrozen_by: Option<Did>,
) -> Option<FrozenMember> {
    info!(
        did = %did,
        reason = %reason,
        proposal = ?proposal_id,
        "Unfreezing member account via governance"
    );
    let result = ledger.freeze_manager.unfreeze_with_metadata(
        did,
        reason.clone(),
        proposal_id.clone(),
        unfrozen_by.clone(),
    );

    // Emit unfreeze event if member was unfrozen
    if result.is_some() {
        if let Some(ref emitter) = ledger.event_emitter {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);
            emitter.emit_member_unfrozen(did, reason, unfrozen_by.as_ref(), proposal_id, now);
        }
    }

    result
}

/// Check if a member is currently frozen
///
/// # Arguments
/// * `ledger` - The ledger instance (mutable to allow cleanup)
/// * `did` - The DID to check
///
/// # Returns
/// `true` if the member is frozen, `false` otherwise
pub(crate) fn is_member_frozen(ledger: &mut Ledger, did: &Did) -> bool {
    ledger.freeze_manager.is_frozen(did)
}

/// Get the freeze record for a member if frozen
///
/// # Arguments
/// * `ledger` - The ledger instance (mutable)
/// * `did` - The DID to query
///
/// # Returns
/// A reference to the freeze record if the member is frozen, None otherwise
pub(crate) fn get_freeze_record<'a>(ledger: &'a mut Ledger, did: &Did) -> Option<&'a FrozenMember> {
    ledger.freeze_manager.get_frozen(did)
}

/// List all currently frozen members
///
/// # Arguments
/// * `ledger` - The ledger instance (mutable)
///
/// # Returns
/// Vector of references to all current freeze records
pub(crate) fn list_frozen_members(ledger: &mut Ledger) -> Vec<&FrozenMember> {
    ledger.freeze_manager.list_frozen()
}

/// Get count of frozen members
///
/// # Arguments
/// * `ledger` - The ledger instance (mutable)
///
/// # Returns
/// The number of currently frozen members
pub(crate) fn frozen_member_count(ledger: &mut Ledger) -> usize {
    ledger.freeze_manager.frozen_count()
}

/// Clean up expired freezes
///
/// Removes freeze records for members whose freeze duration has expired.
///
/// # Arguments
/// * `ledger` - The ledger instance (mutable)
///
/// # Returns
/// The number of expired freezes that were removed
pub(crate) fn cleanup_expired_freezes(ledger: &mut Ledger) -> usize {
    ledger.freeze_manager.cleanup_expired()
}
