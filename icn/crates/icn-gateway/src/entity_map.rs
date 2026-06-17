//! LEGACY, FALLBACK-ONLY coop_id -> EntityId projection.
//!
//! ⚠️ This is **not** the canonical `coop_id <-> EntityId` mapping. It exists only
//! so RFC-0018 observe-mode can attempt a best-effort target resolution for legacy
//! treasuries that have no stored `entity_id` yet. The canonical, persisted,
//! reversible mapping/backfill is a tracked follow-up (see ADR-0035 and
//! RFC-0018). Do not reuse this as if it were the blessed mapping — that would
//! recreate the lossy-normalization / collision risk it deliberately avoids.
//!
//! Safety property: it **rejects** non-mappable ids; it never *normalizes* one id
//! into another. `validate_coop_id` permits uppercase, underscores, Unicode, and
//! length 1, none of which are valid `EntityId` slugs — so e.g. `coop_A` is
//! rejected outright and is *never* silently rewritten to `coop-a` (which would
//! collapse two distinct coops onto one identity).

use icn_entity::EntityId;

/// Error returned when a flat `coop_id` cannot be projected to an `EntityId`
/// slug under the (intentionally strict) `EntityId` slug rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LegacyCoopIdMapError {
    /// The id is not a valid `EntityId` cooperative slug (carries the reason).
    NotMappable(String),
}

/// Best-effort, FALLBACK-ONLY projection of a flat `coop_id` to a cooperative
/// `EntityId`. Returns `Err(LegacyCoopIdMapError::NotMappable)` for any id that is
/// not already a valid `EntityId` slug — it does not normalize.
///
/// This delegates to `EntityId::cooperative`, whose `validate_slug` is the single
/// source of truth for slug rules (lowercase ASCII + digits + hyphens, length
/// 4-64, must start with a letter, no consecutive hyphens).
pub(crate) fn legacy_coop_id_to_entity_id_fallback(
    coop_id: &str,
) -> Result<EntityId, LegacyCoopIdMapError> {
    EntityId::cooperative(coop_id).map_err(|e| LegacyCoopIdMapError::NotMappable(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn is_mappable(coop_id: &str) -> bool {
        legacy_coop_id_to_entity_id_fallback(coop_id).is_ok()
    }

    #[test]
    fn rejects_uppercase() {
        assert!(!is_mappable("coop_A"));
        assert!(!is_mappable("FoodCoop"));
    }

    #[test]
    fn rejects_underscore() {
        assert!(!is_mappable("food_coop"));
    }

    #[test]
    fn rejects_non_ascii_unicode() {
        assert!(!is_mappable("café-coop"));
    }

    #[test]
    fn rejects_too_short() {
        assert!(!is_mappable("abc")); // 3 chars < 4
        assert!(!is_mappable("ab"));
    }

    #[test]
    fn rejects_leading_digit() {
        assert!(!is_mappable("1coop"));
    }

    #[test]
    fn rejects_consecutive_hyphens() {
        assert!(!is_mappable("coop--east"));
    }

    #[test]
    fn maps_valid_slug_on_its_own_merit() {
        // `coop-a` is already a valid EntityId slug, so it maps — because it is
        // valid, NOT because some other id was normalized into it.
        let mapped = legacy_coop_id_to_entity_id_fallback("coop-a").unwrap();
        assert_eq!(mapped, EntityId::cooperative("coop-a").unwrap());

        let mapped2 = legacy_coop_id_to_entity_id_fallback("food-coop").unwrap();
        assert_eq!(mapped2, EntityId::cooperative("food-coop").unwrap());
    }

    #[test]
    fn coop_a_is_rejected_never_normalized_to_coop_dash_a() {
        // The collision guard: `coop_A` (uppercase + underscore) is rejected
        // outright. It must NOT be rewritten into the distinct, valid `coop-a`.
        let result = legacy_coop_id_to_entity_id_fallback("coop_A");
        assert!(result.is_err());
        // And it certainly must not equal the `coop-a` identity.
        assert_ne!(result.ok(), Some(EntityId::cooperative("coop-a").unwrap()));
    }
}
