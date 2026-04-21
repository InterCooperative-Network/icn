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
    // `revoke_authority_grant`'s monotonic-minimum semantics turn the
    // repeat into a no-op at the same-or-later `now` → the mandate
    // write is re-attempted. The original `revoked_at` timestamp is
    // preserved across same-or-later retries; a strictly-earlier
    // retry would correctly tighten (which is also safe — the grant
    // only terminates sooner, never later).
    //
    // The inverse window — lifecycle write fails and we still record
    // the mandate — must NOT be allowed to close silently: the
    // idempotency check at the top of this function would short-
    // circuit retries to `AlreadyMinted`, skipping the lifecycle, and
    // leave the target's grant active despite an accepted revocation
    // decision. So lifecycle errors propagate; no mandate is recorded
    // when any revocation write failed, and the retry re-runs the
    // whole seam.
    let lifecycle_grants =
        apply_acceptance_lifecycle(backend, payload, domain_id, &decision_prov, now)?;

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

        // Steward reconfirmation handled in the lifecycle seam, not
        // here. A pure derive path cannot enforce the history-aware
        // preconditions (active in-domain grant must exist; the most
        // recent in-domain grant must not be revoked; new term must
        // be strictly in the future). Without those guards, the
        // derive path is a stealth-reinstatement and stealth-
        // appointment vector — see [`apply_acceptance_lifecycle`].
        SdisProposal::ReconfirmSteward { .. } => Vec::new(),

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
/// Revocation write failures propagate: if a revocation cannot be
/// durably stamped, the error is returned so the caller aborts before
/// recording the mandate. Recording a mandate while a revocation is
/// only logged-and-skipped would let the idempotency check at the top
/// of `mint_and_persist_for_accepted` short-circuit retries and leave
/// the target grant active indefinitely. Benign `grant_not_found`
/// errors (index skew) are still skipped locally — the grant is
/// already gone so the caller's intent is satisfied.
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
) -> Result<Vec<AuthorityGrant>, String> {
    let ProposalPayload::Sdis { proposal } = payload else {
        return Ok(Vec::new());
    };
    use icn_governance::sdis::{SdisProposal, StewardPenalty};

    match proposal {
        // Direct-removal / direct-suspension: the target loses active
        // authority at `now`. No new grants are minted. Scoped to
        // grants issued by the deciding domain only — another domain's
        // grants are never revoked by this domain's vote.
        SdisProposal::RemoveSteward { steward, .. }
        | SdisProposal::SuspendSteward { steward, .. } => {
            revoke_active_grants_for_person(backend, steward, domain_id, now, now)?;
            Ok(Vec::new())
        }

        // Sanctions revoke when the penalty terminates authority
        // operationally. Removal and Suspension terminate authority by
        // definition. Probation is mapped by the execution layer
        // (`handlers/execution.rs`) to `SdisEffect::SuspendSteward`, so
        // for grant-lifecycle purposes it must behave as a suspension:
        // leaving the grants active while the execution layer treats
        // the steward as suspended would split "what the execution
        // layer sees" from "what the grant store records". Warnings,
        // bond-slashes, and tier-demotions operate on other axes
        // (reputation, bond, monitoring) that the grant record does
        // not model, so they do not revoke.
        SdisProposal::SanctionSteward {
            steward, penalty, ..
        } => {
            let revokes = matches!(
                penalty,
                StewardPenalty::Removal { .. }
                    | StewardPenalty::Suspension { .. }
                    | StewardPenalty::Probation { .. }
            );
            if revokes {
                revoke_active_grants_for_person(backend, steward, domain_id, now, now)?;
            }
            Ok(Vec::new())
        }

        // Institutional-authority revocation: honors the payload's
        // `effective_at` when set (allows a governance-granted grace
        // period); otherwise takes effect at `now`. Candidate grants
        // are still selected from the set active at acceptance time,
        // so grants minted after `effective_at` but before acceptance
        // are not silently missed. The backend enforces
        // monotonic-minimum: a strictly-earlier revocation tightens,
        // a later-or-equal one is a no-op. Scoped to grants this
        // domain issued — cross-domain grants are not touched.
        SdisProposal::RevokeAuthority {
            authority_did,
            effective_at,
            ..
        } => {
            let revoke_at = effective_at.unwrap_or(now);
            revoke_active_grants_for_person(backend, authority_did, domain_id, now, revoke_at)?;
            Ok(Vec::new())
        }

        // Reinstatement mints a **fresh** grant — new UUID, new
        // `valid_from = now`, new provenance bound to *this* decision.
        // It does not mutate the revoked grant back to active. If no
        // prior grant is found, we decline (return empty) and log: the
        // seam refuses to fabricate bounds a payload did not carry.
        SdisProposal::ReinstateSteward { steward, .. } => Ok(
            match mint_reinstatement_grant(backend, steward, domain_id, decision, now)? {
                Some(g) => vec![g],
                None => Vec::new(),
            },
        ),

        // Steward reconfirmation: refresh the term of an already-
        // active steward. History-aware guards:
        //   - Decline if `new_term_end <= now`. An absolute timestamp
        //     at or before `now` produces an inactive grant — silent
        //     no-op. Fail closed instead.
        //   - Decline unless an active in-domain grant exists. This
        //     closes two attack paths the pure derive path had open:
        //       (a) Stealth reinstatement — ReconfirmSteward on a
        //           steward whose most-recent in-domain grant is
        //           revoked would mint fresh active authority,
        //           bypassing `mint_reinstatement_grant`'s revoked-
        //           template guard and the Reinstate proposal flow.
        //       (b) Stealth appointment — ReconfirmSteward on a DID
        //           that never had a grant in this domain would mint
        //           authority without the AppointSteward sponsorship/
        //           bond/term-length flow.
        //
        // Scope is cloned from the active template rather than
        // fabricated, mirroring `mint_reinstatement_grant`'s pattern
        // so a refresh preserves the prior grant's authority shape
        // rather than quietly rewriting it.
        //
        // The prior active grant is intentionally NOT pre-revoked.
        // Revoking outside `put_mandate_with_grants` would create a
        // retry-livelock under partial-failure: a failed mandate
        // commit after a successful prior-revoke would make the
        // retry's active-grant lookup miss and decline the refresh,
        // leaving the steward with no active authority. Letting the
        // prior grant expire naturally on its original `valid_until`
        // keeps the refresh retry-safe. Short coexistence overlap
        // (prior's remaining term) is a benign consequence: both
        // grants are governance-minted under the same scope/class/
        // grantor, so accumulation is not a privilege-extension
        // vector — it is append-only continuity-of-authority until
        // the old term lapses.
        SdisProposal::ReconfirmSteward {
            steward,
            new_term_end,
            ..
        } => Ok(
            match mint_reconfirmation_grant(
                backend,
                steward,
                domain_id,
                decision,
                now,
                *new_term_end,
            )? {
                Some(g) => vec![g],
                None => Vec::new(),
            },
        ),

        // Revocation appeals do not un-revoke in place — reviving a
        // revoked grant by clearing `revoked_at` would lose the
        // constitutional record of the original revocation event and
        // violate the one-way property the backend's monotonic-minimum
        // rule enforces. If an appeal is upheld, the
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
            Ok(Vec::new())
        }

        // Non-lifecycle SDIS variants (threshold, authority approval,
        // jurisdiction-tier bumps, forced key rotation) do not revoke
        // grants — they change other domain state. No lifecycle work.
        // AppointSteward is handled in the pure derive path (no prior
        // state to consult).
        SdisProposal::AppointSteward { .. }
        | SdisProposal::ModifyThreshold { .. }
        | SdisProposal::ApproveAuthority { .. }
        | SdisProposal::UpdateJurisdictionTier { .. }
        | SdisProposal::ForceKeyRotation { .. } => Ok(Vec::new()),
    }
}

/// Revoke every grant whose grantee is the given person DID, whose
/// grantor is the deciding domain, and which is active at
/// `selection_now`; stamp each revocation with `revoked_at`.
///
/// Error propagation: any failure to *list* or *write* a revocation —
/// other than the benign `grant_not_found` index-skew case — is
/// returned to the caller so the acceptance seam can abort before
/// recording the mandate. The mandate write must not land when a
/// revocation write failed: if it did, the idempotency check at the
/// top of `mint_and_persist_for_accepted` would short-circuit retries
/// to `AlreadyMinted`, skipping the lifecycle, and the target grant
/// would remain active indefinitely despite an accepted revocation
/// decision. Propagating the error lets the retry re-run the lifecycle
/// under monotonic-minimum semantics.
///
/// `grant_not_found` errors (primary record absent despite an active
/// by-grantee index entry — in-flight index skew) are logged and
/// skipped, not propagated: the grant the caller meant to revoke is
/// already gone, so the caller's intent is satisfied. All other
/// backend errors short-circuit the remaining revocations to avoid
/// partial revocation state (some grants revoked, others not, yet no
/// mandate recorded — but under a successful retry the list call
/// re-discovers the not-yet-revoked grants, so stopping early costs
/// at most one extra retry pass).
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
    selection_now: Timestamp,
    revoked_at: Timestamp,
) -> Result<(), String> {
    let grantee = Grantee::Person(person.clone());
    let grants = backend
        .list_active_authority_grants_by_grantee(&grantee, selection_now)
        .map_err(|e| {
            tracing::error!(
                grantee = ?grantee,
                error = %e,
                "list_active_authority_grants_by_grantee failed; aborting acceptance before mandate write"
            );
            e
        })?;
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
                    selection_now,
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
                    "revoke_authority_grant failed; aborting acceptance before mandate write"
                );
                return Err(e);
            }
        }
    }
    Ok(())
}

/// Mint a fresh reinstatement grant for a steward, cloning
/// class/scope from the most-recent **revoked** prior grant issued by
/// **this deciding domain** and setting `grantor` from `domain_id`.
///
/// Returns `Ok(None)` (benign decline) when:
/// - no prior grant exists for this grantee in this domain,
/// - no prior grant in this domain has been revoked (which includes
///   the common case where the steward is already active — a
///   reinstatement of an already-active steward is a no-op, never a
///   privilege extension),
/// - an active in-domain grant already exists for this grantee at
///   `now` — the precheck blocks the "1 revoked + 1 active after a
///   prior Remove→Reinstate cycle" case where a duplicate Reinstate
///   would otherwise find the old revoked grant as a template and
///   silently mint a second concurrent active grant, or
/// - cloning the prior term would overflow `now`.
///
/// Returns `Err(e)` when the backend read fails transiently. The
/// caller must propagate so the mandate is NOT recorded — otherwise
/// the idempotency check at the top of `mint_and_persist_for_accepted`
/// would short-circuit retries to `AlreadyMinted`, skipping the
/// lifecycle, and the accepted reinstatement would never actually
/// materialize authority.
///
/// Reinstatement declines rather than fabricate bounds the payload
/// does not carry, and declines rather than clone a template from
/// another domain's history. Term length is taken from the prior
/// grant's `valid_until - valid_from` (when `valid_until` was
/// bounded); for prior grants that were unbounded (`valid_until:
/// None`) the fresh grant is also unbounded, mirroring the prior
/// authority shape.
fn mint_reinstatement_grant(
    backend: &dyn GovernanceReceiptBackend,
    steward: &icn_identity::Did,
    domain_id: &GovernanceDomainId,
    decision: &DecisionProvenance,
    now: Timestamp,
) -> Result<Option<AuthorityGrant>, String> {
    let grantee = Grantee::Person(steward.clone());
    let all = backend
        .list_authority_grants_by_grantee(&grantee)
        .map_err(|e| {
            tracing::error!(
                grantee = ?grantee,
                error = %e,
                "list_authority_grants_by_grantee failed; aborting acceptance before mandate write"
            );
            e
        })?;
    // Precheck: if any in-domain grant is already active at `now`,
    // decline. The per-candidate filter below selects the most-recent
    // revoked in-domain template, but after a normal
    // Remove→Reinstate cycle the history is [revoked_old, active_new].
    // Without this precheck, a duplicate ReinstateSteward on the same
    // steward would still find `revoked_old`, pass the filter, and
    // silently mint a *second* concurrent active grant — re-opening
    // the privilege-extension path the revoked-filter was supposed to
    // close. Decline instead.
    let has_active_in_domain = all
        .iter()
        .any(|g| g.grantor.0 == domain_id.0 && g.is_active_at(now));
    if has_active_in_domain {
        tracing::warn!(
            grantee = ?grantee,
            proposal_id = %decision.proposal_id,
            deciding_domain = %domain_id.0,
            "ReinstateSteward: an in-domain grant is already active; declining to mint (reinstatement of an active steward is a no-op)"
        );
        return Ok(None);
    }

    // Restrict candidates to:
    //   (a) grants issued by THIS deciding domain — cross-domain
    //       reinstatement would import another sovereign's shape, and
    //   (b) grants that have been revoked — reinstating an already-
    //       active steward must be a no-op, not a privilege-extension
    //       mint. Backend returns oldest-first by `valid_from`, so we
    //       take the last matching entry (most-recent revoked).
    let template = all
        .iter()
        .filter(|g| g.grantor.0 == domain_id.0 && g.revoked_at.is_some())
        .next_back();
    let template = match template {
        Some(t) => t.clone(),
        None => {
            tracing::warn!(
                grantee = ?grantee,
                proposal_id = %decision.proposal_id,
                deciding_domain = %domain_id.0,
                "ReinstateSteward: no revoked in-domain grant found; declining to mint (reinstatement needs a revoked template issued by this domain)"
            );
            return Ok(None);
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
        return Ok(None);
    }

    // Reinstatement preserves the prior grant scope as-is by cloning the
    // template scope without validating or rewriting any embedded domain
    // identifier. Prior grants issued under a different domain therefore
    // keep their original domain context. The grantor remains the
    // sovereign entity that decided *this* reinstatement, which matches
    // the derive-path convention.
    let scope = template.scope.clone();

    Ok(Some(AuthorityGrant {
        id: AuthorityGrantId::new(),
        class: template.class,
        grantor: GrantorEntityId(domain_id.0.clone()),
        grantee: Grantee::Person(steward.clone()),
        scope,
        granted_by: Some(decision.clone()),
        valid_from: now,
        valid_until,
        revoked_at: None,
    }))
}

/// Mint a fresh reconfirmation grant for a steward, cloning
/// class/scope from the current **active** prior grant issued by
/// **this deciding domain** and setting `grantor` from `domain_id`.
///
/// Returns `Ok(None)` (benign decline) when:
/// - `new_term_end <= now` — the payload carries a degenerate absolute
///   timestamp that would produce an inactive grant (silent no-op).
///   Fail closed rather than record a grant that never activates.
/// - no active in-domain grant exists for this grantee at `now` —
///   reconfirmation presupposes active authority. Declining closes
///   the stealth-reinstatement path (ReconfirmSteward on a revoked
///   steward) and the stealth-appointment path (ReconfirmSteward on
///   a never-granted DID). The callers for those cases are
///   ReinstateSteward and AppointSteward respectively.
///
/// Returns `Err(e)` when the backend read fails transiently. The
/// caller must propagate so the mandate is NOT recorded — otherwise
/// the idempotency check at the top of `mint_and_persist_for_accepted`
/// would short-circuit retries to `AlreadyMinted`, skipping the
/// lifecycle, and the accepted reconfirmation would never actually
/// refresh authority.
///
/// The cloned scope mirrors `mint_reinstatement_grant` so a refresh
/// preserves the prior grant's authority shape rather than
/// fabricating a narrower/broader scope from the payload alone.
///
/// The prior active grant is NOT revoked here — see the comment on
/// the `ReconfirmSteward` lifecycle arm for the retry-safety rationale.
fn mint_reconfirmation_grant(
    backend: &dyn GovernanceReceiptBackend,
    steward: &icn_identity::Did,
    domain_id: &GovernanceDomainId,
    decision: &DecisionProvenance,
    now: Timestamp,
    new_term_end: Timestamp,
) -> Result<Option<AuthorityGrant>, String> {
    if new_term_end <= now {
        tracing::warn!(
            grantee = %steward,
            proposal_id = %decision.proposal_id,
            now,
            new_term_end,
            "ReconfirmSteward: new_term_end is at or before now; declining to mint (would be inactive on creation)"
        );
        return Ok(None);
    }

    let grantee = Grantee::Person(steward.clone());
    let actives = backend
        .list_active_authority_grants_by_grantee(&grantee, now)
        .map_err(|e| {
            tracing::error!(
                grantee = ?grantee,
                error = %e,
                "list_active_authority_grants_by_grantee failed; aborting acceptance before mandate write"
            );
            e
        })?;
    // Restrict candidates to grants issued by THIS deciding domain.
    // A cross-domain active grant does not authorize this domain to
    // refresh it — the grantor must match.
    let template = actives
        .iter()
        .filter(|g| g.grantor.0 == domain_id.0)
        .next_back();
    let template = match template {
        Some(t) => t.clone(),
        None => {
            tracing::warn!(
                grantee = ?grantee,
                proposal_id = %decision.proposal_id,
                deciding_domain = %domain_id.0,
                "ReconfirmSteward: no active in-domain grant found; declining to mint (reconfirmation requires an active grant issued by this domain — use AppointSteward for first-time authority, ReinstateSteward for a revoked template)"
            );
            return Ok(None);
        }
    };

    let scope = template.scope.clone();

    Ok(Some(AuthorityGrant {
        id: AuthorityGrantId::new(),
        class: template.class,
        grantor: GrantorEntityId(domain_id.0.clone()),
        grantee: Grantee::Person(steward.clone()),
        scope,
        granted_by: Some(decision.clone()),
        valid_from: now,
        valid_until: Some(new_term_end),
        revoked_at: None,
    }))
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
    fn reconfirm_steward_is_not_a_pure_derive_path() {
        // ReconfirmSteward now requires backend/history access (it is
        // lifecycle, not pure derive). The pure derive path must
        // return an empty vec; the real behavior is exercised via
        // `mint_and_persist_for_accepted` against a backend.
        let payload = ProposalPayload::Sdis {
            proposal: SdisProposal::ReconfirmSteward {
                steward: did(3),
                new_term_end: 5_000_000,
                performance_notes: None,
            },
        };
        let grants = derive_grants_for_accepted_proposal(&payload, &domain(), &decision(), 1_000);
        assert!(
            grants.is_empty(),
            "ReconfirmSteward must not mint from the pure derive path — needs history lookup"
        );
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
        /// One-shot fault-injection switch: when `true`, the next call
        /// to `revoke_authority_grant` returns a transient backend
        /// error and clears the flag. Used to simulate a revocation
        /// write failure and verify the acceptance seam propagates it.
        fail_next_revoke: std::sync::atomic::AtomicBool,
        /// One-shot fault-injection switch: when `true`, the next call
        /// to `list_authority_grants_by_grantee` returns a transient
        /// backend error and clears the flag. Used to verify
        /// reinstatement read-path errors propagate and abort the
        /// acceptance seam before the mandate write.
        fail_next_list_all: std::sync::atomic::AtomicBool,
        /// One-shot fault-injection switch: when `true`, the next call
        /// to `list_active_authority_grants_by_grantee` returns a
        /// transient backend error and clears the flag. Used to verify
        /// reconfirmation read-path errors propagate and abort the
        /// acceptance seam before the mandate write.
        fail_next_list_active: std::sync::atomic::AtomicBool,
    }

    impl InMemoryGrantBackend {
        fn new() -> Self {
            Self {
                mandates: std::sync::Mutex::new(vec![]),
                grants: std::sync::Mutex::new(vec![]),
                fail_next_revoke: std::sync::atomic::AtomicBool::new(false),
                fail_next_list_all: std::sync::atomic::AtomicBool::new(false),
                fail_next_list_active: std::sync::atomic::AtomicBool::new(false),
            }
        }

        fn arm_next_revoke_failure(&self) {
            self.fail_next_revoke
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }

        fn arm_next_list_all_failure(&self) {
            self.fail_next_list_all
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }

        fn arm_next_list_active_failure(&self) {
            self.fail_next_list_active
                .store(true, std::sync::atomic::Ordering::SeqCst);
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
            if self
                .fail_next_list_active
                .swap(false, std::sync::atomic::Ordering::SeqCst)
            {
                return Err("transient_backend_error: simulated list_active failure".into());
            }
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
            if self
                .fail_next_list_all
                .swap(false, std::sync::atomic::Ordering::SeqCst)
            {
                return Err("transient_backend_error: simulated list_all failure".into());
            }
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
            if self
                .fail_next_revoke
                .swap(false, std::sync::atomic::Ordering::SeqCst)
            {
                return Err("transient_backend_error: simulated revoke failure".into());
            }
            let mut guard = self.grants.lock().unwrap();
            let Some(g) = guard.iter_mut().find(|g| &g.id == grant_id) else {
                return Err(format!("grant_not_found: {grant_id}"));
            };
            // Monotonic-minimum idempotency, matching the ReceiptStore
            // sled-backed semantics: the earliest effective revocation
            // wins. A retry at a later-or-equal timestamp is a no-op;
            // a strictly-earlier timestamp tightens termination.
            match g.revoked_at {
                Some(existing) if revoked_at >= existing => {}
                _ => g.revoked_at = Some(revoked_at),
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
        // write failure) must not move the original `revoked_at`
        // forward. The seam-level idempotency check short-circuits on
        // the already-minted mandate, and even if it didn't, the
        // backend's monotonic-minimum revocation rule would reject the
        // later-timestamp retry.
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
        // backend's monotonic-minimum would keep revoked_at at 2_500
        // (the later retry at 9_999 cannot loosen termination).
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
    fn revoke_authority_revokes_grants_minted_after_effective_at_but_before_acceptance() {
        // Candidate selection must happen at acceptance time, not at
        // `effective_at`. Otherwise a grant minted during the grace
        // window would remain active forever after an accepted
        // RevokeAuthority decision.
        let backend = InMemoryGrantBackend::new();
        let dom = domain();
        let authority = did(41);

        mint_and_persist_for_accepted(
            &backend,
            "prop-appoint-authority-late",
            &dom,
            [0xf3u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::AppointSteward {
                    candidate: authority.clone(),
                    sponsors: vec![did(1)],
                    region: "nyc".into(),
                    bond_amount: 100,
                    term_length: 10_000,
                },
            },
            5_800,
        )
        .unwrap();

        // Grace-period revocation accepted after the grant was minted.
        mint_and_persist_for_accepted(
            &backend,
            "prop-revoke-authority-late",
            &dom,
            [0xf4u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::RevokeAuthority {
                    authority_did: authority.clone(),
                    reason: "decertified".into(),
                    effective_at: Some(5_555),
                },
            },
            6_000,
        )
        .unwrap();

        let all = backend
            .list_authority_grants_by_grantee(&Grantee::Person(authority.clone()))
            .unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(
            all[0].revoked_at,
            Some(5_555),
            "accepted revocation must stamp payload effective_at even for grants discovered at acceptance time"
        );

        let active_after_acceptance = backend
            .list_active_authority_grants_by_grantee(&Grantee::Person(authority), 6_000)
            .unwrap();
        assert!(
            active_after_acceptance.is_empty(),
            "grant minted during the grace window must still be revoked once the decision is accepted"
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

    #[test]
    fn reinstate_steward_declines_when_target_is_active() {
        // Reinstating an already-active steward must be a no-op, not a
        // fresh-grant mint. The common-case bug: repeated or erroneous
        // ReinstateSteward proposals must NOT silently duplicate or
        // extend privileges.
        let backend = InMemoryGrantBackend::new();
        let dom = domain();
        let steward = did(50);

        // Appoint — steward is active; there are no revoked grants.
        mint_and_persist_for_accepted(
            &backend,
            "prop-appoint",
            &dom,
            [0x51u8; 32],
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

        // Reinstate while still active — must decline.
        let outcome = mint_and_persist_for_accepted(
            &backend,
            "prop-reinstate-active",
            &dom,
            [0x52u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::ReinstateSteward {
                    steward: steward.clone(),
                    reason: "redundant appeal".into(),
                },
            },
            2_000,
        )
        .unwrap();

        match outcome {
            MandateMintOutcome::Minted {
                grants_persisted, ..
            } => assert_eq!(
                grants_persisted, 0,
                "reinstatement of an active steward must NOT mint a new grant"
            ),
            other => panic!("expected Minted (pending-grants); got {other:?}"),
        }

        let all = backend
            .list_authority_grants_by_grantee(&Grantee::Person(steward))
            .unwrap();
        assert_eq!(all.len(), 1, "exactly the original appointment grant");
        assert!(
            all[0].revoked_at.is_none(),
            "original grant remains active and untouched"
        );
    }

    #[test]
    fn reinstate_steward_declines_when_only_cross_domain_revoked_template_exists() {
        // Reinstatement must NOT clone a template from another domain.
        // Domain B revoked the steward. Domain A (which never issued a
        // grant to this steward) accepts a ReinstateSteward — must
        // decline rather than import domain B's shape/bounds.
        let backend = InMemoryGrantBackend::new();
        let steward = did(51);
        let domain_a = GovernanceDomainId("coop:alpha".into());
        let domain_b = GovernanceDomainId("coop:beta".into());

        // Appoint + revoke under domain B only.
        mint_and_persist_for_accepted(
            &backend,
            "prop-appoint-b",
            &domain_b,
            [0x60u8; 32],
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
        mint_and_persist_for_accepted(
            &backend,
            "prop-remove-b",
            &domain_b,
            [0x61u8; 32],
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

        // Domain A tries to reinstate — no in-domain template exists.
        let outcome = mint_and_persist_for_accepted(
            &backend,
            "prop-reinstate-a",
            &domain_a,
            [0x62u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::ReinstateSteward {
                    steward: steward.clone(),
                    reason: "cross-domain import attempt".into(),
                },
            },
            3_000,
        )
        .unwrap();

        match outcome {
            MandateMintOutcome::Minted {
                grants_persisted, ..
            } => assert_eq!(
                grants_persisted, 0,
                "cross-domain reinstatement without an in-domain template must decline"
            ),
            other => panic!("expected Minted (pending-grants); got {other:?}"),
        }

        // Still only the domain_b grant exists (revoked); no domain_a grant minted.
        let all = backend
            .list_authority_grants_by_grantee(&Grantee::Person(steward))
            .unwrap();
        assert_eq!(
            all.len(),
            1,
            "no fresh grant should be minted — domain_b's revoked grant remains as the only record"
        );
        assert_eq!(all[0].grantor, GrantorEntityId(domain_b.0));
        assert_eq!(all[0].revoked_at, Some(2_000));
    }

    #[test]
    fn sanction_steward_probation_revokes_active_grants() {
        // Probation maps to SdisEffect::SuspendSteward in
        // handlers/execution.rs. Grant lifecycle must match: leaving
        // grants active while the execution layer treats the steward
        // as suspended would split operational state from the grant
        // store. Probation therefore revokes the target's active
        // in-domain grants, the same way an explicit Suspension does.
        use icn_governance::sdis::StewardPenalty;

        let backend = InMemoryGrantBackend::new();
        let dom = domain();
        let steward = did(70);

        mint_and_persist_for_accepted(
            &backend,
            "prop-appoint-probation",
            &dom,
            [0x70u8; 32],
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

        mint_and_persist_for_accepted(
            &backend,
            "prop-probation",
            &dom,
            [0x71u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::SanctionSteward {
                    steward: steward.clone(),
                    penalty: StewardPenalty::Probation {
                        duration: 86_400,
                        conditions: vec!["weekly check-in".into()],
                    },
                    reason: "conduct review".into(),
                    evidence: vec![],
                },
            },
            2_000,
        )
        .unwrap();

        let all = backend
            .list_authority_grants_by_grantee(&Grantee::Person(steward))
            .unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(
            all[0].revoked_at,
            Some(2_000),
            "Probation must revoke active in-domain grants (execution layer treats it as SuspendSteward)"
        );
    }

    #[test]
    fn reinstate_steward_declines_when_active_in_domain_grant_exists_after_prior_cycle() {
        // After a normal Remove→Reinstate cycle, grant history is
        // [revoked_old, active_new]. A duplicate ReinstateSteward on
        // the same steward must NOT mint a second active grant — the
        // revoked-filter would otherwise still select `revoked_old`
        // as a template. The active-in-domain precheck guards this.
        let backend = InMemoryGrantBackend::new();
        let dom = domain();
        let steward = did(71);

        // Appoint.
        mint_and_persist_for_accepted(
            &backend,
            "prop-appoint-cycle",
            &dom,
            [0x80u8; 32],
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

        // Remove → first grant is revoked.
        mint_and_persist_for_accepted(
            &backend,
            "prop-remove-cycle",
            &dom,
            [0x81u8; 32],
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

        // First Reinstate → mints a fresh active grant (valid cycle).
        mint_and_persist_for_accepted(
            &backend,
            "prop-reinstate-1",
            &dom,
            [0x82u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::ReinstateSteward {
                    steward: steward.clone(),
                    reason: "appeal upheld".into(),
                },
            },
            3_000,
        )
        .unwrap();

        let after_first = backend
            .list_authority_grants_by_grantee(&Grantee::Person(steward.clone()))
            .unwrap();
        assert_eq!(
            after_first.len(),
            2,
            "history after first cycle: 1 revoked + 1 active"
        );
        assert!(after_first.iter().any(|g| g.revoked_at.is_some()));
        assert!(after_first.iter().any(|g| g.revoked_at.is_none()));

        // Second (duplicate/erroneous) Reinstate while an active
        // in-domain grant already exists — must decline to mint.
        let outcome = mint_and_persist_for_accepted(
            &backend,
            "prop-reinstate-2",
            &dom,
            [0x83u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::ReinstateSteward {
                    steward: steward.clone(),
                    reason: "duplicate appeal".into(),
                },
            },
            4_000,
        )
        .unwrap();

        match outcome {
            MandateMintOutcome::Minted {
                grants_persisted, ..
            } => assert_eq!(
                grants_persisted, 0,
                "duplicate reinstatement while an in-domain active grant exists must decline"
            ),
            other => panic!("expected Minted (pending-grants); got {other:?}"),
        }

        let after_second = backend
            .list_authority_grants_by_grantee(&Grantee::Person(steward))
            .unwrap();
        assert_eq!(
            after_second.len(),
            2,
            "still exactly 1 revoked + 1 active; no second active grant minted"
        );
    }

    #[test]
    fn remove_steward_revocation_write_failure_aborts_acceptance_and_retry_succeeds() {
        // If a revocation write fails, the mandate MUST NOT be recorded:
        // otherwise the idempotency check at the top of
        // `mint_and_persist_for_accepted` short-circuits retries to
        // `AlreadyMinted`, the lifecycle never re-runs, and the target
        // grant stays active indefinitely despite an accepted
        // revocation decision.
        //
        // Walk: appoint → arm one-shot revoke failure → RemoveSteward
        // (must Err, no mandate recorded, grant still active) → retry
        // (must succeed, mandate recorded, grant revoked).
        let backend = InMemoryGrantBackend::new();
        let dom = domain();
        let steward = did(80);

        mint_and_persist_for_accepted(
            &backend,
            "prop-appoint-fail",
            &dom,
            [0x90u8; 32],
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

        // Arm one-shot failure and attempt revocation — must Err.
        backend.arm_next_revoke_failure();
        let err = mint_and_persist_for_accepted(
            &backend,
            "prop-remove-fail",
            &dom,
            [0x91u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::RemoveSteward {
                    steward: steward.clone(),
                    reason: "breach".into(),
                    return_bond: false,
                },
            },
            2_000,
        )
        .expect_err("revocation write failure must propagate, not record mandate");
        assert!(
            err.contains("transient_backend_error"),
            "propagated error should surface the backend failure cause, got: {err}"
        );

        // Grant still active — revocation did not land.
        let after_fail = backend
            .list_authority_grants_by_grantee(&Grantee::Person(steward.clone()))
            .unwrap();
        assert_eq!(after_fail.len(), 1);
        assert!(
            after_fail[0].revoked_at.is_none(),
            "grant must remain active when revocation write failed"
        );

        // Mandate must NOT be recorded — otherwise retry would hit the
        // idempotency short-circuit and skip the lifecycle forever.
        assert!(
            backend
                .get_mandate_by_proposal("prop-remove-fail")
                .unwrap()
                .is_none(),
            "no mandate may be recorded when a revocation write failed"
        );

        // Retry same proposal_id — flag is cleared, revocation lands,
        // mandate records.
        mint_and_persist_for_accepted(
            &backend,
            "prop-remove-fail",
            &dom,
            [0x91u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::RemoveSteward {
                    steward: steward.clone(),
                    reason: "breach".into(),
                    return_bond: false,
                },
            },
            2_500,
        )
        .expect("retry after transient failure must succeed");

        let after_retry = backend
            .list_authority_grants_by_grantee(&Grantee::Person(steward))
            .unwrap();
        assert_eq!(after_retry.len(), 1);
        assert_eq!(
            after_retry[0].revoked_at,
            Some(2_500),
            "retry must revoke at its own `now`"
        );
        assert!(
            backend
                .get_mandate_by_proposal("prop-remove-fail")
                .unwrap()
                .is_some(),
            "mandate must be recorded on the successful retry"
        );
    }

    #[test]
    fn reinstate_steward_list_read_failure_aborts_acceptance_and_retry_succeeds() {
        // Regression for the mint-helper read-failure path: if
        // `list_authority_grants_by_grantee` fails transiently during
        // `mint_reinstatement_grant`, the helper previously returned
        // `None` which the lifecycle arm mapped to `Ok(Vec::new())`.
        // That caused the mandate to still be recorded (as pending-
        // grants), and retries then short-circuited on the idempotency
        // check, so the accepted ReinstateSteward never actually minted
        // authority. The read failure must now propagate, abort the
        // mandate write, and let the retry recover.
        let backend = InMemoryGrantBackend::new();
        let dom = domain();
        let steward = did(82);

        // Appoint and revoke to produce a revoked in-domain template.
        mint_and_persist_for_accepted(
            &backend,
            "prop-appoint-reinstate-read-fail",
            &dom,
            [0x93u8; 32],
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
        mint_and_persist_for_accepted(
            &backend,
            "prop-remove-reinstate-read-fail",
            &dom,
            [0x94u8; 32],
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

        // Arm one-shot list-all failure and attempt reinstatement —
        // must Err, no mandate recorded.
        backend.arm_next_list_all_failure();
        let err = mint_and_persist_for_accepted(
            &backend,
            "prop-reinstate-read-fail",
            &dom,
            [0x95u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::ReinstateSteward {
                    steward: steward.clone(),
                    reason: "appeal upheld".into(),
                },
            },
            3_000,
        )
        .expect_err("list read failure must propagate, not record mandate");
        assert!(
            err.contains("transient_backend_error"),
            "propagated error should surface the backend failure cause, got: {err}"
        );

        assert!(
            backend
                .get_mandate_by_proposal("prop-reinstate-read-fail")
                .unwrap()
                .is_none(),
            "no mandate may be recorded when the reinstatement read failed \
             — otherwise the retry would short-circuit on AlreadyMinted"
        );

        // Retry — flag is cleared, reinstatement mints fresh grant.
        mint_and_persist_for_accepted(
            &backend,
            "prop-reinstate-read-fail",
            &dom,
            [0x95u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::ReinstateSteward {
                    steward: steward.clone(),
                    reason: "appeal upheld".into(),
                },
            },
            3_500,
        )
        .expect("retry after transient read failure must succeed");

        let after_retry = backend
            .list_authority_grants_by_grantee(&Grantee::Person(steward))
            .unwrap();
        let active: Vec<_> = after_retry
            .iter()
            .filter(|g| g.is_active_at(3_500))
            .collect();
        assert_eq!(
            active.len(),
            1,
            "retry must materialize exactly one active grant"
        );
        assert_eq!(active[0].valid_from, 3_500);
        assert!(
            backend
                .get_mandate_by_proposal("prop-reinstate-read-fail")
                .unwrap()
                .is_some(),
            "mandate must be recorded on the successful retry"
        );
    }

    #[test]
    fn reconfirm_steward_list_read_failure_aborts_acceptance_and_retry_succeeds() {
        // Same regression class as the reinstatement read-failure test
        // above, but for the reconfirmation arm:
        // `list_active_authority_grants_by_grantee` failure during
        // `mint_reconfirmation_grant` must propagate, abort the
        // mandate write, and let the retry recover.
        let backend = InMemoryGrantBackend::new();
        let dom = domain();
        let steward = did(83);

        mint_and_persist_for_accepted(
            &backend,
            "prop-appoint-reconfirm-read-fail",
            &dom,
            [0x96u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::AppointSteward {
                    candidate: steward.clone(),
                    sponsors: vec![did(1)],
                    region: "nyc".into(),
                    bond_amount: 100,
                    term_length: 10_000,
                },
            },
            1_000,
        )
        .unwrap();

        backend.arm_next_list_active_failure();
        let err = mint_and_persist_for_accepted(
            &backend,
            "prop-reconfirm-read-fail",
            &dom,
            [0x97u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::ReconfirmSteward {
                    steward: steward.clone(),
                    new_term_end: 20_000,
                    performance_notes: None,
                },
            },
            2_000,
        )
        .expect_err("list_active read failure must propagate, not record mandate");
        assert!(
            err.contains("transient_backend_error"),
            "propagated error should surface the backend failure cause, got: {err}"
        );

        assert!(
            backend
                .get_mandate_by_proposal("prop-reconfirm-read-fail")
                .unwrap()
                .is_none(),
            "no mandate may be recorded when the reconfirmation read failed"
        );

        // Retry — flag is cleared, reconfirmation mints fresh grant.
        mint_and_persist_for_accepted(
            &backend,
            "prop-reconfirm-read-fail",
            &dom,
            [0x97u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::ReconfirmSteward {
                    steward: steward.clone(),
                    new_term_end: 20_000,
                    performance_notes: None,
                },
            },
            2_500,
        )
        .expect("retry after transient read failure must succeed");

        let all = backend
            .list_authority_grants_by_grantee(&Grantee::Person(steward))
            .unwrap();
        assert_eq!(
            all.len(),
            2,
            "reconfirmation retry appends a fresh grant alongside the original (no pre-revoke)"
        );
        let fresh = all
            .iter()
            .find(|g| g.valid_from == 2_500)
            .expect("fresh reconfirmation grant at retry `now` must exist");
        assert_eq!(fresh.valid_until, Some(20_000));
        assert!(fresh.revoked_at.is_none());
        assert!(
            backend
                .get_mandate_by_proposal("prop-reconfirm-read-fail")
                .unwrap()
                .is_some(),
            "mandate must be recorded on the successful retry"
        );
    }

    // ========================================================================
    // ReconfirmSteward history-aware lifecycle tests
    // ========================================================================

    #[test]
    fn reconfirm_steward_declines_when_steward_has_no_prior_grant() {
        // ReconfirmSteward on a DID with no in-domain history must
        // decline — this is the stealth-appointment path. First-time
        // authority belongs to AppointSteward.
        let backend = InMemoryGrantBackend::new();
        let dom = domain();
        let steward = did(90);

        let outcome = mint_and_persist_for_accepted(
            &backend,
            "prop-reconfirm-ghost",
            &dom,
            [0xA0u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::ReconfirmSteward {
                    steward: steward.clone(),
                    new_term_end: 10_000,
                    performance_notes: None,
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
                "reconfirmation without any prior in-domain grant must decline"
            ),
            other => panic!("expected Minted (pending-grants); got {other:?}"),
        }

        let all = backend
            .list_authority_grants_by_grantee(&Grantee::Person(steward))
            .unwrap();
        assert!(
            all.is_empty(),
            "no grant may be minted for a never-appointed steward"
        );
    }

    #[test]
    fn reconfirm_steward_declines_when_only_revoked_in_domain_grant_exists() {
        // The critical bug: ReconfirmSteward on a revoked steward was
        // a stealth-reinstatement path in the pure derive version.
        // Must decline — reinstatement of a revoked steward routes
        // through ReinstateSteward, which mints from a valid revoked
        // template under its own flow.
        let backend = InMemoryGrantBackend::new();
        let dom = domain();
        let steward = did(91);

        mint_and_persist_for_accepted(
            &backend,
            "prop-appoint-for-revoke",
            &dom,
            [0xA1u8; 32],
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

        mint_and_persist_for_accepted(
            &backend,
            "prop-remove-before-reconfirm",
            &dom,
            [0xA2u8; 32],
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

        // Attempt reconfirmation while the steward is revoked — must
        // decline rather than silently re-activate.
        let outcome = mint_and_persist_for_accepted(
            &backend,
            "prop-reconfirm-revoked",
            &dom,
            [0xA3u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::ReconfirmSteward {
                    steward: steward.clone(),
                    new_term_end: 10_000,
                    performance_notes: None,
                },
            },
            3_000,
        )
        .unwrap();

        match outcome {
            MandateMintOutcome::Minted {
                grants_persisted, ..
            } => assert_eq!(
                grants_persisted, 0,
                "reconfirmation of a revoked steward must decline (closes stealth-reinstatement path)"
            ),
            other => panic!("expected Minted (pending-grants); got {other:?}"),
        }

        let all = backend
            .list_authority_grants_by_grantee(&Grantee::Person(steward))
            .unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(
            all[0].revoked_at,
            Some(2_000),
            "revoked grant must remain revoked; no fresh active grant was minted"
        );
    }

    #[test]
    fn reconfirm_steward_declines_when_new_term_end_is_at_or_before_now() {
        // An absolute `new_term_end <= now` would record a grant that
        // is inactive on creation. Fail closed rather than record a
        // silent no-op grant.
        let backend = InMemoryGrantBackend::new();
        let dom = domain();
        let steward = did(92);

        mint_and_persist_for_accepted(
            &backend,
            "prop-appoint-92",
            &dom,
            [0xA4u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::AppointSteward {
                    candidate: steward.clone(),
                    sponsors: vec![did(1)],
                    region: "nyc".into(),
                    bond_amount: 100,
                    term_length: 10_000,
                },
            },
            1_000,
        )
        .unwrap();

        // new_term_end equal to now: must decline.
        let outcome = mint_and_persist_for_accepted(
            &backend,
            "prop-reconfirm-now-eq-term",
            &dom,
            [0xA5u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::ReconfirmSteward {
                    steward: steward.clone(),
                    new_term_end: 5_000,
                    performance_notes: None,
                },
            },
            5_000,
        )
        .unwrap();
        match outcome {
            MandateMintOutcome::Minted {
                grants_persisted, ..
            } => assert_eq!(grants_persisted, 0, "new_term_end == now must decline"),
            other => panic!("expected Minted; got {other:?}"),
        }

        // new_term_end strictly before now: must decline.
        let outcome = mint_and_persist_for_accepted(
            &backend,
            "prop-reconfirm-term-in-past",
            &dom,
            [0xA6u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::ReconfirmSteward {
                    steward: steward.clone(),
                    new_term_end: 4_000,
                    performance_notes: None,
                },
            },
            5_000,
        )
        .unwrap();
        match outcome {
            MandateMintOutcome::Minted {
                grants_persisted, ..
            } => assert_eq!(grants_persisted, 0, "new_term_end < now must decline"),
            other => panic!("expected Minted; got {other:?}"),
        }

        let all = backend
            .list_authority_grants_by_grantee(&Grantee::Person(steward))
            .unwrap();
        assert_eq!(
            all.len(),
            1,
            "only the original appointment grant exists; no degenerate reconfirmation grants were recorded"
        );
    }

    #[test]
    fn reconfirm_steward_mints_fresh_grant_when_active_in_domain_grant_exists() {
        // Happy path: steward has an active in-domain grant,
        // `new_term_end > now`. Mints one fresh grant with
        // `valid_from = now`, `valid_until = new_term_end`. The prior
        // grant is intentionally left to expire naturally — retry-
        // safety rationale is in the lifecycle arm comment.
        let backend = InMemoryGrantBackend::new();
        let dom = domain();
        let steward = did(93);

        mint_and_persist_for_accepted(
            &backend,
            "prop-appoint-93",
            &dom,
            [0xA7u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::AppointSteward {
                    candidate: steward.clone(),
                    sponsors: vec![did(1)],
                    region: "nyc".into(),
                    bond_amount: 100,
                    term_length: 10_000,
                },
            },
            1_000,
        )
        .unwrap();

        let outcome = mint_and_persist_for_accepted(
            &backend,
            "prop-reconfirm-happy",
            &dom,
            [0xA8u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::ReconfirmSteward {
                    steward: steward.clone(),
                    new_term_end: 50_000,
                    performance_notes: Some("solid term".into()),
                },
            },
            5_000,
        )
        .unwrap();

        match outcome {
            MandateMintOutcome::Minted {
                grants_persisted, ..
            } => assert_eq!(
                grants_persisted, 1,
                "happy-path reconfirm mints one fresh grant"
            ),
            other => panic!("expected Minted; got {other:?}"),
        }

        let all = backend
            .list_authority_grants_by_grantee(&Grantee::Person(steward))
            .unwrap();
        assert_eq!(
            all.len(),
            2,
            "history holds the appointment + the reconfirmation"
        );

        let refreshed = all
            .iter()
            .find(|g| g.valid_from == 5_000)
            .expect("refresh exists");
        assert_eq!(refreshed.valid_until, Some(50_000));
        assert_eq!(refreshed.class, AuthorityClass::Attestation);
        assert_eq!(refreshed.grantor, GrantorEntityId("coop:tech".into()));
        assert!(refreshed.revoked_at.is_none());

        let original = all
            .iter()
            .find(|g| g.valid_from == 1_000)
            .expect("original exists");
        assert!(
            original.revoked_at.is_none(),
            "prior grant is intentionally left active — retry-safety over pre-revoke"
        );
        assert_eq!(
            original.valid_until,
            Some(11_000),
            "original term unchanged"
        );
    }

    #[test]
    fn reconfirm_steward_declines_when_only_cross_domain_active_grant_exists() {
        // A cross-domain active grant does not authorize this domain
        // to refresh. Only an active grant whose grantor matches the
        // deciding domain counts.
        let backend = InMemoryGrantBackend::new();
        let steward = did(94);
        let domain_a = GovernanceDomainId("coop:alpha".into());
        let domain_b = GovernanceDomainId("coop:beta".into());

        // Domain B appoints the steward.
        mint_and_persist_for_accepted(
            &backend,
            "prop-appoint-b-94",
            &domain_b,
            [0xA9u8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::AppointSteward {
                    candidate: steward.clone(),
                    sponsors: vec![did(2)],
                    region: "nyc".into(),
                    bond_amount: 100,
                    term_length: 10_000,
                },
            },
            1_000,
        )
        .unwrap();

        // Domain A attempts to reconfirm — must decline (domain A has
        // no grant to refresh).
        let outcome = mint_and_persist_for_accepted(
            &backend,
            "prop-reconfirm-cross-domain",
            &domain_a,
            [0xAAu8; 32],
            &ProposalPayload::Sdis {
                proposal: SdisProposal::ReconfirmSteward {
                    steward: steward.clone(),
                    new_term_end: 50_000,
                    performance_notes: None,
                },
            },
            2_000,
        )
        .unwrap();

        match outcome {
            MandateMintOutcome::Minted {
                grants_persisted, ..
            } => assert_eq!(
                grants_persisted, 0,
                "cross-domain reconfirmation must decline"
            ),
            other => panic!("expected Minted; got {other:?}"),
        }

        let all = backend
            .list_authority_grants_by_grantee(&Grantee::Person(steward))
            .unwrap();
        assert_eq!(all.len(), 1, "only domain_b's original grant exists");
        assert_eq!(all[0].grantor, GrantorEntityId(domain_b.0));
    }
}
