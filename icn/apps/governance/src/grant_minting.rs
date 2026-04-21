//! ADR-0014 AuthorityGrant minting at the accepted-decision seam.
//!
//! This module derives zero or more [`AuthorityGrant`]s from an accepted
//! proposal. It is deliberately **narrow and truthful**:
//!
//! - It mints a grant **only** for proposal classes whose payload
//!   already names the grantee, the class of authority, and a truthful
//!   time bound. If a payload does not carry that information, we
//!   return an empty `Vec` — no grant is better than a fabricated one.
//! - The grantor is always the sovereign entity behind the accepted
//!   decision (the governance domain the proposal was decided in). The
//!   platform, the runtime, the gateway, and any shared service are
//!   never grantors here.
//! - Scope is populated **only** with categories that can be honestly
//!   derived from the payload. No invented action kinds, no invented
//!   ceilings, no invented durations.
//! - This module does not gate dispatch, authorize executors, or
//!   change effect semantics. It only produces records.
//!
//! # Bootstrap posture
//!
//! Today the only classes that mint grants are steward-appointment and
//! steward-reconfirmation proposals: those are the narrowest cases
//! where the accepted payload unambiguously names a grantee, a term
//! length, and a class (Attestation — stewards issue attestations).
//! Every other accepted proposal currently produces zero grants, and
//! its mandate is recorded via [`crate::manager`] using the
//! bootstrap-phase `new_pending_grants` constructor. Expanding this
//! set is explicit future work; silently broadening it would be the
//! kind of "mint default grants" move the ADR specifically forbids.
//!
//! # Distinctness
//!
//! `AuthorityGrant` lives on the *authorization* side of the chain:
//!
//! ```text
//!     Charter → Decision → Mandate [ + Grants ] → Action → Receipt → Evidence
//! ```
//!
//! It is **not** a replacement for `InstitutionalEffectRecord` or
//! `EffectDispatchEvidence`; those record downstream execution and
//! evidence, which is a separate layer.

use icn_governance::{
    AuthorityClass, AuthorityGrant, AuthorityGrantId, DecisionProvenance, GovernanceDomainId,
    Grantee, GrantorEntityId, Mandate, MandateId, ProposalPayload, Timestamp, TypedScope,
};

use crate::receipt_backend::GovernanceReceiptBackend;

/// Canonical content hash of a `ProposalPayload`.
///
/// Used by the mandate seam to bind the accepted decision's content into
/// the mandate. Returns `Err` on serialization failure — callers must
/// decline to mint a mandate in that case; substituting a sentinel hash
/// would collapse distinct payloads to the same content-binding and
/// silently break the mandate↔payload invariant.
pub(crate) fn hash_proposal_payload(
    payload: &ProposalPayload,
) -> Result<icn_kernel_api::Hash, serde_json::Error> {
    let bytes = serde_json::to_vec(payload)?;
    Ok(*blake3::hash(&bytes).as_bytes())
}

/// Outcome of running the ADR-0014 mandate seam at acceptance time.
///
/// Modeled after [`crate::institutional_effect::AcceptanceEmissionOutcome`]
/// so both paths that call into the seam can pattern-match uniformly.
#[derive(Debug, Clone)]
pub enum MandateMintOutcome {
    /// A new mandate was derived and persisted. `grants_persisted` is the
    /// number of `AuthorityGrant`s that were successfully stored before
    /// the mandate — it may be zero (e.g. a `Text` proposal), in which
    /// case the mandate was recorded with `has_no_grants()` semantics.
    Minted {
        mandate_id: MandateId,
        grants_persisted: usize,
    },
    /// A mandate for this proposal already exists in the backend.
    /// Idempotent re-acceptance; no new writes happened.
    AlreadyMinted { mandate_id: MandateId },
    /// Payload hashing failed, so no mandate was minted. Substituting a
    /// sentinel hash would silently break content-binding.
    HashFailed,
}

/// Mint and persist the constitutional-memory artifacts for an accepted
/// decision — zero or more [`AuthorityGrant`]s plus one [`Mandate`].
///
/// This is the **canonical shared seam** called by both the standalone
/// close path in [`crate::manager::GovernanceManager::close_proposal_inner`]
/// and the actor-backed close handler in [`crate::actor`]. Both paths
/// invoke it with the same arguments so the constitutional-memory
/// record lands regardless of which path produced the acceptance.
///
/// Idempotency: if a mandate already exists for `proposal_id` in the
/// backend, returns [`MandateMintOutcome::AlreadyMinted`] without
/// writing. This matches the actor/HTTP idempotency pattern used by
/// the institutional-effect seam.
///
/// Caller contract: only invoke after the proposal has been recorded
/// as `Accepted` and after the governance decision receipt has been
/// stored (so `decision_hash` binds into the INV-5 chain).
pub fn mint_and_persist_for_accepted(
    backend: &dyn GovernanceReceiptBackend,
    proposal_id: &str,
    domain_id: &GovernanceDomainId,
    decision_hash: icn_kernel_api::Hash,
    payload: &ProposalPayload,
    now: Timestamp,
) -> Result<MandateMintOutcome, String> {
    // Idempotency: if a mandate already exists for this proposal, don't
    // re-derive or re-persist. Backends that don't implement the lookup
    // (the default-no-op) return `Ok(None)`, which falls through to the
    // mint path; in that case duplicate minting is acceptable because
    // the default-no-op put is also a no-op.
    if let Some(prior) = backend.get_mandate_by_proposal(proposal_id)? {
        return Ok(MandateMintOutcome::AlreadyMinted {
            mandate_id: prior.id,
        });
    }

    let payload_hash = match hash_proposal_payload(payload) {
        Ok(h) => h,
        Err(_) => return Ok(MandateMintOutcome::HashFailed),
    };

    let decision_prov = DecisionProvenance {
        proposal_id: proposal_id.to_string(),
        decision_hash,
    };

    // Apply lifecycle side effects (revocations / reinstatement) for
    // SDIS proposal variants that mutate existing authority. Revocations
    // are durably stamped on target primary records before we proceed to
    // mandate composition. Reinstatement may mint a fresh grant that
    // composes into the derive-path grant set below.
    //
    // Revocations happen outside the mandate transaction — they must, in
    // this tranche, because `put_mandate_with_grants_atomic` is a write
    // path (inserts + index updates) that does not know about primary-
    // record mutations. This creates a recoverable race window: if a
    // revocation lands but the mandate write subsequently fails, the
    // caller retries this seam → the idempotency check at the top of
    // this function misses (no mandate) → the lifecycle runs again →
    // `revoke_authority_grant`'s first-write-wins semantics turn the
    // repeat into a no-op → the mandate write is re-attempted. The
    // original `revoked_at` timestamp is preserved across retries.
    let lifecycle_grants =
        apply_acceptance_lifecycle(backend, payload, domain_id, &decision_prov, now);

    // Derive zero or more grants from the payload itself. Empty vec is
    // the truthful default for most payload classes today.
    let mut grants = derive_grants_for_accepted_proposal(payload, domain_id, &decision_prov, now);
    grants.extend(lifecycle_grants);
    let grant_ids: Vec<_> = grants.iter().map(|g| g.id.clone()).collect();

    // When we derived grants, attempt the atomic
    // `put_mandate_with_grants` commit: mandate + all grants land
    // together or neither does. The gateway sled-backed ReceiptStore
    // overrides this with a real transaction; the default impl (used by
    // in-memory test backends and the sled backend's inherited no-op
    // for grant storage) does sequential writes with read-after-write
    // verification, returning the sentinel
    // `grant_durability_not_supported` error if a grant cannot be
    // round-tripped. That sentinel is the cue to degrade to a
    // pending-grants mandate so no orphan grant IDs appear in the
    // mandate record.
    if !grant_ids.is_empty() {
        let strict_mandate = match Mandate::new(
            decision_prov.clone(),
            payload_hash,
            grant_ids,
            None,
            None,
            now,
        ) {
            Ok(m) => m,
            Err(e) => {
                tracing::error!(
                    proposal_id = %proposal_id,
                    error = %e,
                    "Mandate::new rejected derived grant set; recording pending-grants mandate"
                );
                return write_pending_mandate(backend, decision_prov, payload_hash, now);
            }
        };
        let mandate_id = strict_mandate.id.clone();
        match backend.put_mandate_with_grants(&strict_mandate, &grants) {
            Ok(()) => {
                return Ok(MandateMintOutcome::Minted {
                    mandate_id,
                    grants_persisted: grants.len(),
                });
            }
            Err(e) if e.starts_with("grant_durability_not_supported") => {
                tracing::warn!(
                    proposal_id = %proposal_id,
                    error = %e,
                    "Backend does not durably persist authority grants; mandate will be recorded as pending-grants"
                );
                // Fall through to pending-grants record. No orphan grants
                // because the sequential default bails out on the first
                // failed read-after-write before persisting subsequent
                // grants; the atomic override leaves the store unchanged
                // on abort.
            }
            Err(e) => {
                tracing::error!(
                    proposal_id = %proposal_id,
                    error = %e,
                    "Atomic put_mandate_with_grants failed; no mandate recorded"
                );
                return Err(e);
            }
        }
    }

    write_pending_mandate(backend, decision_prov, payload_hash, now)
}

/// Record a pending-grants mandate and return a `Minted { grants_persisted: 0 }`
/// outcome. Used when the payload derives no grants, when `Mandate::new`
/// rejects a derived grant set, or when the backend cannot durably
/// persist grants.
fn write_pending_mandate(
    backend: &dyn GovernanceReceiptBackend,
    decision_prov: DecisionProvenance,
    payload_hash: icn_kernel_api::Hash,
    now: Timestamp,
) -> Result<MandateMintOutcome, String> {
    let mandate = Mandate::new_pending_grants(decision_prov, payload_hash, None, None, now);
    let mandate_id = mandate.id.clone();
    backend.put_mandate(&mandate)?;
    Ok(MandateMintOutcome::Minted {
        mandate_id,
        grants_persisted: 0,
    })
}

/// Derive zero or more [`AuthorityGrant`]s from an accepted proposal.
///
/// Returns an empty vector for payloads that cannot truthfully be
/// translated into bounded grants today. Callers **must not** treat
/// the empty case as an error — it is the correct behavior for the
/// vast majority of proposal classes in this bootstrap tranche.
pub(crate) fn derive_grants_for_accepted_proposal(
    payload: &ProposalPayload,
    domain_id: &GovernanceDomainId,
    decision: &DecisionProvenance,
    now: Timestamp,
) -> Vec<AuthorityGrant> {
    match payload {
        ProposalPayload::Sdis { proposal } => {
            derive_sdis_grants(proposal, domain_id, decision, now)
        }
        // Every other payload class currently mints no grants. This is
        // the truthful default: we do not yet have enough structure in
        // the payload to derive a narrow, bounded grant without
        // guessing. Expanding this is intentional future work.
        _ => Vec::new(),
    }
}

fn derive_sdis_grants(
    sdis: &icn_governance::sdis::SdisProposal,
    domain_id: &GovernanceDomainId,
    decision: &DecisionProvenance,
    now: Timestamp,
) -> Vec<AuthorityGrant> {
    use icn_governance::sdis::SdisProposal;

    match sdis {
        // Steward appointment: the payload names the candidate
        // (grantee), a term length (time bound), and implies
        // Attestation class (stewards issue signed identity
        // attestations under SDIS).
        SdisProposal::AppointSteward {
            candidate,
            term_length,
            ..
        } => {
            // Overflow must fail closed: an overflowed `now + term_length`
            // previously fell through to `valid_until: None`, which the
            // grant model interprets as unbounded authority. A steward
            // term cannot silently become permanent because of arithmetic
            // overflow. Decline to mint; the caller records a
            // pending-grants mandate instead.
            let Some(valid_until) = now.checked_add(*term_length) else {
                tracing::error!(
                    grantee = ?candidate,
                    now,
                    term_length,
                    "AppointSteward term_length overflow; declining to mint grant (would be unbounded)"
                );
                return Vec::new();
            };
            let scope = TypedScope {
                domain: Some(domain_id.clone()),
                proposal_class: vec!["Sdis".into()],
                ..TypedScope::default()
            };
            vec![AuthorityGrant {
                id: AuthorityGrantId::new(),
                class: AuthorityClass::Attestation,
                grantor: GrantorEntityId(domain_id.0.clone()),
                grantee: Grantee::Person(candidate.clone()),
                scope,
                granted_by: Some(decision.clone()),
                valid_from: now,
                valid_until: Some(valid_until),
                revoked_at: None,
            }]
        }

        // Steward reconfirmation: the payload names the steward
        // (grantee) and a new absolute term end (time bound). Same
        // class / scope shape as appointment; the grant represents
        // the refreshed term.
        SdisProposal::ReconfirmSteward {
            steward,
            new_term_end,
            ..
        } => {
            let scope = TypedScope {
                domain: Some(domain_id.clone()),
                proposal_class: vec!["Sdis".into()],
                ..TypedScope::default()
            };
            vec![AuthorityGrant {
                id: AuthorityGrantId::new(),
                class: AuthorityClass::Attestation,
                grantor: GrantorEntityId(domain_id.0.clone()),
                grantee: Grantee::Person(steward.clone()),
                scope,
                granted_by: Some(decision.clone()),
                valid_from: now,
                valid_until: Some(*new_term_end),
                revoked_at: None,
            }]
        }

        // Removals, sanctions, suspensions, and authority revocations
        // do not mint **new** grants from the derive path — they
        // *revoke* existing authority. The side-effecting lifecycle
        // work (durably stamping `revoked_at` on the target's grants,
        // and minting a fresh grant for reinstatement) happens in
        // [`apply_acceptance_lifecycle`], which is a separate seam so
        // revocation writes and the pending-grants fall-through stay
        // untangled from pure derivation. Returning an empty vec here
        // keeps the "derivation is pure; lifecycle is side-effecting"
        // split clean.
        SdisProposal::RemoveSteward { .. }
        | SdisProposal::SanctionSteward { .. }
        | SdisProposal::SuspendSteward { .. }
        | SdisProposal::ReinstateSteward { .. }
        | SdisProposal::RevokeAuthority { .. }
        | SdisProposal::RevocationAppeal { .. }
        | SdisProposal::ModifyThreshold { .. }
        | SdisProposal::ApproveAuthority { .. }
        | SdisProposal::UpdateJurisdictionTier { .. }
        | SdisProposal::ForceKeyRotation { .. } => Vec::new(),
    }
}

/// Apply lifecycle side effects (revocations, reinstatement) at proposal
/// acceptance time and return any fresh grants minted by reinstatement.
///
/// Called from [`mint_and_persist_for_accepted`] after the idempotency
/// check passes. For revocation-shaped SDIS payloads this durably stamps
/// `revoked_at` on the target's active grants via
/// [`GovernanceReceiptBackend::revoke_authority_grant`]. For
/// [`SdisProposal::ReinstateSteward`] this mints a brand-new grant
/// (fresh UUID) cloning class/scope from the most-recent prior grant
/// and setting `grantor` from this reinstatement decision's
/// `domain_id` — never mutating a revoked grant back to active.
///
/// Revocations are best-effort: backend errors are logged and do not
/// abort the decision. A `grant_not_found` error from an in-flight index
/// skew is treated as skippable.
///
/// All backend calls on the defaulted no-op trait methods return empty
/// lists / `Ok(())`, so in-memory test backends that do not durably
/// persist grants produce the truthful "no grants to revoke" answer
/// without error.
fn apply_acceptance_lifecycle(
    backend: &dyn GovernanceReceiptBackend,
    payload: &ProposalPayload,
    domain_id: &GovernanceDomainId,
    decision: &DecisionProvenance,
    now: Timestamp,
) -> Vec<AuthorityGrant> {
    let ProposalPayload::Sdis { proposal } = payload else {
        return Vec::new();
    };
    use icn_governance::sdis::{SdisProposal, StewardPenalty};

    match proposal {
        // Direct-removal / direct-suspension: the target loses active
        // authority at `now`. No new grants are minted. Scoped to
        // grants issued by the deciding domain only — another domain's
        // grants are never revoked by this domain's vote.
        SdisProposal::RemoveSteward { steward, .. }
        | SdisProposal::SuspendSteward { steward, .. } => {
            revoke_active_grants_for_person(backend, steward, domain_id, now);
            Vec::new()
        }

        // Sanctions only revoke when the penalty is removal-shaped.
        // Warnings, bond-slashes, tier-demotions, and probation do not
        // terminate existing authority; they operate on other axes
        // (reputation, bond, monitoring) that the grant record does
        // not model. Suspension and Removal do terminate authority;
        // those penalties revoke the target's active grants issued by
        // the deciding domain.
        SdisProposal::SanctionSteward {
            steward, penalty, ..
        } => {
            let revokes = matches!(
                penalty,
                StewardPenalty::Removal { .. } | StewardPenalty::Suspension { .. }
            );
            if revokes {
                revoke_active_grants_for_person(backend, steward, domain_id, now);
            }
            Vec::new()
        }

        // Institutional-authority revocation: honors the payload's
        // `effective_at` when set (allows a governance-granted grace
        // period); otherwise takes effect at `now`. Grants whose
        // `revoked_at` is already set are left untouched by the
        // first-write-wins semantics in the backend. Scoped to grants
        // this domain issued — cross-domain grants are not touched.
        SdisProposal::RevokeAuthority {
            authority_did,
            effective_at,
            ..
        } => {
            let revoke_at = effective_at.unwrap_or(now);
            revoke_active_grants_for_person(backend, authority_did, domain_id, revoke_at);
            Vec::new()
        }

        // Reinstatement mints a **fresh** grant — new UUID, new
        // `valid_from = now`, new provenance bound to *this* decision.
        // It does not mutate the revoked grant back to active. If no
        // prior grant is found, we decline (return empty) and log: the
        // seam refuses to fabricate bounds a payload did not carry.
        SdisProposal::ReinstateSteward { steward, .. } => {
            match mint_reinstatement_grant(backend, steward, domain_id, decision, now) {
                Some(g) => vec![g],
                None => Vec::new(),
            }
        }

        // Revocation appeals do not mutate the original revocation in
        // place — that would violate the first-write-wins invariant on
        // `revoked_at` and would lose the constitutional record of the
        // original revocation event. If an appeal is upheld, the
        // governance flow follows up with a targeted remint proposal
        // (e.g. `ReinstateSteward` for a steward, or a fresh
        // `AppointSteward`/authority-granting proposal). In this
        // tranche the acceptance seam records the mandate (via the
        // pending-grants path) without any grant-store mutation.
        SdisProposal::RevocationAppeal { .. } => {
            tracing::info!(
                proposal_id = %decision.proposal_id,
                "RevocationAppeal acceptance: no in-place grant mutation; any reinstatement routes through a separate reinstatement proposal"
            );
            Vec::new()
        }

        // Non-lifecycle SDIS variants (threshold, authority approval,
        // jurisdiction-tier bumps, forced key rotation) do not revoke
        // grants — they change other domain state. No lifecycle work.
        SdisProposal::AppointSteward { .. }
        | SdisProposal::ReconfirmSteward { .. }
        | SdisProposal::ModifyThreshold { .. }
        | SdisProposal::ApproveAuthority { .. }
        | SdisProposal::UpdateJurisdictionTier { .. }
        | SdisProposal::ForceKeyRotation { .. } => Vec::new(),
    }
}

/// Revoke every active grant whose grantee is the given person DID AND
/// whose grantor is the deciding domain, best-effort. Errors are logged
/// and the decision continues; missing primaries (index skew) are
/// warned and skipped.
///
/// Cross-domain guard: an accepted SDIS revocation runs in exactly one
/// governance domain. That domain's vote must not strip authority
/// granted by another sovereign entity — grants whose `grantor` does
/// not match `domain_id` are skipped unconditionally. This preserves
/// the ADR-0014 invariant that revocation is by-grantor: only the
/// entity that issued a grant can end it.
fn revoke_active_grants_for_person(
    backend: &dyn GovernanceReceiptBackend,
    person: &icn_identity::Did,
    domain_id: &GovernanceDomainId,
    revoked_at: Timestamp,
) {
    let grantee = Grantee::Person(person.clone());
    let grants = match backend.list_active_authority_grants_by_grantee(&grantee, revoked_at) {
        Ok(g) => g,
        Err(e) => {
            tracing::error!(
                grantee = ?grantee,
                error = %e,
                "list_active_authority_grants_by_grantee failed; skipping revocation"
            );
            return;
        }
    };
    for g in grants {
        if g.grantor.0 != domain_id.0 {
            // Cross-domain grant: this decision's domain did not
            // issue it, so this decision does not terminate it.
            tracing::debug!(
                grant_id = %g.id,
                grant_grantor = %g.grantor,
                deciding_domain = %domain_id.0,
                "skipping cross-domain grant during revocation"
            );
            continue;
        }
        match backend.revoke_authority_grant(&g.id, revoked_at) {
            Ok(()) => {
                tracing::info!(
                    grant_id = %g.id,
                    grantee = ?grantee,
                    revoked_at,
                    "revoked authority grant at acceptance seam"
                );
            }
            Err(e) if e.starts_with("grant_not_found") => {
                tracing::warn!(
                    grant_id = %g.id,
                    error = %e,
                    "revoke_authority_grant skipped: grant_not_found (index skew)"
                );
            }
            Err(e) => {
                tracing::error!(
                    grant_id = %g.id,
                    error = %e,
                    "revoke_authority_grant failed"
                );
            }
        }
    }
}

/// Mint a fresh reinstatement grant for a steward, cloning
/// class/scope/grantor from the most-recent prior grant. Returns
/// `None` when no prior grant can be located; reinstatement declines
/// rather than fabricate bounds the payload does not carry.
///
/// Term length is taken from the prior grant's
/// `valid_until - valid_from` (when `valid_until` was bounded); for
/// prior grants that were unbounded (`valid_until: None`) the fresh
/// grant is also unbounded, which mirrors the prior authority shape.
fn mint_reinstatement_grant(
    backend: &dyn GovernanceReceiptBackend,
    steward: &icn_identity::Did,
    domain_id: &GovernanceDomainId,
    decision: &DecisionProvenance,
    now: Timestamp,
) -> Option<AuthorityGrant> {
    let grantee = Grantee::Person(steward.clone());
    let all = match backend.list_authority_grants_by_grantee(&grantee) {
        Ok(g) => g,
        Err(e) => {
            tracing::error!(
                grantee = ?grantee,
                error = %e,
                "list_authority_grants_by_grantee failed; declining to mint reinstatement grant"
            );
            return None;
        }
    };
    // Prefer the most-recent revoked grant (typical reinstatement
    // shape); fall back to the most-recent grant of any status so a
    // reinstatement after a mere expiry also works. Backend returns
    // oldest-first by `valid_from`, so we take the last entry.
    let template = all
        .iter()
        .filter(|g| g.revoked_at.is_some())
        .next_back()
        .or_else(|| all.last());
    let template = match template {
        Some(t) => t.clone(),
        None => {
            tracing::warn!(
                grantee = ?grantee,
                proposal_id = %decision.proposal_id,
                "ReinstateSteward: no prior grant found for steward; declining to mint (no term bounds to clone)"
            );
            return None;
        }
    };

    let valid_until = match template.valid_until {
        Some(old_until) => {
            let term = old_until.saturating_sub(template.valid_from);
            now.checked_add(term)
        }
        None => None,
    };
    // If we attempted to add a bounded term and it overflowed, decline
    // rather than silently widen the grant to unbounded (same fail-
    // closed rule as [`derive_sdis_grants`] applies to AppointSteward).
    if template.valid_until.is_some() && valid_until.is_none() {
        tracing::error!(
            grantee = ?grantee,
            now,
            "ReinstateSteward term overflow cloning prior grant; declining to mint (would be unbounded)"
        );
        return None;
    }

    // Reinstatement preserves the prior grant scope as-is by cloning the
    // template scope without validating or rewriting any embedded domain
    // identifier. Prior grants issued under a different domain therefore
    // keep their original domain context. The grantor remains the
    // sovereign entity that decided *this* reinstatement, which matches
    // the derive-path convention.
    let scope = template.scope.clone();

    Some(AuthorityGrant {
        id: AuthorityGrantId::new(),
        class: template.class,
        grantor: GrantorEntityId(domain_id.0.clone()),
        grantee: Grantee::Person(steward.clone()),
        scope,
        granted_by: Some(decision.clone()),
        valid_from: now,
        valid_until,
        revoked_at: None,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;
    use icn_governance::sdis::SdisProposal;
    use icn_identity::Did;

    fn did(seed: u8) -> Did {
        Did::from_anchor_id(&[seed; 32])
    }

    fn domain() -> GovernanceDomainId {
        GovernanceDomainId("coop:tech".into())
    }

    fn decision() -> DecisionProvenance {
        DecisionProvenance {
            proposal_id: "prop-steward-1".into(),
            decision_hash: [7u8; 32],
        }
    }

    #[test]
    fn appoint_steward_mints_one_attestation_grant_bounded_by_term() {
        let payload = ProposalPayload::Sdis {
            proposal: SdisProposal::AppointSteward {
                candidate: did(9),
                sponsors: vec![did(1), did(2)],
                region: "nyc".into(),
                bond_amount: 100,
                term_length: 365 * 24 * 60 * 60,
            },
        };
        let d = decision();
        let dom = domain();
        let now: Timestamp = 1_000;

        let grants = derive_grants_for_accepted_proposal(&payload, &dom, &d, now);
        assert_eq!(grants.len(), 1, "expected exactly one grant");
        let g = &grants[0];

        assert_eq!(g.class, AuthorityClass::Attestation);
        assert_eq!(g.grantor, GrantorEntityId("coop:tech".into()));
        assert_eq!(g.grantee, Grantee::Person(did(9)));
        assert_eq!(g.granted_by.as_ref().unwrap(), &d);
        assert_eq!(g.valid_from, now);
        assert_eq!(g.valid_until, Some(now + 365 * 24 * 60 * 60));
        assert!(g.revoked_at.is_none());

        assert_eq!(g.scope.domain.as_ref(), Some(&dom));
        assert_eq!(g.scope.proposal_class, vec!["Sdis".to_string()]);
        assert!(g.scope.action_kind.is_empty());
        assert!(g.scope.amount_ceiling.is_none());
        assert!(
            !g.scope.is_empty(),
            "scope must not be empty (unbounded-on-everything malformation)"
        );
    }

    #[test]
    fn reconfirm_steward_mints_one_attestation_grant_bounded_by_new_term_end() {
        let new_term_end: Timestamp = 5_000_000;
        let payload = ProposalPayload::Sdis {
            proposal: SdisProposal::ReconfirmSteward {
                steward: did(3),
                new_term_end,
                performance_notes: None,
            },
        };
        let d = decision();
        let dom = domain();

        let grants = derive_grants_for_accepted_proposal(&payload, &dom, &d, 1_000);
        assert_eq!(grants.len(), 1);
        let g = &grants[0];
        assert_eq!(g.class, AuthorityClass::Attestation);
        assert_eq!(g.grantee, Grantee::Person(did(3)));
        assert_eq!(g.valid_until, Some(new_term_end));
    }

    #[test]
    fn remove_steward_mints_zero_grants() {
        let payload = ProposalPayload::Sdis {
            proposal: SdisProposal::RemoveSteward {
                steward: did(3),
                reason: "breach".into(),
                return_bond: false,
            },
        };
        let grants = derive_grants_for_accepted_proposal(&payload, &domain(), &decision(), 100);
        assert!(
            grants.is_empty(),
            "revocation-shaped proposals must not mint new grants"
        );
    }

    #[test]
    fn text_payload_mints_zero_grants() {
        let payload = ProposalPayload::Text {
            body: "hello".into(),
        };
        let grants = derive_grants_for_accepted_proposal(&payload, &domain(), &decision(), 100);
        assert!(grants.is_empty());
    }

    #[test]
    fn budget_payload_mints_zero_grants_until_truthful_mapping_exists() {
        // We intentionally do not mint grants for `Budget` in this
        // tranche: the payload carries amount + currency as strings,
        // which do not map cleanly to the closed `AmountUnit` enum
        // without guessing. Adding a truthful mapping is future work.
        let payload = ProposalPayload::Budget {
            amount: 500,
            currency: "COOP".into(),
            recipient: did(4),
            purpose: "lab equipment".into(),
        };
        let grants = derive_grants_for_accepted_proposal(&payload, &domain(), &decision(), 100);
        assert!(grants.is_empty());
    }

    #[test]
    fn appoint_steward_term_length_overflow_declines_to_mint() {
        // u64::MAX term_length with a nonzero `now` overflows
        // `now.checked_add(term_length)`. Before this hardening, that
        // silently produced `valid_until: None` — an effectively
        // unbounded grant. The seam must decline instead.
        let payload = ProposalPayload::Sdis {
            proposal: SdisProposal::AppointSteward {
                candidate: did(9),
                sponsors: vec![did(1)],
                region: "nyc".into(),
                bond_amount: 100,
                term_length: u64::MAX,
            },
        };
        let grants = derive_grants_for_accepted_proposal(
            &payload,
            &domain(),
            &decision(),
            1_000, // now is nonzero → now + u64::MAX overflows
        );
        assert!(
            grants.is_empty(),
            "overflowed term_length must not mint a grant (would be unbounded); got {grants:?}"
        );
    }

    /// Backend that tracks mandates but leaves grant storage at its
    /// defaulted no-op — i.e. `put_authority_grant` returns `Ok(())`
    /// without storing, and `get_authority_grant` returns `Ok(None)`.
    /// Mirrors the real sled backend's current bootstrap posture.
    struct MandateOnlyBackend {
        mandates: std::sync::Mutex<Vec<Mandate>>,
    }

    impl MandateOnlyBackend {
        fn new() -> Self {
            Self {
                mandates: std::sync::Mutex::new(vec![]),
            }
        }
    }

    impl GovernanceReceiptBackend for MandateOnlyBackend {
        fn put_governance(
            &self,
            _: &icn_governance::GovernanceDecisionReceipt,
        ) -> Result<(), String> {
            Ok(())
        }
        fn get_governance_by_proposal(
            &self,
            _: &str,
        ) -> Result<Option<icn_governance::GovernanceDecisionReceipt>, String> {
            Ok(None)
        }
        fn put_allocation(
            &self,
            _: &icn_kernel_api::AllocationReceipt,
        ) -> Result<icn_kernel_api::Hash, String> {
            Ok([0u8; 32])
        }
        fn get_governance_by_decision(
            &self,
            _: &icn_kernel_api::Hash,
        ) -> Result<Option<icn_governance::GovernanceDecisionReceipt>, String> {
            Ok(None)
        }
        fn list_allocations_by_decision(
            &self,
            _: &icn_kernel_api::Hash,
        ) -> Result<Vec<icn_kernel_api::AllocationReceipt>, String> {
            Ok(vec![])
        }
        fn put_mandate(&self, mandate: &Mandate) -> Result<(), String> {
            self.mandates.lock().unwrap().push(mandate.clone());
            Ok(())
        }
        fn get_mandate_by_proposal(&self, proposal_id: &str) -> Result<Option<Mandate>, String> {
            Ok(self
                .mandates
                .lock()
                .unwrap()
                .iter()
                .find(|m| m.decision.proposal_id == proposal_id)
                .cloned())
        }
        // `put_authority_grant` and `get_authority_grant` intentionally
        // left at their defaulted no-ops to simulate an unsupported
        // grant-storage backend.
    }

    #[test]
    fn no_op_grant_backend_falls_back_to_pending_grants_mandate() {
        // AppointSteward would normally mint a grant. But if the
        // backend's `put_authority_grant` is the defaulted no-op and
        // `get_authority_grant` returns None, the read-after-write
        // check must fail the grant, and the mandate must fall back to
        // `new_pending_grants` rather than referencing a non-durable
        // grant ID.
        let backend = MandateOnlyBackend::new();
        let payload = ProposalPayload::Sdis {
            proposal: SdisProposal::AppointSteward {
                candidate: did(9),
                sponsors: vec![did(1)],
                region: "nyc".into(),
                bond_amount: 100,
                term_length: 3_600,
            },
        };

        let outcome = mint_and_persist_for_accepted(
            &backend,
            "prop-no-op-grant",
            &domain(),
            [9u8; 32],
            &payload,
            1_000,
        )
        .expect("mint");

        match outcome {
            MandateMintOutcome::Minted {
                grants_persisted, ..
            } => {
                assert_eq!(
                    grants_persisted, 0,
                    "no-op grant backend must report zero grants persisted"
                );
            }
            other => panic!("expected Minted; got {other:?}"),
        }

        let mandates = backend.mandates.lock().unwrap().clone();
        assert_eq!(mandates.len(), 1, "mandate must still be recorded");
        assert!(
            mandates[0].has_no_grants(),
            "mandate must fall back to pending-grants when grant durability unsupported"
        );
    }

    #[test]
    fn overflow_and_no_op_backend_compose_to_pending_grants() {
        // Defensive: both failure modes at once (overflowed term_length
        // on a backend that doesn't store grants) must still produce a
        // pending-grants mandate, not a strict mandate with fake IDs.
        let backend = MandateOnlyBackend::new();
        let payload = ProposalPayload::Sdis {
            proposal: SdisProposal::AppointSteward {
                candidate: did(9),
                sponsors: vec![did(1)],
                region: "nyc".into(),
                bond_amount: 100,
                term_length: u64::MAX,
            },
        };

        let outcome = mint_and_persist_for_accepted(
            &backend,
            "prop-overflow",
            &domain(),
            [3u8; 32],
            &payload,
            1_000,
        )
        .expect("mint");

        match outcome {
            MandateMintOutcome::Minted {
                grants_persisted, ..
            } => assert_eq!(grants_persisted, 0),
            other => panic!("expected Minted; got {other:?}"),
        }

        let mandates = backend.mandates.lock().unwrap().clone();
        assert_eq!(mandates.len(), 1);
        assert!(mandates[0].has_no_grants());
    }

    #[test]
    fn appoint_steward_grant_provenance_matches_decision() {
        let payload = ProposalPayload::Sdis {
            proposal: SdisProposal::AppointSteward {
                candidate: did(9),
                sponsors: vec![did(1)],
                region: "nyc".into(),
                bond_amount: 100,
                term_length: 1_000,
            },
        };
        let d = DecisionProvenance {
            proposal_id: "prop-xyz".into(),
            decision_hash: [42u8; 32],
        };
        let grants = derive_grants_for_accepted_proposal(&payload, &domain(), &d, 100);
        let g = &grants[0];
        let p = g.granted_by.as_ref().unwrap();
        assert_eq!(p.proposal_id, "prop-xyz");
        assert_eq!(p.decision_hash, [42u8; 32]);
    }

    // ========================================================================
    // Acceptance-seam lifecycle tests (revocation + reinstatement)
    // ========================================================================

    /// In-memory backend that fully supports mandate + grant storage,
    /// including the ADR-0014 revocation write-path. Used by
    /// acceptance-seam lifecycle tests to exercise `apply_acceptance_lifecycle`
    /// without depending on the sled-backed gateway store.
    struct InMemoryGrantBackend {
        mandates: std::sync::Mutex<Vec<Mandate>>,
        grants: std::sync::Mutex<Vec<AuthorityGrant>>,
    }

    impl InMemoryGrantBackend {
        fn new() -> Self {
            Self {
                mandates: std::sync::Mutex::new(vec![]),
                grants: std::sync::Mutex::new(vec![]),
            }
        }
    }

    impl GovernanceReceiptBackend for InMemoryGrantBackend {
        fn put_governance(
            &self,
            _: &icn_governance::GovernanceDecisionReceipt,
        ) -> Result<(), String> {
            Ok(())
        }
        fn get_governance_by_proposal(
            &self,
            _: &str,
        ) -> Result<Option<icn_governance::GovernanceDecisionReceipt>, String> {
            Ok(None)
        }
        fn put_allocation(
            &self,
            _: &icn_kernel_api::AllocationReceipt,
        ) -> Result<icn_kernel_api::Hash, String> {
            Ok([0u8; 32])
        }
        fn get_governance_by_decision(
            &self,
            _: &icn_kernel_api::Hash,
        ) -> Result<Option<icn_governance::GovernanceDecisionReceipt>, String> {
            Ok(None)
        }
        fn list_allocations_by_decision(
            &self,
            _: &icn_kernel_api::Hash,
        ) -> Result<Vec<icn_kernel_api::AllocationReceipt>, String> {
            Ok(vec![])
        }
        fn put_mandate(&self, mandate: &Mandate) -> Result<(), String> {
            self.mandates.lock().unwrap().push(mandate.clone());
            Ok(())
        }
        fn get_mandate_by_proposal(&self, proposal_id: &str) -> Result<Option<Mandate>, String> {
            Ok(self
                .mandates
                .lock()
                .unwrap()
                .iter()
                .find(|m| m.decision.proposal_id == proposal_id)
                .cloned())
        }
        fn put_authority_grant(&self, grant: &AuthorityGrant) -> Result<(), String> {
            let mut guard = self.grants.lock().unwrap();
            if let Some(existing) = guard.iter_mut().find(|g| g.id == grant.id) {
                *existing = grant.clone();
            } else {
                guard.push(grant.clone());
            }
            Ok(())
        }
        fn get_authority_grant(
            &self,
            grant_id: &AuthorityGrantId,
        ) -> Result<Option<AuthorityGrant>, String> {
            Ok(self
                .grants
                .lock()
                .unwrap()
                .iter()
                .find(|g| &g.id == grant_id)
                .cloned())
        }
        fn list_active_authority_grants_by_grantee(
            &self,
            grantee: &Grantee,
            now: Timestamp,
        ) -> Result<Vec<AuthorityGrant>, String> {
            Ok(self
                .grants
                .lock()
                .unwrap()
                .iter()
                .filter(|g| &g.grantee == grantee && g.is_active_at(now))
                .cloned()
                .collect())
        }
        fn list_authority_grants_by_grantee(
            &self,
            grantee: &Grantee,
        ) -> Result<Vec<AuthorityGrant>, String> {
            let mut out: Vec<_> = self
                .grants
                .lock()
                .unwrap()
                .iter()
                .filter(|g| &g.grantee == grantee)
                .cloned()
                .collect();
            out.sort_by_key(|g| g.valid_from);
            Ok(out)
        }
        fn revoke_authority_grant(
            &self,
            grant_id: &AuthorityGrantId,
            revoked_at: Timestamp,
        ) -> Result<(), String> {
            let mut guard = self.grants.lock().unwrap();
            let Some(g) = guard.iter_mut().find(|g| &g.id == grant_id) else {
                return Err(format!("grant_not_found: {grant_id}"));
            };
            // First-write-wins idempotency.
            if g.revoked_at.is_none() {
                g.revoked_at = Some(revoked_at);
            }
            Ok(())
        }
    }

    #[test]
    fn remove_steward_revokes_only_target_grants_at_acceptance_seam() {
        // Two stewards with active grants; a RemoveSteward proposal for
        // steward A must revoke A's grant and leave B's untouched. This
        // is the scoped-revocation invariant the by-grantee index is
        // designed to serve.
        let backend = InMemoryGrantBackend::new();
        let dom = domain();
        let steward_a = did(10);
        let steward_b = did(11);

        // Mint via the normal AppointSteward path so both grants exist
        // under the acceptance seam's own rules.
        mint_and_persist_for_accepted(
            &backend,
            "prop-appoint-a",
            &dom,
            [0xa1u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::AppointSteward {
                    candidate: steward_a.clone(),
                    sponsors: vec![did(1)],
                    region: "nyc".into(),
                    bond_amount: 100,
                    term_length: 3_600,
                },
            },
            1_000,
        )
        .unwrap();
        mint_and_persist_for_accepted(
            &backend,
            "prop-appoint-b",
            &dom,
            [0xa2u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::AppointSteward {
                    candidate: steward_b.clone(),
                    sponsors: vec![did(2)],
                    region: "nyc".into(),
                    bond_amount: 100,
                    term_length: 3_600,
                },
            },
            1_000,
        )
        .unwrap();

        // Sanity: both grants are active pre-revocation.
        let a_active = backend
            .list_active_authority_grants_by_grantee(&Grantee::Person(steward_a.clone()), 2_000)
            .unwrap();
        let b_active = backend
            .list_active_authority_grants_by_grantee(&Grantee::Person(steward_b.clone()), 2_000)
            .unwrap();
        assert_eq!(a_active.len(), 1);
        assert_eq!(b_active.len(), 1);

        // Now accept a RemoveSteward for A only.
        let outcome = mint_and_persist_for_accepted(
            &backend,
            "prop-remove-a",
            &dom,
            [0xadu8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::RemoveSteward {
                    steward: steward_a.clone(),
                    reason: "breach".into(),
                    return_bond: false,
                },
            },
            2_500,
        )
        .unwrap();

        // RemoveSteward itself mints no grant, so the mandate lands in
        // pending-grants shape.
        match outcome {
            MandateMintOutcome::Minted {
                grants_persisted, ..
            } => assert_eq!(grants_persisted, 0),
            other => panic!("expected Minted (pending-grants); got {other:?}"),
        }

        // A's active list must now be empty; B's must still be present.
        let a_after = backend
            .list_active_authority_grants_by_grantee(&Grantee::Person(steward_a.clone()), 3_000)
            .unwrap();
        let b_after = backend
            .list_active_authority_grants_by_grantee(&Grantee::Person(steward_b.clone()), 3_000)
            .unwrap();
        assert!(
            a_after.is_empty(),
            "target steward's active grants must be revoked; got {a_after:?}"
        );
        assert_eq!(
            b_after.len(),
            1,
            "non-target steward's grants must not be touched"
        );

        // A's revoked grant's `revoked_at` equals the decision `now`.
        let a_all = backend
            .list_authority_grants_by_grantee(&Grantee::Person(steward_a.clone()))
            .unwrap();
        assert_eq!(a_all.len(), 1);
        assert_eq!(a_all[0].revoked_at, Some(2_500));
    }

    #[test]
    fn remove_steward_acceptance_is_idempotent_across_retries() {
        // Retrying the RemoveSteward acceptance (e.g. after a mandate-
        // write failure) must not move the original `revoked_at`. The
        // seam-level idempotency check + first-write-wins at the backend
        // together guarantee this.
        let backend = InMemoryGrantBackend::new();
        let dom = domain();
        let steward = did(10);

        mint_and_persist_for_accepted(
            &backend,
            "prop-appoint",
            &dom,
            [0xc1u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::AppointSteward {
                    candidate: steward.clone(),
                    sponsors: vec![did(1)],
                    region: "nyc".into(),
                    bond_amount: 100,
                    term_length: 3_600,
                },
            },
            1_000,
        )
        .unwrap();

        // First acceptance of the RemoveSteward: revocation lands at
        // now=2_500.
        mint_and_persist_for_accepted(
            &backend,
            "prop-remove",
            &dom,
            [0xc2u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::RemoveSteward {
                    steward: steward.clone(),
                    reason: "breach".into(),
                    return_bond: false,
                },
            },
            2_500,
        )
        .unwrap();

        // Simulated retry at a later now=9_999: seam short-circuits on
        // idempotency (mandate already exists). Even if it didn't, the
        // backend's first-write-wins would keep revoked_at at 2_500.
        let outcome = mint_and_persist_for_accepted(
            &backend,
            "prop-remove",
            &dom,
            [0xc2u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::RemoveSteward {
                    steward: steward.clone(),
                    reason: "breach".into(),
                    return_bond: false,
                },
            },
            9_999,
        )
        .unwrap();
        assert!(matches!(outcome, MandateMintOutcome::AlreadyMinted { .. }));

        let all = backend
            .list_authority_grants_by_grantee(&Grantee::Person(steward))
            .unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(
            all[0].revoked_at,
            Some(2_500),
            "original revoked_at must be preserved across retries"
        );
    }

    #[test]
    fn reinstate_steward_mints_fresh_grant_not_mutating_revoked_one() {
        // After a RemoveSteward + ReinstateSteward sequence, the original
        // grant must remain revoked and a brand-new grant (distinct id)
        // must exist for the steward. This is the fresh-grant invariant:
        // reinstatement never resurrects a revoked record.
        let backend = InMemoryGrantBackend::new();
        let dom = domain();
        let steward = did(20);

        // Appoint.
        mint_and_persist_for_accepted(
            &backend,
            "prop-appoint",
            &dom,
            [0xd1u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::AppointSteward {
                    candidate: steward.clone(),
                    sponsors: vec![did(1)],
                    region: "nyc".into(),
                    bond_amount: 100,
                    term_length: 3_600,
                },
            },
            1_000,
        )
        .unwrap();

        let original_id = backend
            .list_authority_grants_by_grantee(&Grantee::Person(steward.clone()))
            .unwrap()[0]
            .id
            .clone();

        // Remove (revoke).
        mint_and_persist_for_accepted(
            &backend,
            "prop-remove",
            &dom,
            [0xd2u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::RemoveSteward {
                    steward: steward.clone(),
                    reason: "misconduct".into(),
                    return_bond: false,
                },
            },
            2_000,
        )
        .unwrap();

        // Reinstate.
        let outcome = mint_and_persist_for_accepted(
            &backend,
            "prop-reinstate",
            &dom,
            [0xd3u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::ReinstateSteward {
                    steward: steward.clone(),
                    reason: "appeal upheld".into(),
                },
            },
            3_000,
        )
        .unwrap();
        match outcome {
            MandateMintOutcome::Minted {
                grants_persisted, ..
            } => assert_eq!(
                grants_persisted, 1,
                "reinstatement must mint exactly one fresh grant"
            ),
            other => panic!("expected Minted with fresh grant; got {other:?}"),
        }

        let all = backend
            .list_authority_grants_by_grantee(&Grantee::Person(steward.clone()))
            .unwrap();
        assert_eq!(all.len(), 2, "original + fresh grant");

        // Original still revoked; fresh grant has distinct id, is active
        // at `now`, and carries the reinstatement decision's provenance.
        let original = all.iter().find(|g| g.id == original_id).unwrap();
        let fresh = all.iter().find(|g| g.id != original_id).unwrap();
        assert_eq!(
            original.revoked_at,
            Some(2_000),
            "revoked grant must not be mutated back to active"
        );
        assert!(
            fresh.revoked_at.is_none(),
            "fresh grant must not be born revoked"
        );
        assert_ne!(
            fresh.id, original_id,
            "reinstatement must allocate a fresh AuthorityGrantId"
        );
        assert!(fresh.is_active_at(3_500));
        assert_eq!(
            fresh.granted_by.as_ref().unwrap().proposal_id,
            "prop-reinstate",
            "fresh grant's provenance must bind to the reinstatement decision"
        );
        assert_eq!(fresh.class, AuthorityClass::Attestation);
        assert_eq!(fresh.grantee, Grantee::Person(steward));
        // Cloned term length: original was 3_600 (1_000..4_600); fresh
        // starts at now=3_000 and must also span 3_600.
        assert_eq!(fresh.valid_from, 3_000);
        assert_eq!(fresh.valid_until, Some(3_000 + 3_600));
    }

    #[test]
    fn reinstate_steward_without_prior_grant_declines_to_mint() {
        // If no prior grant exists for the steward, reinstatement has
        // no template to clone bounds from. It must decline rather than
        // fabricate unbounded authority.
        let backend = InMemoryGrantBackend::new();
        let dom = domain();
        let steward = did(30);

        let outcome = mint_and_persist_for_accepted(
            &backend,
            "prop-reinstate-orphan",
            &dom,
            [0xe1u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::ReinstateSteward {
                    steward: steward.clone(),
                    reason: "mistaken identity".into(),
                },
            },
            1_000,
        )
        .unwrap();

        match outcome {
            MandateMintOutcome::Minted {
                grants_persisted, ..
            } => assert_eq!(
                grants_persisted, 0,
                "reinstatement with no template must produce pending-grants mandate"
            ),
            other => panic!("expected Minted (pending-grants); got {other:?}"),
        }
        let all = backend
            .list_authority_grants_by_grantee(&Grantee::Person(steward))
            .unwrap();
        assert!(all.is_empty(), "no grant may be fabricated");
    }

    #[test]
    fn revoke_authority_honors_effective_at() {
        // RevokeAuthority may carry an `effective_at` grace-period
        // timestamp; the revocation must stamp that value (not `now`)
        // on the target's grants.
        let backend = InMemoryGrantBackend::new();
        let dom = domain();
        let authority = did(40);

        mint_and_persist_for_accepted(
            &backend,
            "prop-appoint-authority",
            &dom,
            [0xf1u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::AppointSteward {
                    candidate: authority.clone(),
                    sponsors: vec![did(1)],
                    region: "nyc".into(),
                    bond_amount: 100,
                    term_length: 10_000,
                },
            },
            1_000,
        )
        .unwrap();

        mint_and_persist_for_accepted(
            &backend,
            "prop-revoke-authority",
            &dom,
            [0xf2u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::RevokeAuthority {
                    authority_did: authority.clone(),
                    reason: "decertified".into(),
                    effective_at: Some(5_555),
                },
            },
            2_000,
        )
        .unwrap();

        let all = backend
            .list_authority_grants_by_grantee(&Grantee::Person(authority))
            .unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(
            all[0].revoked_at,
            Some(5_555),
            "effective_at must override the acceptance `now` for revocation timestamp"
        );
    }

    #[test]
    fn remove_steward_does_not_revoke_cross_domain_grants() {
        // Cross-domain guard: a RemoveSteward accepted in domain A must
        // NOT revoke the same steward's active grant issued by domain B.
        // Only the grantor entity that issued a grant can end it.
        let backend = InMemoryGrantBackend::new();
        let steward = did(12);
        let domain_a = GovernanceDomainId("coop:alpha".into());
        let domain_b = GovernanceDomainId("coop:beta".into());

        // Mint a grant under domain A.
        mint_and_persist_for_accepted(
            &backend,
            "prop-appoint-a",
            &domain_a,
            [0xaau8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::AppointSteward {
                    candidate: steward.clone(),
                    sponsors: vec![did(1)],
                    region: "nyc".into(),
                    bond_amount: 100,
                    term_length: 3_600,
                },
            },
            1_000,
        )
        .unwrap();
        // Mint a grant under domain B for the same steward.
        mint_and_persist_for_accepted(
            &backend,
            "prop-appoint-b",
            &domain_b,
            [0xbbu8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::AppointSteward {
                    candidate: steward.clone(),
                    sponsors: vec![did(2)],
                    region: "nyc".into(),
                    bond_amount: 100,
                    term_length: 3_600,
                },
            },
            1_000,
        )
        .unwrap();

        // Sanity: two active grants for the same grantee, different grantors.
        let active = backend
            .list_active_authority_grants_by_grantee(&Grantee::Person(steward.clone()), 1_500)
            .unwrap();
        assert_eq!(active.len(), 2);

        // Domain A accepts RemoveSteward — must NOT touch domain B's grant.
        mint_and_persist_for_accepted(
            &backend,
            "prop-remove-a",
            &domain_a,
            [0xccu8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::RemoveSteward {
                    steward: steward.clone(),
                    reason: "breach".into(),
                    return_bond: false,
                },
            },
            2_000,
        )
        .unwrap();

        let all = backend
            .list_authority_grants_by_grantee(&Grantee::Person(steward.clone()))
            .unwrap();
        assert_eq!(all.len(), 2, "no grant should be deleted");

        let domain_a_grant = all
            .iter()
            .find(|g| g.grantor == GrantorEntityId(domain_a.0.clone()))
            .expect("domain_a grant must exist");
        let domain_b_grant = all
            .iter()
            .find(|g| g.grantor == GrantorEntityId(domain_b.0.clone()))
            .expect("domain_b grant must exist");

        assert_eq!(
            domain_a_grant.revoked_at,
            Some(2_000),
            "domain_a's own grant must be revoked by its own RemoveSteward decision"
        );
        assert_eq!(
            domain_b_grant.revoked_at, None,
            "domain_b's grant must NOT be revoked by domain_a's decision — cross-domain guard"
        );
    }
}
