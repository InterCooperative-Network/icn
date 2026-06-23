//! Gate-wired domain-policy adoption (#2142 follow-up to #2162).
//!
//! [`icn_governance::InstitutionalDomain::adopt_policy`] is a pure, storage-
//! agnostic **structural** check: it proves a presented
//! [`Mandate`](icn_governance::Mandate) is single,
//! live, and grant-bearing, but it does not resolve a grant's
//! [`TypedScope`](icn_governance::TypedScope)`.domain` against the target
//! domain, learn who the actor is, or consult the grant store. Authority
//! *resolution* lives app-side in [`crate::mandate_gate`].
//!
//! This module is the thin seam that wires the two together:
//!
//! ```text
//! adopt request (actor, domain, policy, now)
//!   → DefaultMandateGate::require()   // actor → active grants → TypedScope.domain
//!                                       //   + class + act-token + mandate lifecycle
//!   → InstitutionalDomain::adopt_policy()  // structural commit (defense-in-depth)
//! ```
//!
//! The gate is the real [`DefaultMandateGate`] over the existing
//! [`GovernanceReceiptBackend`]; no resolver logic is duplicated here, and
//! `icn-governance` never imports the gate. The meaning firewall stays intact:
//! `icn-governance` holds pure types + structural validation; this crate
//! (`apps/governance`) holds authority resolution.
//!
//! Scope (smallest honest slice): policy *adoption* only. It does **not** add a
//! new authority primitive, a CCL runtime, a policy registry, a service-binding
//! runtime, package activation, an HTTP surface, or any auth-model change.

use std::sync::Arc;

use icn_governance::Timestamp;
use icn_governance::{
    DomainPolicy, DomainPolicyRef, InstitutionalDomain, InstitutionalDomainError,
};
use icn_identity::Did;

use crate::mandate_gate::{
    DefaultMandateGate, MandateAct, MandateGate, MandateGateError, MandateRequest, MandateTarget,
};
use crate::receipt_backend::GovernanceReceiptBackend;

/// Error from a gate-wired domain-policy adoption. Every variant leaves the
/// domain's policy state **unchanged** — adoption is fail-closed.
#[derive(Debug)]
pub enum AdoptDomainPolicyError {
    /// The app-side [`MandateGate`] refused authority: no authorizing grant, or
    /// a wrong actor / domain / act / class, or an expired or revoked grant or
    /// mandate.
    Unauthorized(MandateGateError),
    /// A backend read failed, or the gate-authorized mandate could not be
    /// recovered from the store (integrity skew). Fail-closed.
    Backend(String),
    /// The pure-core structural commit rejected the adoption (defense-in-depth).
    Core(InstitutionalDomainError),
}

impl std::fmt::Display for AdoptDomainPolicyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unauthorized(e) => write!(f, "domain policy adoption unauthorized: {e}"),
            Self::Backend(e) => write!(f, "domain policy adoption backend error: {e}"),
            Self::Core(e) => write!(f, "domain policy adoption rejected: {e}"),
        }
    }
}

impl std::error::Error for AdoptDomainPolicyError {}

/// Adopt `policy` as `domain`'s current policy, gated by the app-side
/// [`DefaultMandateGate`] over `backend`.
///
/// Resolves whether `actor` holds an active, domain-scoped [`Execution`] grant
/// that binds the `domain_policy:adopt` act — via the **real** gate resolver
/// (actor → active grants → `TypedScope.domain` + class + act-token + mandate
/// lifecycle) — then commits through the pure-core
/// [`InstitutionalDomain::adopt_policy`] as defense-in-depth. Fails closed on
/// any gate rejection, backend read failure, or structural rejection, leaving
/// `domain` unchanged.
///
/// This adds no new authority primitive and duplicates no resolver logic: the
/// `domain_policy:adopt` act token, `MandateTarget::Domain`, and the
/// `DefaultMandateGate` are the existing app-side machinery.
///
/// [`Execution`]: icn_governance::AuthorityClass::Execution
pub fn adopt_domain_policy_gated(
    backend: Arc<dyn GovernanceReceiptBackend>,
    domain: &mut InstitutionalDomain,
    policy: &DomainPolicy,
    actor: &Did,
    at: Timestamp,
) -> Result<DomainPolicyRef, AdoptDomainPolicyError> {
    let gate = DefaultMandateGate::new(Arc::clone(&backend));
    let req = MandateRequest {
        actor: actor.clone(),
        domain: domain.domain_id.clone(),
        act: MandateAct::AdoptDomainPolicy,
        target: MandateTarget::Domain(domain.domain_id.clone()),
        at,
    };

    let grant = gate
        .require(&req)
        .map_err(AdoptDomainPolicyError::Unauthorized)?;

    // Recover the mandate the gate just authorized, to feed the pure-core
    // structural commit. The gate resolved it from this same store moments ago,
    // so a miss here is backend integrity skew, not "no authority".
    let mandate = backend
        .list_mandates_by_decision(&grant.decision_hash)
        .map_err(AdoptDomainPolicyError::Backend)?
        .into_iter()
        .find(|m| m.id == grant.mandate_id)
        .ok_or_else(|| {
            AdoptDomainPolicyError::Backend(format!(
                "gate-authorized mandate {} not recoverable from backend",
                grant.mandate_id
            ))
        })?;

    domain
        .adopt_policy(policy, std::slice::from_ref(&mandate), at)
        .map_err(AdoptDomainPolicyError::Core)
}

/// Error from the [`GovernanceManager`](crate::manager::GovernanceManager)
/// domain-policy adoption seam.
#[derive(Debug)]
pub enum DomainPolicyAdoptionError {
    /// The manager has no receipt/grant backend wired, so authority cannot be
    /// resolved. Fail closed — a manager that cannot resolve authority must
    /// never allow adoption.
    MissingReceiptBackend,
    /// The gated adoption was refused (unauthorized, backend read failure, or
    /// pure-core structural rejection).
    Gated(AdoptDomainPolicyError),
}

impl std::fmt::Display for DomainPolicyAdoptionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingReceiptBackend => write!(
                f,
                "domain policy adoption fail-closed: no receipt backend wired"
            ),
            Self::Gated(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for DomainPolicyAdoptionError {}

impl crate::manager::GovernanceManager {
    /// Adopt `policy` as `domain`'s current policy, resolving authority through
    /// this manager's wired [`GovernanceReceiptBackend`] and the app-side
    /// [`DefaultMandateGate`].
    ///
    /// Fails closed with [`DomainPolicyAdoptionError::MissingReceiptBackend`]
    /// when no backend is wired; otherwise delegates to
    /// [`adopt_domain_policy_gated`], mutating the caller-held `domain` only on
    /// success and returning the adopted [`DomainPolicyRef`].
    ///
    /// **Persistence note:** there is no `InstitutionalDomain` store yet, so
    /// this seam operates on a caller-held domain; durable persistence of the
    /// adopted policy is a later manager/domain-store lane. This method adds no
    /// HTTP surface and changes no auth model — it composes the existing gate
    /// and the pure-core structural commit behind the governance-app boundary.
    pub fn adopt_domain_policy(
        &self,
        domain: &mut InstitutionalDomain,
        policy: &DomainPolicy,
        actor: &Did,
        now: Timestamp,
    ) -> Result<DomainPolicyRef, DomainPolicyAdoptionError> {
        let backend = self
            .receipt_backend()
            .ok_or(DomainPolicyAdoptionError::MissingReceiptBackend)?;
        adopt_domain_policy_gated(backend, domain, policy, actor, now)
            .map_err(DomainPolicyAdoptionError::Gated)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use crate::manager::GovernanceManager;
    use crate::mandate_gate::MandateRejection;
    use icn_governance::{
        AuthorityClass, AuthorityGrant, AuthorityGrantId, BootstrapEntityType, DecisionProvenance,
        GovernanceDecisionReceipt, GovernanceDomainId, Grantee, GrantorEntityId, Mandate,
        MandateStatus, TypedScope,
    };
    use icn_kernel_api::{AllocationReceipt, Hash};

    // ----- fixtures ---------------------------------------------------------

    /// Minimal in-memory backend seeded with pre-built mandates and grants.
    /// Implements only the methods the gate + adopter read; the rest are
    /// trivial stubs (mirrors `mandate_gate::tests::FixtureBackend`).
    struct TestBackend {
        mandates: Vec<Mandate>,
        grants: Vec<AuthorityGrant>,
    }

    impl TestBackend {
        fn new(mandates: Vec<Mandate>, grants: Vec<AuthorityGrant>) -> Arc<Self> {
            Arc::new(Self { mandates, grants })
        }
    }

    impl GovernanceReceiptBackend for TestBackend {
        fn put_governance(&self, _: &GovernanceDecisionReceipt) -> Result<(), String> {
            Ok(())
        }
        fn get_governance_by_proposal(
            &self,
            _: &str,
        ) -> Result<Option<GovernanceDecisionReceipt>, String> {
            Ok(None)
        }
        fn put_allocation(&self, _: &AllocationReceipt) -> Result<Hash, String> {
            Ok([0u8; 32])
        }
        fn get_governance_by_decision(
            &self,
            _: &Hash,
        ) -> Result<Option<GovernanceDecisionReceipt>, String> {
            Ok(None)
        }
        fn list_allocations_by_decision(&self, _: &Hash) -> Result<Vec<AllocationReceipt>, String> {
            Ok(vec![])
        }
        fn list_mandates_by_decision(&self, decision_hash: &Hash) -> Result<Vec<Mandate>, String> {
            Ok(self
                .mandates
                .iter()
                .filter(|m| &m.decision.decision_hash == decision_hash)
                .cloned()
                .collect())
        }
        fn get_authority_grant(
            &self,
            grant_id: &AuthorityGrantId,
        ) -> Result<Option<AuthorityGrant>, String> {
            Ok(self.grants.iter().find(|g| &g.id == grant_id).cloned())
        }
        fn list_active_authority_grants_by_grantee(
            &self,
            grantee: &Grantee,
            now: Timestamp,
        ) -> Result<Vec<AuthorityGrant>, String> {
            Ok(self
                .grants
                .iter()
                .filter(|g| &g.grantee == grantee && g.is_active_at(now))
                .cloned()
                .collect())
        }
    }

    fn did(seed: u8) -> Did {
        Did::from_anchor_id(&[seed; 32])
    }

    fn domain_id(s: &str) -> GovernanceDomainId {
        GovernanceDomainId::new(s)
    }

    fn decision() -> DecisionProvenance {
        DecisionProvenance {
            proposal_id: "prop-1".into(),
            decision_hash: [7u8; 32],
        }
    }

    /// An Execution grant for `actor`, bound to `dom`, carrying the
    /// `domain_policy:adopt` act token, provenance-linked to `decision()`.
    fn adopt_grant(actor: Did, dom: &str, grant_id: AuthorityGrantId) -> AuthorityGrant {
        AuthorityGrant {
            id: grant_id,
            class: AuthorityClass::Execution,
            grantor: GrantorEntityId("coop:alpha".into()),
            grantee: Grantee::Person(actor),
            scope: TypedScope {
                domain: Some(domain_id(dom)),
                action_kind: vec!["domain_policy:adopt".into()],
                ..TypedScope::default()
            },
            granted_by: Some(decision()),
            valid_from: 0,
            valid_until: Some(1000),
            revoked_at: None,
        }
    }

    /// A live mandate backing `grant_id` (provenance == `decision()`).
    fn backing_mandate(grant_id: AuthorityGrantId) -> Mandate {
        Mandate::new(decision(), [9u8; 32], vec![grant_id], None, None, 0)
            .expect("grant-bearing mandate is valid")
    }

    fn coop_alpha() -> InstitutionalDomain {
        InstitutionalDomain::declare(domain_id("coop-alpha"), BootstrapEntityType::Cooperative)
    }

    // ----- tests ------------------------------------------------------------

    #[test]
    fn adoption_succeeds_with_active_domain_scoped_grant() {
        let actor = did(1);
        let gid = AuthorityGrantId::new();
        let grant = adopt_grant(actor.clone(), "coop-alpha", gid.clone());
        let mandate = backing_mandate(gid);
        let backend = TestBackend::new(vec![mandate], vec![grant]);

        let mut domain = coop_alpha();
        let policy = DomainPolicy::new(domain_id("coop-alpha"), b"policy v1");

        let adopted = adopt_domain_policy_gated(backend, &mut domain, &policy, &actor, 100)
            .expect("an active domain-scoped adopt grant authorizes adoption");

        assert_eq!(adopted, policy.policy_ref());
        assert_eq!(domain.current_policy(), Some(&policy.policy_ref()));
        assert!(domain.has_adopted(&policy.policy_ref()));
    }

    #[test]
    fn rejects_grant_scoped_to_another_domain() {
        let actor = did(1);
        let gid = AuthorityGrantId::new();
        // Grant bound to coop-beta, but adoption target is coop-alpha.
        let grant = adopt_grant(actor.clone(), "coop-beta", gid.clone());
        let mandate = backing_mandate(gid);
        let backend = TestBackend::new(vec![mandate], vec![grant]);

        let mut domain = coop_alpha();
        let policy = DomainPolicy::new(domain_id("coop-alpha"), b"policy v1");

        let err = adopt_domain_policy_gated(backend, &mut domain, &policy, &actor, 100)
            .expect_err("a grant scoped to a different domain must be refused");
        assert!(matches!(
            err,
            AdoptDomainPolicyError::Unauthorized(MandateGateError::Rejected(
                MandateRejection::WrongTarget
            ))
        ));
        assert!(domain.current_policy().is_none());
    }

    #[test]
    fn rejects_wrong_actor() {
        let grantee = did(1);
        let gid = AuthorityGrantId::new();
        let grant = adopt_grant(grantee, "coop-alpha", gid.clone());
        let mandate = backing_mandate(gid);
        let backend = TestBackend::new(vec![mandate], vec![grant]);

        let mut domain = coop_alpha();
        let policy = DomainPolicy::new(domain_id("coop-alpha"), b"policy v1");

        // A different actor with no grant of their own.
        let err = adopt_domain_policy_gated(backend, &mut domain, &policy, &did(2), 100)
            .expect_err("an actor with no authorizing grant must be refused");
        assert!(matches!(
            err,
            AdoptDomainPolicyError::Unauthorized(MandateGateError::Rejected(
                MandateRejection::NoMandate
            ))
        ));
        assert!(domain.current_policy().is_none());
    }

    #[test]
    fn rejects_wrong_act_token() {
        let actor = did(1);
        let gid = AuthorityGrantId::new();
        let mut grant = adopt_grant(actor.clone(), "coop-alpha", gid.clone());
        // Right domain + class, but the grant authorizes a different act.
        grant.scope.action_kind = vec!["membership:add".into()];
        grant.scope.proposal_class = vec!["Membership".into()];
        let mandate = backing_mandate(gid);
        let backend = TestBackend::new(vec![mandate], vec![grant]);

        let mut domain = coop_alpha();
        let policy = DomainPolicy::new(domain_id("coop-alpha"), b"policy v1");

        let err = adopt_domain_policy_gated(backend, &mut domain, &policy, &actor, 100)
            .expect_err("a grant that does not bind the adopt act must be refused");
        assert!(matches!(
            err,
            AdoptDomainPolicyError::Unauthorized(MandateGateError::Rejected(
                MandateRejection::WrongTarget
            ))
        ));
        assert!(domain.current_policy().is_none());
    }

    #[test]
    fn rejects_expired_grant() {
        let actor = did(1);
        let gid = AuthorityGrantId::new();
        let mut grant = adopt_grant(actor.clone(), "coop-alpha", gid.clone());
        grant.valid_until = Some(50); // expired well before `at = 100`
        let mandate = backing_mandate(gid);
        let backend = TestBackend::new(vec![mandate], vec![grant]);

        let mut domain = coop_alpha();
        let policy = DomainPolicy::new(domain_id("coop-alpha"), b"policy v1");

        let err = adopt_domain_policy_gated(backend, &mut domain, &policy, &actor, 100)
            .expect_err("an expired grant must be refused");
        assert!(matches!(
            err,
            AdoptDomainPolicyError::Unauthorized(MandateGateError::Rejected(
                MandateRejection::NoMandate
            ))
        ));
        assert!(domain.current_policy().is_none());
    }

    #[test]
    fn rejects_revoked_mandate() {
        let actor = did(1);
        let gid = AuthorityGrantId::new();
        let grant = adopt_grant(actor.clone(), "coop-alpha", gid.clone());
        let mut mandate = backing_mandate(gid);
        mandate.status = MandateStatus::Revoked; // grant active, mandate dead
        let backend = TestBackend::new(vec![mandate], vec![grant]);

        let mut domain = coop_alpha();
        let policy = DomainPolicy::new(domain_id("coop-alpha"), b"policy v1");

        let err = adopt_domain_policy_gated(backend, &mut domain, &policy, &actor, 100)
            .expect_err("a revoked backing mandate must be refused");
        assert!(matches!(
            err,
            AdoptDomainPolicyError::Unauthorized(MandateGateError::Rejected(
                MandateRejection::Revoked
            ))
        ));
        assert!(domain.current_policy().is_none());
    }

    #[test]
    fn pure_core_adopt_policy_remains_defense_in_depth() {
        // The gate authorizes the actor for coop-alpha, but the policy object is
        // authored for a DIFFERENT domain. The app-side gate cannot catch this
        // (it resolves authority, not policy↔domain identity); the pure-core
        // `adopt_policy` structural check must still reject it.
        let actor = did(1);
        let gid = AuthorityGrantId::new();
        let grant = adopt_grant(actor.clone(), "coop-alpha", gid.clone());
        let mandate = backing_mandate(gid);
        let backend = TestBackend::new(vec![mandate], vec![grant]);

        let mut domain = coop_alpha();
        let foreign_policy = DomainPolicy::new(domain_id("coop-beta"), b"policy v1");

        let err = adopt_domain_policy_gated(backend, &mut domain, &foreign_policy, &actor, 100)
            .expect_err("a policy authored for another domain must be refused by the core check");
        assert!(matches!(
            err,
            AdoptDomainPolicyError::Core(InstitutionalDomainError::PolicyForOtherDomain)
        ));
        assert!(domain.current_policy().is_none());
    }

    // ----- GovernanceManager seam tests -------------------------------------

    fn manager_with(backend: Arc<TestBackend>) -> GovernanceManager {
        GovernanceManager::new().with_receipt_store(backend)
    }

    #[test]
    fn manager_adoption_succeeds_with_active_grant() {
        let actor = did(1);
        let gid = AuthorityGrantId::new();
        let grant = adopt_grant(actor.clone(), "coop-alpha", gid.clone());
        let mandate = backing_mandate(gid);
        let manager = manager_with(TestBackend::new(vec![mandate], vec![grant]));

        let mut domain = coop_alpha();
        let policy = DomainPolicy::new(domain_id("coop-alpha"), b"policy v1");

        let adopted = manager
            .adopt_domain_policy(&mut domain, &policy, &actor, 100)
            .expect("an active domain-scoped grant authorizes adoption via the manager");
        assert_eq!(adopted, policy.policy_ref());
        assert_eq!(domain.current_policy(), Some(&policy.policy_ref()));
    }

    #[test]
    fn manager_adoption_fails_closed_without_receipt_backend() {
        // A manager that cannot resolve authority must never silently allow.
        let manager = GovernanceManager::new();
        let mut domain = coop_alpha();
        let policy = DomainPolicy::new(domain_id("coop-alpha"), b"policy v1");

        let err = manager
            .adopt_domain_policy(&mut domain, &policy, &did(1), 100)
            .expect_err("a manager with no receipt backend must fail closed");
        assert!(matches!(
            err,
            DomainPolicyAdoptionError::MissingReceiptBackend
        ));
        assert!(domain.current_policy().is_none());
    }

    #[test]
    fn manager_adoption_rejects_wrong_actor() {
        let gid = AuthorityGrantId::new();
        let grant = adopt_grant(did(1), "coop-alpha", gid.clone());
        let mandate = backing_mandate(gid);
        let manager = manager_with(TestBackend::new(vec![mandate], vec![grant]));

        let mut domain = coop_alpha();
        let policy = DomainPolicy::new(domain_id("coop-alpha"), b"policy v1");

        let err = manager
            .adopt_domain_policy(&mut domain, &policy, &did(2), 100)
            .expect_err("an actor with no authorizing grant must be refused");
        assert!(matches!(
            err,
            DomainPolicyAdoptionError::Gated(AdoptDomainPolicyError::Unauthorized(
                MandateGateError::Rejected(MandateRejection::NoMandate)
            ))
        ));
        assert!(domain.current_policy().is_none());
    }

    #[test]
    fn manager_adoption_rejects_wrong_domain() {
        let actor = did(1);
        let gid = AuthorityGrantId::new();
        // Grant bound to coop-beta; adoption target is coop-alpha.
        let grant = adopt_grant(actor.clone(), "coop-beta", gid.clone());
        let mandate = backing_mandate(gid);
        let manager = manager_with(TestBackend::new(vec![mandate], vec![grant]));

        let mut domain = coop_alpha();
        let policy = DomainPolicy::new(domain_id("coop-alpha"), b"policy v1");

        let err = manager
            .adopt_domain_policy(&mut domain, &policy, &actor, 100)
            .expect_err("a grant scoped to another domain must be refused");
        assert!(matches!(
            err,
            DomainPolicyAdoptionError::Gated(AdoptDomainPolicyError::Unauthorized(
                MandateGateError::Rejected(MandateRejection::WrongTarget)
            ))
        ));
        assert!(domain.current_policy().is_none());
    }

    #[test]
    fn manager_adoption_rejects_revoked_authority() {
        let actor = did(1);
        let gid = AuthorityGrantId::new();
        let grant = adopt_grant(actor.clone(), "coop-alpha", gid.clone());
        let mut mandate = backing_mandate(gid);
        mandate.status = MandateStatus::Revoked;
        let manager = manager_with(TestBackend::new(vec![mandate], vec![grant]));

        let mut domain = coop_alpha();
        let policy = DomainPolicy::new(domain_id("coop-alpha"), b"policy v1");

        let err = manager
            .adopt_domain_policy(&mut domain, &policy, &actor, 100)
            .expect_err("a revoked backing mandate must be refused");
        assert!(matches!(
            err,
            DomainPolicyAdoptionError::Gated(AdoptDomainPolicyError::Unauthorized(
                MandateGateError::Rejected(MandateRejection::Revoked)
            ))
        ));
        assert!(domain.current_policy().is_none());
    }

    #[test]
    fn manager_adoption_preserves_pure_core_defense_in_depth() {
        // Gate authorizes the actor for coop-alpha, but the policy is authored
        // for coop-beta: the pure-core structural check must still reject it.
        let actor = did(1);
        let gid = AuthorityGrantId::new();
        let grant = adopt_grant(actor.clone(), "coop-alpha", gid.clone());
        let mandate = backing_mandate(gid);
        let manager = manager_with(TestBackend::new(vec![mandate], vec![grant]));

        let mut domain = coop_alpha();
        let foreign_policy = DomainPolicy::new(domain_id("coop-beta"), b"policy v1");

        let err = manager
            .adopt_domain_policy(&mut domain, &foreign_policy, &actor, 100)
            .expect_err("a policy authored for another domain must be refused by the core check");
        assert!(matches!(
            err,
            DomainPolicyAdoptionError::Gated(AdoptDomainPolicyError::Core(
                InstitutionalDomainError::PolicyForOtherDomain
            ))
        ));
        assert!(domain.current_policy().is_none());
    }
}
