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
    BootstrapEntityType, CharterId, DomainPolicy, DomainPolicyRef, GovernanceDomainId,
    InstitutionalDomain, InstitutionalDomainError,
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

/// Error from the persisted `InstitutionalDomain` manager operations
/// (`declare_institutional_domain` / `adopt_domain_policy_persisted`).
#[derive(Debug)]
pub enum InstitutionalDomainStoreError {
    /// No [`crate::GovernanceStateStore`] is wired, so the `InstitutionalDomain` record
    /// cannot be loaded or persisted. Fail closed.
    MissingDomainStore,
    /// `declare`: an `InstitutionalDomain` is already declared for this
    /// `GovernanceDomainId`.
    AlreadyDeclared,
    /// `adopt`: no `InstitutionalDomain` has been declared for this
    /// `GovernanceDomainId`.
    NotDeclared,
    /// The backing store returned an error — including the fail-closed default
    /// for a `GovernanceStateStore` that does not implement institutional-domain
    /// persistence.
    Store(String),
    /// The gated adoption itself was refused (delegated to
    /// [`crate::manager::GovernanceManager::adopt_domain_policy`]).
    Adopt(DomainPolicyAdoptionError),
}

impl std::fmt::Display for InstitutionalDomainStoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingDomainStore => write!(
                f,
                "institutional domain persistence fail-closed: no governance state store wired"
            ),
            Self::AlreadyDeclared => {
                write!(
                    f,
                    "institutional domain already declared for this domain id"
                )
            }
            Self::NotDeclared => {
                write!(f, "no institutional domain declared for this domain id")
            }
            Self::Store(e) => write!(f, "institutional domain store error: {e}"),
            Self::Adopt(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for InstitutionalDomainStoreError {}

/// Error from the **gated** `InstitutionalDomain` declaration seam
/// ([`crate::manager::GovernanceManager::declare_institutional_domain_gated`]).
///
/// Declaring a governed domain is an authority-bearing act (ADR-0083): this
/// seam resolves that authority through the app-side [`DefaultMandateGate`]
/// before delegating to the bootstrap/in-process
/// [`crate::manager::GovernanceManager::declare_institutional_domain`]. Every
/// variant is fail-closed — nothing is persisted unless authority resolves.
#[derive(Debug)]
pub enum DeclareInstitutionalDomainError {
    /// The manager has no receipt/grant backend wired, so declaration authority
    /// cannot be resolved. Fail closed — a manager that cannot resolve authority
    /// must never declare a governed domain.
    MissingReceiptBackend,
    /// The app-side [`MandateGate`] refused authority: no authorizing grant, or
    /// a wrong actor / domain / act / class, or an expired or revoked grant or
    /// mandate.
    Unauthorized(MandateGateError),
    /// Authority resolved, but the underlying persisted declaration failed (no
    /// domain store wired, already declared, or a backend write error).
    Store(InstitutionalDomainStoreError),
}

impl std::fmt::Display for DeclareInstitutionalDomainError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingReceiptBackend => write!(
                f,
                "institutional domain declaration fail-closed: no receipt backend wired"
            ),
            Self::Unauthorized(e) => {
                write!(f, "institutional domain declaration unauthorized: {e}")
            }
            Self::Store(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for DeclareInstitutionalDomainError {}

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

    /// Declare and persist a new [`InstitutionalDomain`] authority record for an
    /// existing `GovernanceDomainId`, via the wired [`crate::GovernanceStateStore`].
    ///
    /// Fails closed with [`InstitutionalDomainStoreError::MissingDomainStore`]
    /// when no store is wired, and with [`InstitutionalDomainStoreError::AlreadyDeclared`]
    /// when a record already exists for `domain_id`. The declared domain is
    /// unbound (`current_policy == None`) until a later adoption.
    ///
    /// **Authority note — bootstrap / in-process only.** Declaring a governed
    /// domain is itself an authority-bearing act, and this seam does **not**
    /// resolve that authority: it persists unconditionally. It must **never** be
    /// wired to a routable surface. Any HTTP/network declare path must call the
    /// gate-resolving [`Self::declare_institutional_domain_gated`] instead;
    /// this ungated form is for bootstrap and in-process callers (including
    /// tests) where authority is established out of band. No HTTP route is added
    /// here.
    pub fn declare_institutional_domain(
        &self,
        domain_id: GovernanceDomainId,
        owning_entity_class: BootstrapEntityType,
        charter_ref: Option<CharterId>,
    ) -> Result<InstitutionalDomain, InstitutionalDomainStoreError> {
        let store = self
            .domain_state_store()
            .ok_or(InstitutionalDomainStoreError::MissingDomainStore)?;

        if store
            .get_institutional_domain(&domain_id)
            .map_err(|e| InstitutionalDomainStoreError::Store(e.to_string()))?
            .is_some()
        {
            return Err(InstitutionalDomainStoreError::AlreadyDeclared);
        }

        let mut domain = InstitutionalDomain::declare(domain_id, owning_entity_class);
        if let Some(charter) = charter_ref {
            domain = domain.with_charter(charter);
        }
        store
            .save_institutional_domain(&domain)
            .map_err(|e| InstitutionalDomainStoreError::Store(e.to_string()))?;
        Ok(domain)
    }

    /// Declare and persist a new [`InstitutionalDomain`], **gated** by the
    /// app-side [`DefaultMandateGate`] over this manager's wired
    /// [`GovernanceReceiptBackend`].
    ///
    /// Declaring a governed domain is an authority-bearing act (ADR-0083): this
    /// is the authorization-resolving wrapper a routable surface (a future
    /// declare/create HTTP route) must call instead of the bootstrap-only
    /// [`Self::declare_institutional_domain`]. It resolves whether `actor` holds
    /// an active, domain-scoped [`Execution`] grant binding the
    /// `institutional_domain:declare` act — via the **real** gate resolver, the
    /// same machinery [`Self::adopt_domain_policy`] uses — then delegates to
    /// [`Self::declare_institutional_domain`] for the persisted create. No
    /// resolver logic is duplicated and no new authority primitive is added.
    ///
    /// Fails closed: [`DeclareInstitutionalDomainError::MissingReceiptBackend`]
    /// (no backend wired), [`DeclareInstitutionalDomainError::Unauthorized`] on
    /// any gate rejection (wrong actor / domain / act / class, expired or revoked
    /// authority), and [`DeclareInstitutionalDomainError::Store`] when authority
    /// resolves but the persisted declaration fails (no domain store, already
    /// declared, or a backend write error). Authority is resolved **before** any
    /// store write, so an unauthorized caller never mutates state.
    ///
    /// [`Execution`]: icn_governance::AuthorityClass::Execution
    pub fn declare_institutional_domain_gated(
        &self,
        domain_id: GovernanceDomainId,
        owning_entity_class: BootstrapEntityType,
        charter_ref: Option<CharterId>,
        actor: &Did,
        now: Timestamp,
    ) -> Result<InstitutionalDomain, DeclareInstitutionalDomainError> {
        let backend = self
            .receipt_backend()
            .ok_or(DeclareInstitutionalDomainError::MissingReceiptBackend)?;

        // Resolve declaration authority through the real gate BEFORE any store
        // write, so an unauthorized caller never creates a domain. Mirrors the
        // adoption seam: actor → active grants → TypedScope.domain + Execution
        // class + `institutional_domain:declare` act token + mandate lifecycle.
        let gate = DefaultMandateGate::new(backend);
        let req = MandateRequest {
            actor: actor.clone(),
            domain: domain_id.clone(),
            act: MandateAct::DeclareInstitutionalDomain,
            target: MandateTarget::Domain(domain_id.clone()),
            at: now,
        };
        gate.require(&req)
            .map_err(DeclareInstitutionalDomainError::Unauthorized)?;

        self.declare_institutional_domain(domain_id, owning_entity_class, charter_ref)
            .map_err(DeclareInstitutionalDomainError::Store)
    }

    /// Adopt `policy` as the current policy of a **persisted**
    /// [`InstitutionalDomain`], loading it by `domain_id`, resolving authority
    /// through the existing gate, and saving the mutated record on success.
    ///
    /// Composes the existing pieces — it does **not** bypass the gate:
    /// `load → adopt_domain_policy (real DefaultMandateGate + pure-core commit)
    /// → save`. Fails closed with [`InstitutionalDomainStoreError::MissingDomainStore`]
    /// (no store), [`InstitutionalDomainStoreError::NotDeclared`] (domain never
    /// declared), [`InstitutionalDomainStoreError::Adopt`] (gate/structural
    /// rejection — state left unchanged, nothing saved), or
    /// [`InstitutionalDomainStoreError::Store`] (backend read/write error).
    ///
    /// # Concurrency
    ///
    /// The `load → mutate → save` sequence is serialized **per domain** by an
    /// in-process lock keyed by `GovernanceDomainId`
    /// ([`crate::manager::GovernanceManager::domain_adoption_lock`]), so two
    /// concurrent adoptions for the same domain cannot interleave their
    /// load→save and last-writer-win, dropping an intervening `current_policy`
    /// update. Adoptions for *different* domains take distinct locks and run
    /// concurrently. The guarded section is fully synchronous, so the lock is
    /// never held across an `.await`.
    ///
    /// This is the in-process (single-node) guarantee the #2142 HTTP adoption
    /// route relies on. It is **not** a cross-process / multi-writer guarantee:
    /// a transactional store primitive (atomic compare-and-swap `get`+`put`)
    /// for multi-writer deployments remains later work.
    pub fn adopt_domain_policy_persisted(
        &self,
        domain_id: &GovernanceDomainId,
        policy: &DomainPolicy,
        actor: &Did,
        now: Timestamp,
    ) -> Result<DomainPolicyRef, InstitutionalDomainStoreError> {
        let store = self
            .domain_state_store()
            .ok_or(InstitutionalDomainStoreError::MissingDomainStore)?;

        // Serialize the load→adopt→save critical section per domain so two
        // concurrent adoptions for the *same* domain cannot interleave and
        // last-writer-win, dropping an intervening `current_policy` update.
        // Different domains take distinct locks and proceed concurrently. The
        // whole section below is synchronous (no `.await`), so holding this
        // guard across it never blocks an async executor on a held lock.
        let domain_lock = self.domain_adoption_lock(domain_id);
        let _adopt_guard = domain_lock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());

        let mut domain = store
            .get_institutional_domain(domain_id)
            .map_err(|e| InstitutionalDomainStoreError::Store(e.to_string()))?
            .ok_or(InstitutionalDomainStoreError::NotDeclared)?;

        // Real gate resolution + pure-core structural commit. On rejection the
        // in-memory `domain` is left unchanged and nothing is persisted.
        let policy_ref = self
            .adopt_domain_policy(&mut domain, policy, actor, now)
            .map_err(InstitutionalDomainStoreError::Adopt)?;

        store
            .save_institutional_domain(&domain)
            .map_err(|e| InstitutionalDomainStoreError::Store(e.to_string()))?;
        Ok(policy_ref)
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

    // ----- persisted (store-backed) manager tests --------------------------

    use crate::state_store::SledGovernanceStateStore;

    /// A manager wired with both a receipt backend (for the gate) and a
    /// temporary sled-backed `GovernanceStateStore` (for InstitutionalDomain
    /// persistence — exercises the real `SledGovernanceStateStore` impl).
    fn manager_with_stores(backend: Arc<TestBackend>) -> GovernanceManager {
        let domain_store = Arc::new(SledGovernanceStateStore::new(Arc::new(
            icn_store::SledStore::temporary().expect("temp sled"),
        )));
        GovernanceManager::new()
            .with_receipt_store(backend)
            .with_domain_store(domain_store)
    }

    fn live_adopt_fixture(actor: &Did, dom: &str) -> (Arc<TestBackend>,) {
        let gid = AuthorityGrantId::new();
        let grant = adopt_grant(actor.clone(), dom, gid.clone());
        let mandate = backing_mandate(gid);
        (TestBackend::new(vec![mandate], vec![grant]),)
    }

    #[test]
    fn declare_persists_and_round_trips() {
        let mgr = manager_with_stores(TestBackend::new(vec![], vec![]));
        let declared = mgr
            .declare_institutional_domain(
                domain_id("coop-alpha"),
                BootstrapEntityType::Cooperative,
                None,
            )
            .expect("declare succeeds");
        assert_eq!(declared.domain_id, domain_id("coop-alpha"));
        assert!(declared.current_policy().is_none());

        let store = mgr.domain_state_store().unwrap();
        let loaded = store
            .get_institutional_domain(&domain_id("coop-alpha"))
            .unwrap()
            .expect("persisted");
        assert_eq!(loaded, declared);
    }

    #[test]
    fn declare_twice_fails() {
        let mgr = manager_with_stores(TestBackend::new(vec![], vec![]));
        mgr.declare_institutional_domain(
            domain_id("coop-alpha"),
            BootstrapEntityType::Cooperative,
            None,
        )
        .unwrap();
        let err = mgr
            .declare_institutional_domain(
                domain_id("coop-alpha"),
                BootstrapEntityType::Cooperative,
                None,
            )
            .expect_err("second declare must fail");
        assert!(matches!(
            err,
            InstitutionalDomainStoreError::AlreadyDeclared
        ));
    }

    #[test]
    fn adopt_persisted_loads_adopts_saves_and_survives_reload() {
        let actor = did(1);
        let (backend,) = live_adopt_fixture(&actor, "coop-alpha");
        let mgr = manager_with_stores(backend);
        mgr.declare_institutional_domain(
            domain_id("coop-alpha"),
            BootstrapEntityType::Cooperative,
            None,
        )
        .unwrap();

        let policy = DomainPolicy::new(domain_id("coop-alpha"), b"policy v1");
        let adopted = mgr
            .adopt_domain_policy_persisted(&domain_id("coop-alpha"), &policy, &actor, 100)
            .expect("persisted adoption succeeds");
        assert_eq!(adopted, policy.policy_ref());

        // current_policy survives reload from the store.
        let store = mgr.domain_state_store().unwrap();
        let reloaded = store
            .get_institutional_domain(&domain_id("coop-alpha"))
            .unwrap()
            .expect("persisted");
        assert_eq!(reloaded.current_policy(), Some(&policy.policy_ref()));
    }

    #[test]
    fn adopt_persisted_fails_when_not_declared() {
        let actor = did(1);
        let (backend,) = live_adopt_fixture(&actor, "coop-alpha");
        let mgr = manager_with_stores(backend);
        let policy = DomainPolicy::new(domain_id("coop-alpha"), b"policy v1");
        let err = mgr
            .adopt_domain_policy_persisted(&domain_id("coop-alpha"), &policy, &actor, 100)
            .expect_err("adoption on an undeclared domain must fail");
        assert!(matches!(err, InstitutionalDomainStoreError::NotDeclared));
    }

    #[test]
    fn persisted_ops_fail_closed_without_domain_store() {
        // Receipt store wired, but NO domain store.
        let mgr = GovernanceManager::new().with_receipt_store(TestBackend::new(vec![], vec![]));
        let declare_err = mgr
            .declare_institutional_domain(
                domain_id("coop-alpha"),
                BootstrapEntityType::Cooperative,
                None,
            )
            .expect_err("declare must fail closed without a store");
        assert!(matches!(
            declare_err,
            InstitutionalDomainStoreError::MissingDomainStore
        ));

        let policy = DomainPolicy::new(domain_id("coop-alpha"), b"policy v1");
        let adopt_err = mgr
            .adopt_domain_policy_persisted(&domain_id("coop-alpha"), &policy, &did(1), 100)
            .expect_err("persisted adoption must fail closed without a store");
        assert!(matches!(
            adopt_err,
            InstitutionalDomainStoreError::MissingDomainStore
        ));
    }

    #[test]
    fn adopt_persisted_rejects_wrong_actor_via_gate() {
        // Grant is for did(1); the actor attempting adoption is did(2).
        let (backend,) = live_adopt_fixture(&did(1), "coop-alpha");
        let mgr = manager_with_stores(backend);
        mgr.declare_institutional_domain(
            domain_id("coop-alpha"),
            BootstrapEntityType::Cooperative,
            None,
        )
        .unwrap();

        let policy = DomainPolicy::new(domain_id("coop-alpha"), b"policy v1");
        let err = mgr
            .adopt_domain_policy_persisted(&domain_id("coop-alpha"), &policy, &did(2), 100)
            .expect_err("wrong actor must be refused by the gate");
        assert!(matches!(
            err,
            InstitutionalDomainStoreError::Adopt(DomainPolicyAdoptionError::Gated(
                AdoptDomainPolicyError::Unauthorized(MandateGateError::Rejected(
                    MandateRejection::NoMandate
                ))
            ))
        ));

        // State unchanged on rejection: current_policy stays None after reload.
        let store = mgr.domain_state_store().unwrap();
        assert!(store
            .get_institutional_domain(&domain_id("coop-alpha"))
            .unwrap()
            .unwrap()
            .current_policy()
            .is_none());
    }

    #[test]
    fn adopt_persisted_rejects_revoked_authority_via_gate() {
        let actor = did(1);
        let gid = AuthorityGrantId::new();
        let grant = adopt_grant(actor.clone(), "coop-alpha", gid.clone());
        let mut mandate = backing_mandate(gid);
        mandate.status = MandateStatus::Revoked;
        let mgr = manager_with_stores(TestBackend::new(vec![mandate], vec![grant]));
        mgr.declare_institutional_domain(
            domain_id("coop-alpha"),
            BootstrapEntityType::Cooperative,
            None,
        )
        .unwrap();

        let policy = DomainPolicy::new(domain_id("coop-alpha"), b"policy v1");
        let err = mgr
            .adopt_domain_policy_persisted(&domain_id("coop-alpha"), &policy, &actor, 100)
            .expect_err("a revoked backing mandate must be refused by the gate");
        assert!(matches!(
            err,
            InstitutionalDomainStoreError::Adopt(DomainPolicyAdoptionError::Gated(
                AdoptDomainPolicyError::Unauthorized(MandateGateError::Rejected(
                    MandateRejection::Revoked
                ))
            ))
        ));
    }

    #[test]
    fn adopt_persisted_preserves_pure_core_defense_in_depth() {
        // Gate authorizes the actor for coop-alpha, but the policy is authored
        // for coop-beta: the pure-core structural check must still reject it.
        let actor = did(1);
        let (backend,) = live_adopt_fixture(&actor, "coop-alpha");
        let mgr = manager_with_stores(backend);
        mgr.declare_institutional_domain(
            domain_id("coop-alpha"),
            BootstrapEntityType::Cooperative,
            None,
        )
        .unwrap();

        let foreign = DomainPolicy::new(domain_id("coop-beta"), b"policy v1");
        let err = mgr
            .adopt_domain_policy_persisted(&domain_id("coop-alpha"), &foreign, &actor, 100)
            .expect_err("a policy for another domain must be refused by the core check");
        assert!(matches!(
            err,
            InstitutionalDomainStoreError::Adopt(DomainPolicyAdoptionError::Gated(
                AdoptDomainPolicyError::Core(InstitutionalDomainError::PolicyForOtherDomain)
            ))
        ));
    }

    #[test]
    fn adopt_persisted_serializes_concurrent_same_domain_adoptions() {
        // Eight threads race the same per-domain `load → adopt → save` critical
        // section. The per-domain lock must serialize them: every contending
        // adoption succeeds with the same ref, and the final persisted state is
        // exactly that policy — no panic, no torn read/modify/write, no lost
        // update. This exercises the lock path under real contention.
        use std::thread;

        let actor = did(1);
        let (backend,) = live_adopt_fixture(&actor, "coop-alpha");
        let mgr = Arc::new(manager_with_stores(backend));
        mgr.declare_institutional_domain(
            domain_id("coop-alpha"),
            BootstrapEntityType::Cooperative,
            None,
        )
        .unwrap();

        let policy = DomainPolicy::new(domain_id("coop-alpha"), b"policy v1");
        let expected = policy.policy_ref();

        let handles: Vec<_> = (0..8)
            .map(|_| {
                let mgr = Arc::clone(&mgr);
                let policy = policy.clone();
                let actor = actor.clone();
                thread::spawn(move || {
                    mgr.adopt_domain_policy_persisted(
                        &domain_id("coop-alpha"),
                        &policy,
                        &actor,
                        100,
                    )
                })
            })
            .collect();

        let results: Vec<_> = handles
            .into_iter()
            .map(|h| h.join().expect("adoption thread did not panic"))
            .collect();

        assert_eq!(results.len(), 8);
        for r in &results {
            assert_eq!(
                r.as_ref().expect("each concurrent adoption succeeds"),
                &expected
            );
        }

        // Final persisted state is exactly the adopted policy (no lost update).
        let store = mgr.domain_state_store().unwrap();
        let reloaded = store
            .get_institutional_domain(&domain_id("coop-alpha"))
            .unwrap()
            .expect("persisted");
        assert_eq!(reloaded.current_policy(), Some(&expected));
    }

    // ----- gated declaration tests (#2142 declare-gate lane) ----------------

    /// An Execution grant for `actor`, bound to `dom`, carrying the
    /// `institutional_domain:declare` act token — the declare analogue of
    /// [`adopt_grant`].
    fn declare_grant(actor: Did, dom: &str, grant_id: AuthorityGrantId) -> AuthorityGrant {
        AuthorityGrant {
            id: grant_id,
            class: AuthorityClass::Execution,
            grantor: GrantorEntityId("coop:alpha".into()),
            grantee: Grantee::Person(actor),
            scope: TypedScope {
                domain: Some(domain_id(dom)),
                action_kind: vec!["institutional_domain:declare".into()],
                ..TypedScope::default()
            },
            granted_by: Some(decision()),
            valid_from: 0,
            valid_until: Some(1000),
            revoked_at: None,
        }
    }

    fn live_declare_fixture(actor: &Did, dom: &str) -> Arc<TestBackend> {
        let gid = AuthorityGrantId::new();
        let grant = declare_grant(actor.clone(), dom, gid.clone());
        let mandate = backing_mandate(gid);
        TestBackend::new(vec![mandate], vec![grant])
    }

    #[test]
    fn gated_declare_succeeds_with_active_declare_grant() {
        let actor = did(1);
        let mgr = manager_with_stores(live_declare_fixture(&actor, "coop-alpha"));
        let declared = mgr
            .declare_institutional_domain_gated(
                domain_id("coop-alpha"),
                BootstrapEntityType::Cooperative,
                None,
                &actor,
                100,
            )
            .expect("an active domain-scoped declare grant authorizes declaration");
        assert_eq!(declared.domain_id, domain_id("coop-alpha"));
        assert!(declared.current_policy().is_none());

        // The gated declaration is persisted, identical to the ungated path.
        let store = mgr.domain_state_store().unwrap();
        let loaded = store
            .get_institutional_domain(&domain_id("coop-alpha"))
            .unwrap()
            .expect("persisted");
        assert_eq!(loaded, declared);
    }

    #[test]
    fn gated_declare_fails_closed_without_receipt_backend() {
        // No receipt backend → declaration authority cannot be resolved. The
        // backend check precedes any store access, so this fails closed even
        // with no domain store wired.
        let mgr = GovernanceManager::new();
        let err = mgr
            .declare_institutional_domain_gated(
                domain_id("coop-alpha"),
                BootstrapEntityType::Cooperative,
                None,
                &did(1),
                100,
            )
            .expect_err("a manager with no receipt backend must fail closed");
        assert!(matches!(
            err,
            DeclareInstitutionalDomainError::MissingReceiptBackend
        ));
    }

    #[test]
    fn gated_declare_rejects_wrong_actor() {
        // Grant is for did(1); did(2) attempts the declaration with no grant.
        let mgr = manager_with_stores(live_declare_fixture(&did(1), "coop-alpha"));
        let err = mgr
            .declare_institutional_domain_gated(
                domain_id("coop-alpha"),
                BootstrapEntityType::Cooperative,
                None,
                &did(2),
                100,
            )
            .expect_err("an actor with no authorizing grant must be refused");
        assert!(matches!(
            err,
            DeclareInstitutionalDomainError::Unauthorized(MandateGateError::Rejected(
                MandateRejection::NoMandate
            ))
        ));
        // Nothing persisted on a gate rejection.
        let store = mgr.domain_state_store().unwrap();
        assert!(store
            .get_institutional_domain(&domain_id("coop-alpha"))
            .unwrap()
            .is_none());
    }

    #[test]
    fn gated_declare_rejects_wrong_domain() {
        // Grant scoped to coop-beta; the declaration target is coop-alpha.
        let actor = did(1);
        let mgr = manager_with_stores(live_declare_fixture(&actor, "coop-beta"));
        let err = mgr
            .declare_institutional_domain_gated(
                domain_id("coop-alpha"),
                BootstrapEntityType::Cooperative,
                None,
                &actor,
                100,
            )
            .expect_err("a grant scoped to another domain must be refused");
        assert!(matches!(
            err,
            DeclareInstitutionalDomainError::Unauthorized(MandateGateError::Rejected(
                MandateRejection::WrongTarget
            ))
        ));
    }

    #[test]
    fn gated_declare_rejects_wrong_act_token() {
        // Right actor / domain / Execution class, but the grant authorizes
        // adoption (`domain_policy:adopt`), not declaration — the declare act
        // token / class is absent, so the gate refuses.
        let actor = did(1);
        let mgr = manager_with_stores(live_adopt_fixture(&actor, "coop-alpha").0);
        let err = mgr
            .declare_institutional_domain_gated(
                domain_id("coop-alpha"),
                BootstrapEntityType::Cooperative,
                None,
                &actor,
                100,
            )
            .expect_err("a grant that does not bind the declare act must be refused");
        assert!(matches!(
            err,
            DeclareInstitutionalDomainError::Unauthorized(MandateGateError::Rejected(
                MandateRejection::WrongTarget
            ))
        ));
    }

    #[test]
    fn gated_declare_rejects_expired_grant() {
        let actor = did(1);
        let gid = AuthorityGrantId::new();
        let mut grant = declare_grant(actor.clone(), "coop-alpha", gid.clone());
        grant.valid_until = Some(50); // expired well before `at = 100`
        let mandate = backing_mandate(gid);
        let mgr = manager_with_stores(TestBackend::new(vec![mandate], vec![grant]));
        let err = mgr
            .declare_institutional_domain_gated(
                domain_id("coop-alpha"),
                BootstrapEntityType::Cooperative,
                None,
                &actor,
                100,
            )
            .expect_err("an expired grant must be refused");
        assert!(matches!(
            err,
            DeclareInstitutionalDomainError::Unauthorized(MandateGateError::Rejected(
                MandateRejection::NoMandate
            ))
        ));
    }

    #[test]
    fn gated_declare_rejects_revoked_mandate() {
        let actor = did(1);
        let gid = AuthorityGrantId::new();
        let grant = declare_grant(actor.clone(), "coop-alpha", gid.clone());
        let mut mandate = backing_mandate(gid);
        mandate.status = MandateStatus::Revoked; // grant active, mandate dead
        let mgr = manager_with_stores(TestBackend::new(vec![mandate], vec![grant]));
        let err = mgr
            .declare_institutional_domain_gated(
                domain_id("coop-alpha"),
                BootstrapEntityType::Cooperative,
                None,
                &actor,
                100,
            )
            .expect_err("a revoked backing mandate must be refused");
        assert!(matches!(
            err,
            DeclareInstitutionalDomainError::Unauthorized(MandateGateError::Rejected(
                MandateRejection::Revoked
            ))
        ));
    }

    #[test]
    fn gated_declare_still_refuses_duplicate() {
        // Authority resolves, but a record already exists → the underlying
        // store refusal surfaces as Store(AlreadyDeclared).
        let actor = did(1);
        let mgr = manager_with_stores(live_declare_fixture(&actor, "coop-alpha"));
        mgr.declare_institutional_domain_gated(
            domain_id("coop-alpha"),
            BootstrapEntityType::Cooperative,
            None,
            &actor,
            100,
        )
        .expect("first gated declare succeeds");
        let err = mgr
            .declare_institutional_domain_gated(
                domain_id("coop-alpha"),
                BootstrapEntityType::Cooperative,
                None,
                &actor,
                100,
            )
            .expect_err("a second gated declare must fail");
        assert!(matches!(
            err,
            DeclareInstitutionalDomainError::Store(InstitutionalDomainStoreError::AlreadyDeclared)
        ));
    }

    #[test]
    fn ungated_declare_works_without_receipt_backend() {
        // The bootstrap/in-process seam needs no authority resolver: it persists
        // with only a domain store wired (no receipt backend), proving it is a
        // distinct, deliberately-ungated path from the gated seam above.
        let domain_store = Arc::new(SledGovernanceStateStore::new(Arc::new(
            icn_store::SledStore::temporary().expect("temp sled"),
        )));
        let mgr = GovernanceManager::new().with_domain_store(domain_store);
        let declared = mgr
            .declare_institutional_domain(
                domain_id("coop-alpha"),
                BootstrapEntityType::Cooperative,
                None,
            )
            .expect("ungated declare persists without any authority resolution");
        assert_eq!(declared.domain_id, domain_id("coop-alpha"));
        assert!(declared.current_policy().is_none());
    }
}
