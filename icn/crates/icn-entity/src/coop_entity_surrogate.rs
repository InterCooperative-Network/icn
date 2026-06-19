//! Deterministic surrogate `EntityId` proposal for non-mappable `coop_id`s
//! (#2082 lane, PR4).
//!
//! # Why this exists
//!
//! Default cooperative ids are shaped like `coop:<uuid>` (see
//! `icn_coop::Cooperative::new`), and the gateway's `validate_coop_id` also
//! admits uppercase, underscore, and Unicode ids. None of those are valid
//! cooperative [`EntityId`](crate::EntityId) slugs, so
//! [`project_coop_id`](crate::coop_entity_map::project_coop_id) reports them as
//! [`NotMappable`](crate::coop_entity_map::CoopEntityMapError::NotMappable) and
//! the canonical map cannot be populated for them by projection alone.
//!
//! This module proposes a **deterministic surrogate** cooperative `EntityId` for
//! such ids: a stable, valid slug derived from a domain-separated SHA-256 of the
//! original `coop_id`. It is the algorithm a later allocation/backfill PR would
//! use to *write* bindings; this PR only *proposes* — it never binds.
//!
//! # Why deterministic (not random)
//!
//! Cooperative activation binds the map on every node, and the gossip-sync path
//! mirrors that bind (#2104). So the same `coop_id` is bound independently on
//! multiple nodes. A surrogate must therefore be a pure, node-independent
//! function of the `coop_id` — two nodes must compute the identical surrogate
//! with no coordination. Durable-random allocation would diverge across nodes
//! and is out of scope for this lane.
//!
//! # A surrogate is NOT authority, and NOT normalization
//!
//! A surrogate `EntityId` grants **zero** membership, role, capability, mandate,
//! permission, or standing — exactly like every other binding (a mapping is a
//! name binding only). And it is **not** a rewrite of the original `coop_id`:
//! the original is preserved byte-for-byte; the surrogate is a *new generated
//! handle*. `coop_A` is never silently turned into `coop-a`; instead both get
//! distinct surrogate handles derived from their exact bytes.

use crate::coop_entity_map::CoopEntityMapError;
use crate::entity::EntityId;
use sha2::{Digest, Sha256};

/// Domain-separation tag for v1 of the surrogate scheme. Frozen: any change to
/// the derivation is a new version (`v2`) with a new tag, never a silent
/// redefinition of `v1` (which would change already-proposed surrogates).
pub const SURROGATE_DOMAIN_V1: &[u8] = b"icn:coop-entity-surrogate:v1";

/// Visible slug prefix marking a generated surrogate handle (as opposed to a
/// projected or user-chosen slug). The full slug is `{SURROGATE_PREFIX}-{hex}`.
pub const SURROGATE_PREFIX: &str = "coop-legacy";

/// Number of leading digest bytes used for the surrogate fragment. 10 bytes =
/// 80 bits = 20 lowercase-hex characters. The birthday bound (~2^40 surrogates
/// for a 50% collision chance) leaves ample headroom for realistic populations.
const SURROGATE_HASH_BYTES: usize = 10;

/// Lowercase-hex encode a byte slice without pulling in an encoding dependency.
fn to_lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

/// Propose a deterministic surrogate cooperative [`EntityId`] for a `coop_id`.
///
/// The surrogate is `entity:icn:cooperative:coop-legacy-<20 lowercase hex>`,
/// where the 20 hex chars are the first [`SURROGATE_HASH_BYTES`] of
/// `SHA-256(SURROGATE_DOMAIN_V1 || 0x00 || coop_id)`. Deterministic,
/// node-independent, and a pure function of the input bytes.
///
/// This proposes only; it performs no storage and binds nothing. The returned
/// slug is always a valid cooperative slug by construction (prefix starts with a
/// lowercase letter; body is `[a-z0-9-]` with no consecutive hyphens; length is
/// well within 4..=64), so the error arm is an internal-invariant guard that
/// never fires in practice — it exists so the function never panics.
pub fn propose_surrogate_entity_id(coop_id: &str) -> Result<EntityId, CoopEntityMapError> {
    // Domain-separated hash: tag || NUL || original coop_id bytes. The NUL
    // terminator keeps the fixed (NUL-free) tag from blending into the id.
    let mut hasher = Sha256::new();
    hasher.update(SURROGATE_DOMAIN_V1);
    hasher.update([0u8]);
    hasher.update(coop_id.as_bytes());
    let digest = hasher.finalize();

    let fragment = to_lower_hex(&digest[..SURROGATE_HASH_BYTES]);
    let slug = format!("{SURROGATE_PREFIX}-{fragment}");

    // The slug is valid by construction (starts with the lowercase letter 'c';
    // body is `[a-z0-9-]` with non-adjacent hyphens; length 32 ∈ 4..=64), so the
    // error arm is an internal-invariant guard that never fires — we surface it
    // rather than panic to honor the no-unwrap/expect rule.
    EntityId::cooperative(&slug).map_err(|e| {
        CoopEntityMapError::Storage(format!(
            "internal invariant: surrogate slug {slug:?} failed EntityId validation: {e}"
        ))
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    // ------------------------------------------------------------------
    // Golden vector — pins the v1 algorithm. Any drift in domain tag, hash,
    // byte count, encoding, or prefix breaks this.
    // ------------------------------------------------------------------

    #[test]
    fn golden_vector_default_coop_uuid() {
        let surrogate =
            propose_surrogate_entity_id("coop:550e8400-e29b-41d4-a716-446655440000").unwrap();
        assert_eq!(
            surrogate.as_str(),
            "entity:icn:cooperative:coop-legacy-edf6152975301bd80c13"
        );
    }

    // ------------------------------------------------------------------
    // Determinism
    // ------------------------------------------------------------------

    #[test]
    fn deterministic_same_input_same_output() {
        let a = propose_surrogate_entity_id("coop:abc-123").unwrap();
        let b = propose_surrogate_entity_id("coop:abc-123").unwrap();
        assert_eq!(a, b);
    }

    // ------------------------------------------------------------------
    // No silent normalization: distinct inputs -> distinct surrogates.
    // `coop_A` must NEVER collapse onto `coop-a`.
    // ------------------------------------------------------------------

    #[test]
    fn coop_underscore_a_and_coop_dash_a_get_distinct_surrogates() {
        let underscore = propose_surrogate_entity_id("coop_A").unwrap();
        let dash = propose_surrogate_entity_id("coop-a").unwrap();
        assert_ne!(
            underscore, dash,
            "coop_A and coop-a are distinct identities; surrogates must not collapse them"
        );
    }

    #[test]
    fn colon_uuid_and_dash_uuid_do_not_collapse() {
        let colon =
            propose_surrogate_entity_id("coop:550e8400-e29b-41d4-a716-446655440000").unwrap();
        let dash =
            propose_surrogate_entity_id("coop-550e8400-e29b-41d4-a716-446655440000").unwrap();
        assert_ne!(colon, dash);
    }

    #[test]
    fn surrogate_is_not_the_naive_colon_to_dash_rewrite() {
        // The surrogate must be a generated handle, not `coop:<uuid>` with the
        // colon swapped for a hyphen.
        let coop_id = "coop:550e8400-e29b-41d4-a716-446655440000";
        let surrogate = propose_surrogate_entity_id(coop_id).unwrap();
        let naive = coop_id.replace(':', "-");
        assert_ne!(surrogate.identifier(), naive);
    }

    // ------------------------------------------------------------------
    // Validity, prefix, length — including adversarial / non-mappable inputs.
    // ------------------------------------------------------------------

    #[test]
    fn surrogate_is_a_valid_cooperative_entity_id() {
        let surrogate = propose_surrogate_entity_id("coop_A").unwrap();
        assert!(surrogate.is_cooperative());
        // Re-parsing re-runs slug validation: a valid surrogate round-trips.
        assert_eq!(
            EntityId::cooperative(surrogate.identifier()).unwrap(),
            surrogate
        );
    }

    #[test]
    fn surrogate_identifier_has_prefix_and_fixed_length() {
        let surrogate = propose_surrogate_entity_id("food_coop").unwrap();
        let ident = surrogate.identifier();
        assert!(ident.starts_with("coop-legacy-"), "got {ident}");
        // prefix "coop-legacy-" (12) + 20 hex chars = 32.
        assert_eq!(ident.len(), 32);
        let hex = ident.strip_prefix("coop-legacy-").unwrap();
        assert_eq!(hex.len(), 20);
        assert!(hex
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn all_non_mappable_shapes_yield_valid_surrogates() {
        // Every shape the inventory classifies NonMappable must still produce a
        // valid surrogate (the function must not choke on uppercase, underscore,
        // Unicode, short ids, leading digits, consecutive hyphens, or colons).
        for coop_id in [
            "coop:550e8400-e29b-41d4-a716-446655440000",
            "coop_A",
            "food_coop",
            "café-coop",
            "abc",
            "1coop",
            "coop--east",
            "",
            &"x".repeat(4096),
        ] {
            let surrogate = propose_surrogate_entity_id(coop_id)
                .unwrap_or_else(|e| panic!("surrogate failed for {coop_id:?}: {e}"));
            assert!(surrogate.is_cooperative());
            assert!(surrogate.identifier().starts_with("coop-legacy-"));
        }
    }

    #[test]
    fn distinct_inputs_generally_distinct_surrogates() {
        use std::collections::HashSet;
        let inputs = [
            "coop:550e8400-e29b-41d4-a716-446655440000",
            "coop_A",
            "coop-a",
            "food_coop",
            "café-coop",
            "abc",
            "1coop",
            "coop--east",
        ];
        let surrogates: HashSet<String> = inputs
            .iter()
            .map(|c| propose_surrogate_entity_id(c).unwrap().as_str().to_string())
            .collect();
        assert_eq!(
            surrogates.len(),
            inputs.len(),
            "no surrogate collisions expected"
        );
    }

    #[test]
    fn lower_hex_encodes_known_bytes() {
        assert_eq!(to_lower_hex(&[0x00, 0x0f, 0xa5, 0xff]), "000fa5ff");
        assert_eq!(to_lower_hex(&[]), "");
    }
}
