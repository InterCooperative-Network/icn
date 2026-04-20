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

    // Derive zero or more grants. Empty vec is the truthful default for
    // most payload classes today.
    let grants = derive_grants_for_accepted_proposal(payload, domain_id, &decision_prov, now);
    let grant_ids: Vec<_> = grants.iter().map(|g| g.id.clone()).collect();

    // Persist grants **before** the mandate, then verify each write via
    // read-after-write. The defaulted-no-op `put_authority_grant`
    // returns `Ok(())` without storing anything; a backend that opts
    // into grant persistence must also override `get_authority_grant`.
    // If the read does not round-trip the grant, the backend does not
    // actually durably store grants — we must treat that grant as
    // unpersisted so the mandate falls back to `new_pending_grants`
    // rather than constructing a strict mandate referencing IDs that
    // cannot be retrieved. This is the smallest honest way to keep the
    // seam truthful without widening the trait surface.
    let mut persisted_grants: Vec<AuthorityGrant> = Vec::new();
    for grant in &grants {
        match backend.put_authority_grant(grant) {
            Ok(()) => match backend.get_authority_grant(&grant.id) {
                Ok(Some(_)) => persisted_grants.push(grant.clone()),
                Ok(None) => {
                    tracing::warn!(
                        proposal_id = %proposal_id,
                        grant_id = %grant.id,
                        "Backend does not durably persist authority grants (read-after-write returned None); mandate will fall back to pending-grants"
                    );
                }
                Err(e) => {
                    tracing::error!(
                        proposal_id = %proposal_id,
                        grant_id = %grant.id,
                        error = %e,
                        "Read-after-write verification failed for authority grant; treating as unpersisted"
                    );
                }
            },
            Err(e) => {
                tracing::error!(
                    proposal_id = %proposal_id,
                    grant_id = %grant.id,
                    error = %e,
                    "Failed to store authority grant — grant binding lost for this decision"
                );
            }
        }
    }

    let mandate = if persisted_grants.len() == grants.len() && !grant_ids.is_empty() {
        match Mandate::new(
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
                    "Mandate::new rejected derived grant set; falling back to pending-grants"
                );
                Mandate::new_pending_grants(decision_prov, payload_hash, None, None, now)
            }
        }
    } else {
        // Either the payload class mints no grants, or some grant writes
        // failed. Record the mandate as institutional memory, explicitly
        // unbound on authority.
        Mandate::new_pending_grants(decision_prov, payload_hash, None, None, now)
    };

    let mandate_id = mandate.id.clone();
    let grants_persisted = persisted_grants.len();
    backend.put_mandate(&mandate)?;

    Ok(MandateMintOutcome::Minted {
        mandate_id,
        grants_persisted,
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
        // do not mint new grants — they *revoke* existing authority.
        // Modeling that requires querying the mandate/grant store and
        // updating `revoked_at` on prior grants, which is explicit
        // future work. Returning an empty vec here is the truthful
        // answer: this decision does not create new authority.
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
}
