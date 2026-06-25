//! Fail-closed governed `coop_id → EntityId` resolver seam (A2a).
//!
//! Design: `docs/design/coop-id-entity-resolver.md` (#2187), companion to the
//! entity-aware authorization control map (`docs/design/entity-aware-auth-control-map.md`).
//!
//! This is the **route / observe-side** analogue of the issuance-side
//! [`crate::token_authority::TokenAuthoritySource`] seam. Where that seam answers
//! *"may this subject receive a token binding a typed entity?"*, this one answers
//! *"what trusted [`EntityId`] does a legacy flat `coop_id` denote?"*. Both are
//! fail-closed by construction and are designed to read the same governed,
//! membership-grounded source once it is wired.
//!
//! ## What this slice (A2a) is — and is not
//!
//! This defines only the **seam**: a trait, a by-value result type, and the
//! fail-closed default that resolves nothing. It deliberately does **not**:
//! - consume the `icn-entity` `CoopEntityMap` store (#2082) — per-binding provenance
//!   retrieval is an A2c prerequisite (see the design doc); this slice adds no
//!   dependency on the store,
//! - wire any route, change the flat `require_coop_access` guard, or enforce the
//!   entity-aware `require_entity_access` check anywhere new,
//! - issue positive `entity_id` / `entity_type` token claims.
//!
//! ## Core invariants (shared across the auth migration)
//!
//! - **A mapping is not authority.** Resolving `coop_id → EntityId` says only which
//!   entity an identifier denotes, never what a caller may do. Authority still flows
//!   from memberships, standing, mandates, and decision receipts.
//! - **Fail closed.** Anything other than a trusted, unambiguous resolution is a
//!   first-class [`CoopEntityResolution::Unavailable`] *value* — never an `Err` a
//!   caller might treat as success — and the shipped default resolves nothing.
//! - **Reject, never project.** The default does not fall back to the lossy
//!   `legacy_coop_id_to_entity_id_fallback` projection; an unwired source yields no
//!   identity at all.

use icn_entity::{CoopEntityBinding, CoopEntityBindingProvenance, CoopEntityMap, EntityId};
use std::sync::Arc;

/// Why a [`CoopEntityResolver`] could not return a trusted [`EntityId`].
///
/// Stable, machine-readable reasons (see [`ResolutionUnavailable::label`]) so the
/// outcome can flow into logs/metrics without leaking internals. These enumerate the
/// fail-closed failure modes from the design doc (§5); only a future store-backed,
/// provenance-aware source (A2c) produces most of them. The shipped default
/// ([`UnwiredCoopEntityResolver`]) only ever returns
/// [`ResolutionUnavailable::NoTrustedSourceWired`].
///
/// Note: subject-bound mismatches are intentionally absent here — the A2a seam keys
/// resolution on `coop_id` alone; a subject input (and its mismatch reason) arrives
/// with the observe-mode wiring (A2b) per the design doc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolutionUnavailable {
    /// No trusted resolver source is wired — the fail-closed default for this slice.
    NoTrustedSourceWired,
    /// No binding exists for the `coop_id`.
    Unmapped,
    /// More than one candidate binding, or a forward/reverse conflict.
    Ambiguous,
    /// A binding exists but the target entity is revoked / retired.
    Revoked,
    /// The backing source could not answer (I/O, lock, backend failure).
    SourceUnavailable,
    /// A binding exists but its provenance is stale or cannot be verified.
    UnverifiedProvenance,
    /// The bound entity is not a cooperative (a flat `coop_id` denotes a cooperative).
    EntityTypeMismatch,
}

impl ResolutionUnavailable {
    /// Stable, lowercase label for logs / metrics.
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            ResolutionUnavailable::NoTrustedSourceWired => "no_trusted_source_wired",
            ResolutionUnavailable::Unmapped => "unmapped",
            ResolutionUnavailable::Ambiguous => "ambiguous",
            ResolutionUnavailable::Revoked => "revoked",
            ResolutionUnavailable::SourceUnavailable => "source_unavailable",
            ResolutionUnavailable::UnverifiedProvenance => "unverified_provenance",
            ResolutionUnavailable::EntityTypeMismatch => "entity_type_mismatch",
        }
    }
}

/// The outcome of resolving a legacy flat `coop_id` to a typed [`EntityId`].
///
/// Returned **by value** (never a `Result`): an unavailable resolution — including
/// an unwired or unhealthy source mapped to [`CoopEntityResolution::Unavailable`] —
/// must never be confusable with a successful resolution. A resolved binding is a
/// name binding only and confers no authority.
#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use = "a coop->entity resolution must be acted on (resolved vs unavailable)"]
pub enum CoopEntityResolution {
    /// A trusted, typed cooperative [`EntityId`] for the `coop_id`.
    ///
    /// Provenance is intentionally absent in A2a; a provenance-tagged resolution
    /// arrives in A2c, once the store can return per-binding provenance (see the
    /// design doc — provenance is decisive for `Enforce` / `Issue`).
    Resolved {
        /// The cooperative entity the `coop_id` denotes.
        entity_id: EntityId,
    },
    /// No trusted [`EntityId`] could be returned; carries a machine-readable reason.
    Unavailable {
        /// Why resolution failed closed.
        reason: ResolutionUnavailable,
    },
}

impl CoopEntityResolution {
    /// `true` only for [`CoopEntityResolution::Resolved`].
    #[must_use]
    pub fn is_resolved(&self) -> bool {
        matches!(self, CoopEntityResolution::Resolved { .. })
    }

    /// The resolved [`EntityId`], or `None` for any
    /// [`CoopEntityResolution::Unavailable`]. Never fabricates a value.
    #[must_use]
    pub fn resolved_entity_id(&self) -> Option<&EntityId> {
        match self {
            CoopEntityResolution::Resolved { entity_id } => Some(entity_id),
            CoopEntityResolution::Unavailable { .. } => None,
        }
    }
}

/// The trust boundary a caller consults to resolve a legacy flat `coop_id` to a
/// trusted cooperative [`EntityId`].
///
/// The method is `async` because a real source resolves from a persisted store (the
/// same async shape as [`crate::token_authority::TokenAuthoritySource`]); the
/// fail-closed default resolves without I/O. Returning the outcome by value (rather
/// than `Result`) is deliberate — see [`CoopEntityResolution`].
#[async_trait::async_trait]
pub trait CoopEntityResolver: Send + Sync {
    /// Resolve `coop_id` to a trusted cooperative [`EntityId`], or report why not.
    /// Implementations must never project, normalize, or fabricate an identity.
    async fn resolve_coop_entity(&self, coop_id: &str) -> CoopEntityResolution;
}

/// The fail-closed default [`CoopEntityResolver`]: resolves nothing.
///
/// Until a trusted, provenance-aware source is wired (A2c), this is the only
/// resolver the gateway ships. It always returns
/// [`ResolutionUnavailable::NoTrustedSourceWired`] and reads none of its input — no
/// `coop_id`, however well-formed, can produce a [`CoopEntityResolution::Resolved`].
/// It is the resolver-side counterpart of
/// [`crate::token_authority::DenyUntilWired`].
#[derive(Debug, Default, Clone, Copy)]
pub struct UnwiredCoopEntityResolver;

#[async_trait::async_trait]
impl CoopEntityResolver for UnwiredCoopEntityResolver {
    async fn resolve_coop_entity(&self, _coop_id: &str) -> CoopEntityResolution {
        CoopEntityResolution::Unavailable {
            reason: ResolutionUnavailable::NoTrustedSourceWired,
        }
    }
}

/// A trusted, store-backed [`CoopEntityResolver`] that reads the provenance-aware
/// [`CoopEntityMap`] (#2190) and resolves **only** bindings whose recorded
/// provenance is trusted.
///
/// This is the first real (non-`Unwired`) resolver source — the A2c store-backed
/// slice. It is intentionally **read-only and non-enforcing**: it consumes the
/// `coop_id → EntityId` binding plus its [`CoopEntityBindingProvenance`] and maps
/// the outcome to a [`CoopEntityResolution`] *value*. It performs no authorization,
/// denies nothing, changes no route, and is not yet wired into any handler — the
/// observe-mode treasury path keeps using [`UnwiredCoopEntityResolver`].
///
/// ## Fail-closed mapping (the trust boundary)
///
/// | Store read for `coop_id`                       | Resolution                                   |
/// |------------------------------------------------|----------------------------------------------|
/// | binding present, **trusted** provenance        | [`CoopEntityResolution::Resolved`]           |
/// | binding present, [`CoopEntityBindingProvenance::UnknownLegacy`] | `Unavailable(`[`ResolutionUnavailable::UnverifiedProvenance`]`)` |
/// | no binding                                     | `Unavailable(`[`ResolutionUnavailable::Unmapped`]`)`            |
/// | store/backend error                            | `Unavailable(`[`ResolutionUnavailable::SourceUnavailable`]`)`   |
///
/// A resolved binding is a *name binding only* and confers no authority (see the
/// module docs and the `CoopEntityMap` docs). `UnknownLegacy` — a pre-provenance
/// binding, a binding written with no provenance record, or an unknown future
/// variant read by an older binary — is the substrate's fail-closed sentinel and is
/// never trusted here.
pub struct StoreBackedCoopEntityResolver {
    map: Arc<dyn CoopEntityMap + Send + Sync>,
}

impl StoreBackedCoopEntityResolver {
    /// Build a resolver over a shared provenance-aware [`CoopEntityMap`].
    #[must_use]
    pub fn new(map: Arc<dyn CoopEntityMap + Send + Sync>) -> Self {
        Self { map }
    }
}

/// Whether a binding's recorded provenance is trusted enough to resolve a *name*
/// (never authority).
///
/// Exhaustive by construction: a future [`CoopEntityBindingProvenance`] variant
/// forces a compile error here rather than being silently trusted. Only the
/// fail-closed [`CoopEntityBindingProvenance::UnknownLegacy`] sentinel (missing or
/// unverifiable provenance) is untrusted.
fn provenance_is_trusted_for_resolution(provenance: &CoopEntityBindingProvenance) -> bool {
    match provenance {
        CoopEntityBindingProvenance::Activation
        | CoopEntityBindingProvenance::OperatorBackfill
        | CoopEntityBindingProvenance::Surrogate
        | CoopEntityBindingProvenance::GovernanceReceipt { .. } => true,
        CoopEntityBindingProvenance::UnknownLegacy => false,
    }
}

/// Map a present binding to a [`CoopEntityResolution`], gating on provenance.
fn resolution_from_binding(binding: CoopEntityBinding) -> CoopEntityResolution {
    if provenance_is_trusted_for_resolution(&binding.provenance) {
        CoopEntityResolution::Resolved {
            entity_id: binding.entity_id,
        }
    } else {
        CoopEntityResolution::Unavailable {
            reason: ResolutionUnavailable::UnverifiedProvenance,
        }
    }
}

#[async_trait::async_trait]
impl CoopEntityResolver for StoreBackedCoopEntityResolver {
    async fn resolve_coop_entity(&self, coop_id: &str) -> CoopEntityResolution {
        // Read the provenance-aware binding from the #2190 substrate. The store read
        // is synchronous and fail-closed by construction; this resolver performs no
        // authorization, denies nothing, and never fabricates or normalizes an
        // identity. A present binding only resolves if its provenance is trusted
        // (see `resolution_from_binding`); everything else is a first-class
        // `Unavailable` value.
        match self.map.binding_for_coop(coop_id) {
            Ok(Some(binding)) => resolution_from_binding(binding),
            Ok(None) => CoopEntityResolution::Unavailable {
                reason: ResolutionUnavailable::Unmapped,
            },
            Err(_) => CoopEntityResolution::Unavailable {
                reason: ResolutionUnavailable::SourceUnavailable,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `coop_id` that is itself a valid cooperative [`EntityId`] slug — used to
    /// prove the default does NOT fall back to projecting it.
    const MAPPABLE_SLUG: &str = "food-coop";

    /// Ids the projection bridge would reject anyway (uppercase, underscore, unicode,
    /// too short, empty, non-slug uuid form).
    const NON_MAPPABLE: &[&str] = &["coop_A", "FoodCoop", "café-coop", "abc", "", "coop:7f3a2b"];

    #[tokio::test]
    async fn unwired_resolver_resolves_nothing_for_any_coop_id() {
        let resolver = UnwiredCoopEntityResolver;
        for coop_id in NON_MAPPABLE.iter().copied().chain([MAPPABLE_SLUG]) {
            let out = resolver.resolve_coop_entity(coop_id).await;
            assert_eq!(
                out,
                CoopEntityResolution::Unavailable {
                    reason: ResolutionUnavailable::NoTrustedSourceWired
                },
                "coop_id {coop_id:?} must resolve to NoTrustedSourceWired"
            );
            assert!(!out.is_resolved());
            assert_eq!(out.resolved_entity_id(), None);
        }
    }

    #[tokio::test]
    async fn unwired_resolver_never_fabricates_entity_id_even_for_valid_slug() {
        // `food-coop` IS a valid cooperative EntityId slug, so a naive resolver might
        // be tempted to project it. The fail-closed default must not.
        assert!(
            EntityId::cooperative(MAPPABLE_SLUG).is_ok(),
            "test premise: {MAPPABLE_SLUG} is a valid cooperative slug"
        );
        let out = UnwiredCoopEntityResolver
            .resolve_coop_entity(MAPPABLE_SLUG)
            .await;
        assert!(!out.is_resolved());
        assert_eq!(out.resolved_entity_id(), None);
    }

    #[tokio::test]
    async fn unwired_resolution_cannot_be_mistaken_for_success() {
        let out = UnwiredCoopEntityResolver
            .resolve_coop_entity("anything")
            .await;
        match out {
            CoopEntityResolution::Resolved { .. } => panic!("default must never resolve"),
            CoopEntityResolution::Unavailable { reason } => {
                assert_eq!(reason, ResolutionUnavailable::NoTrustedSourceWired);
            }
        }
    }

    #[tokio::test]
    async fn unwired_resolver_is_deterministic() {
        let resolver = UnwiredCoopEntityResolver;
        let first = resolver.resolve_coop_entity(MAPPABLE_SLUG).await;
        let second = resolver.resolve_coop_entity(MAPPABLE_SLUG).await;
        assert_eq!(first, second);
    }

    #[test]
    fn unavailable_reason_labels_are_stable() {
        assert_eq!(
            ResolutionUnavailable::NoTrustedSourceWired.label(),
            "no_trusted_source_wired"
        );
        assert_eq!(ResolutionUnavailable::Unmapped.label(), "unmapped");
        assert_eq!(ResolutionUnavailable::Ambiguous.label(), "ambiguous");
        assert_eq!(ResolutionUnavailable::Revoked.label(), "revoked");
        assert_eq!(
            ResolutionUnavailable::SourceUnavailable.label(),
            "source_unavailable"
        );
        assert_eq!(
            ResolutionUnavailable::UnverifiedProvenance.label(),
            "unverified_provenance"
        );
        assert_eq!(
            ResolutionUnavailable::EntityTypeMismatch.label(),
            "entity_type_mismatch"
        );
    }

    // ------------------------------------------------------------------
    // StoreBackedCoopEntityResolver (A2c): provenance-gated, fail-closed.
    // ------------------------------------------------------------------

    use icn_entity::{CoopEntityMapError, InMemoryCoopEntityMap};

    /// `food-coop` is itself a valid cooperative slug, so the legacy projection
    /// (`legacy_coop_id_to_entity_id_fallback`) yields `EntityId::cooperative("food-coop")`.
    const COOP_ID: &str = "food-coop";

    fn store_backed(
        map: impl CoopEntityMap + Send + Sync + 'static,
    ) -> StoreBackedCoopEntityResolver {
        StoreBackedCoopEntityResolver::new(Arc::new(map))
    }

    fn coop_entity() -> EntityId {
        EntityId::cooperative(COOP_ID).expect("test premise: valid cooperative slug")
    }

    /// A `CoopEntityMap` whose reads always fail — proves the resolver fails closed
    /// to `SourceUnavailable` on a backend error and never resolves on error.
    struct FailingCoopEntityMap;

    impl CoopEntityMap for FailingCoopEntityMap {
        fn bind_resolved(
            &self,
            _coop_id: &str,
            _entity_id: &EntityId,
        ) -> Result<(), CoopEntityMapError> {
            Ok(())
        }
        fn entity_for_coop(&self, _coop_id: &str) -> Result<Option<EntityId>, CoopEntityMapError> {
            Err(CoopEntityMapError::Storage(
                "simulated backend failure".into(),
            ))
        }
        fn coop_for_entity(
            &self,
            _entity_id: &EntityId,
        ) -> Result<Option<String>, CoopEntityMapError> {
            Err(CoopEntityMapError::Storage(
                "simulated backend failure".into(),
            ))
        }
    }

    #[tokio::test]
    async fn store_backed_resolves_binding_with_trusted_provenance() {
        let map = InMemoryCoopEntityMap::new();
        let entity = coop_entity();
        map.bind_resolved_with_provenance(
            COOP_ID,
            &entity,
            CoopEntityBindingProvenance::Activation,
        )
        .expect("bind with trusted provenance");
        let out = store_backed(map).resolve_coop_entity(COOP_ID).await;
        assert_eq!(out, CoopEntityResolution::Resolved { entity_id: entity });
    }

    #[tokio::test]
    async fn store_backed_resolves_governance_receipt_provenance() {
        let map = InMemoryCoopEntityMap::new();
        let entity = coop_entity();
        map.bind_resolved_with_provenance(
            COOP_ID,
            &entity,
            CoopEntityBindingProvenance::GovernanceReceipt {
                receipt_id: "rcpt-1".into(),
            },
        )
        .expect("bind with trusted provenance");
        let out = store_backed(map).resolve_coop_entity(COOP_ID).await;
        assert!(out.is_resolved());
        assert_eq!(out.resolved_entity_id(), Some(&entity));
    }

    #[tokio::test]
    async fn store_backed_does_not_resolve_missing_provenance() {
        // `bind_resolved` records no provenance, so the read returns the fail-closed
        // `UnknownLegacy` sentinel. Missing provenance must never resolve.
        let map = InMemoryCoopEntityMap::new();
        let entity = coop_entity();
        map.bind_resolved(COOP_ID, &entity)
            .expect("bind without provenance");
        let out = store_backed(map).resolve_coop_entity(COOP_ID).await;
        assert_eq!(
            out,
            CoopEntityResolution::Unavailable {
                reason: ResolutionUnavailable::UnverifiedProvenance
            }
        );
    }

    #[tokio::test]
    async fn store_backed_does_not_resolve_unknown_legacy_provenance() {
        // An explicit `UnknownLegacy` request is treated by the substrate as "no
        // provenance recorded"; it reads back as `UnknownLegacy` and must not resolve.
        let map = InMemoryCoopEntityMap::new();
        let entity = coop_entity();
        map.bind_resolved_with_provenance(
            COOP_ID,
            &entity,
            CoopEntityBindingProvenance::UnknownLegacy,
        )
        .expect("bind unknown-legacy");
        let out = store_backed(map).resolve_coop_entity(COOP_ID).await;
        assert!(!out.is_resolved());
        assert_eq!(out.resolved_entity_id(), None);
        assert_eq!(
            out,
            CoopEntityResolution::Unavailable {
                reason: ResolutionUnavailable::UnverifiedProvenance
            }
        );
    }

    #[tokio::test]
    async fn store_backed_reports_unmapped_for_missing_binding() {
        let out = store_backed(InMemoryCoopEntityMap::new())
            .resolve_coop_entity(COOP_ID)
            .await;
        assert_eq!(
            out,
            CoopEntityResolution::Unavailable {
                reason: ResolutionUnavailable::Unmapped
            }
        );
    }

    #[tokio::test]
    async fn store_backed_fails_closed_on_store_error() {
        let out = store_backed(FailingCoopEntityMap)
            .resolve_coop_entity(COOP_ID)
            .await;
        assert!(!out.is_resolved());
        assert_eq!(
            out,
            CoopEntityResolution::Unavailable {
                reason: ResolutionUnavailable::SourceUnavailable
            }
        );
    }

    /// Guards the core non-enforcement invariant: even a fully-resolving store-backed
    /// resolver, run through the exact observe path the treasury route uses, produces
    /// only a migration-visibility *classification* — never an allow/deny. The
    /// production call site is unchanged (still `UnwiredCoopEntityResolver`); this
    /// proves wiring a real resolver later cannot, by this type's shape, alter route
    /// authorization.
    #[tokio::test]
    async fn store_backed_resolver_in_observe_mode_only_classifies_never_authorizes() {
        use crate::authority::{observe_coop_entity_resolution, CoopResolutionObservation};
        let map = InMemoryCoopEntityMap::new();
        let entity = coop_entity();
        map.bind_resolved_with_provenance(
            COOP_ID,
            &entity,
            CoopEntityBindingProvenance::Activation,
        )
        .expect("bind with trusted provenance");
        let resolver = store_backed(map);
        // The resolver resolves `food-coop` to the same EntityId the legacy
        // projection yields, so the observe-mode comparison classifies as `Agree`.
        // `observe_coop_entity_resolution` returns a classification value and performs
        // no authorization.
        let observation = observe_coop_entity_resolution(&resolver, COOP_ID).await;
        assert_eq!(observation, CoopResolutionObservation::Agree);
    }
}
