//! Institutional domain and domain policy — the minimal runtime authority
//! root (ADR-0083, #2142).
//!
//! The operating model names **Domain** as the governed jurisdiction that
//! *holds authority*, and **Policy** as adopted rule state that is *inert
//! until a domain adopts it*. This module is the smallest runtime slice that
//! closes the gap between the [`crate::domain::GovernanceDomain`] decision
//! space and a standing object that *holds* a domain's authority and *points
//! at its currently-adopted policy*.
//!
//! # What lives here
//!
//! - [`InstitutionalDomain`] — a thin standing authority object for a
//!   governed jurisdiction. Keyed by the existing
//!   [`crate::domain::GovernanceDomainId`] (no rename, no fork of
//!   `GovernanceDomain`), it carries the owning entity class
//!   ([`crate::bootstrap::BootstrapEntityType`]), an optional adopted charter
//!   reference ([`crate::charter::CharterId`]), and a single
//!   `current_policy: Option<DomainPolicyRef>` — the adopted-policy pointer.
//!   It embeds no charter text, no policy text, no membership rolls — only
//!   references and the adoption record.
//! - [`DomainPolicy`] — the persistent shape of a policy a domain *may*
//!   adopt. In this MVP it carries only a content-addressed identifier and
//!   the domain it was authored for; it does **not** store or interpret CCL
//!   text (that lands with the CCL policy registry, #1817).
//! - [`DomainPolicyRef`] / [`DomainPolicyId`] — the content-addressed
//!   reference embedded in `current_policy`, and its identifier.
//!
//! # Inertness is structural
//!
//! A [`DomainPolicyRef`] confers authority/constraint **only** when it is the
//! [`InstitutionalDomain::current_policy`]. Any other policy ref — never
//! adopted, or superseded — is inert: it yields no authority, no constraint,
//! no effect. There is at most one current policy per domain; prior versions
//! are history. Adoption state is therefore *not* a flag on the policy — it is
//! computed structurally via [`InstitutionalDomain::has_adopted`].
//!
//! # Adoption is fail-closed
//!
//! Setting or changing `current_policy` is a governance act that **requires an
//! authority basis** expressed through the existing ADR-0014 objects
//! ([`crate::mandate::Mandate`] composing [`crate::authority::AuthorityGrant`]s).
//! This module does **not** invent a new authority primitive and does **not**
//! re-implement the apps-layer `MandateGate` resolver (which needs a receipt
//! backend, out of scope for this slice). Instead it reuses the *same*
//! fail-closed primitives the gate uses — [`Mandate::has_no_grants`],
//! [`crate::mandate::MandateStatus`] liveness, and [`Mandate::is_past_deadline`]
//! — and refuses any transition presented with a **missing**, **ambiguous**,
//! **unbound**, or **inactive** authority basis, leaving policy state
//! unchanged. Resolving a grant's `TypedScope.domain` against the target
//! domain (which needs the grant store) is deferred to the gate-wired
//! follow-up.
//!
//! # Layer placement
//!
//! Like the rest of the ADR-0014 / ADR-0083 types, these objects live at the
//! governance app layer and carry only generic ICN vocabulary. They are never
//! imported by kernel crates, and no package-local (NYCN/Summit) noun appears
//! here. The Meaning Firewall remains intact.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::bootstrap::BootstrapEntityType;
use crate::charter::CharterId;
use crate::domain::GovernanceDomainId;
use crate::mandate::{Mandate, MandateStatus};
use crate::Timestamp;

/// Content-addressed identifier for a [`DomainPolicy`] version.
///
/// The blake3 hash of the policy's canonical content. Distinct, searchable
/// newtype in the repo idiom of [`CharterId`] / [`crate::mandate::MandateId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DomainPolicyId(pub [u8; 32]);

impl DomainPolicyId {
    /// Derive an id from the policy's canonical content bytes.
    pub fn from_content(content: &[u8]) -> Self {
        Self(*blake3::hash(content).as_bytes())
    }

    /// Wrap raw identifier bytes (e.g. when rehydrating from storage).
    pub fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Borrow the underlying bytes.
    pub fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// Hex-encode the identifier.
    pub fn to_hex(&self) -> String {
        hex::encode(self.0)
    }
}

impl std::fmt::Display for DomainPolicyId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

/// A minimal, content-addressed reference to a policy version, plus the
/// governance domain it was authored for.
///
/// This is the lightweight projection embedded in
/// [`InstitutionalDomain::current_policy`]. It is **inert by construction**: a
/// ref confers authority only when it is a domain's `current_policy` (see the
/// module docs). Holding a `DomainPolicyRef` in isolation grants nothing.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct DomainPolicyRef {
    /// Content-addressed identifier of the referenced policy version.
    pub id: DomainPolicyId,
    /// The governance domain the policy was authored for.
    pub domain_id: GovernanceDomainId,
}

/// The persistent shape of a policy a domain *may* adopt.
///
/// In this MVP a `DomainPolicy` carries only its content-addressed identifier
/// and the domain it targets — it does **not** store or interpret CCL text
/// (that lands with the CCL policy registry, #1817). A `DomainPolicy` is inert
/// until an [`InstitutionalDomain`] adopts its [`DomainPolicyRef`]; adoption
/// state is structural, never a flag carried here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DomainPolicy {
    /// Content-addressed identifier of this policy version.
    pub id: DomainPolicyId,
    /// The governance domain this policy was authored for.
    pub domain_id: GovernanceDomainId,
}

impl DomainPolicy {
    /// Construct a policy for `domain_id`, content-addressing `content`.
    pub fn new(domain_id: GovernanceDomainId, content: &[u8]) -> Self {
        Self {
            id: DomainPolicyId::from_content(content),
            domain_id,
        }
    }

    /// The embeddable [`DomainPolicyRef`] projection of this policy.
    pub fn policy_ref(&self) -> DomainPolicyRef {
        DomainPolicyRef {
            id: self.id,
            domain_id: self.domain_id.clone(),
        }
    }
}

/// Errors returned when a governed-domain transition is refused.
///
/// Every variant leaves the domain's policy state **unchanged** — adoption is
/// fail-closed (ADR-0083 / ADR-0014).
#[derive(Debug, Error, PartialEq, Eq)]
pub enum InstitutionalDomainError {
    /// The policy was authored for a different governance domain.
    #[error("policy belongs to a different governance domain")]
    PolicyForOtherDomain,
    /// No authority basis was presented for the transition.
    #[error("no authority basis presented for the transition")]
    MissingAuthority,
    /// More than one candidate mandate was presented with no unique
    /// selection — the authority basis does not resolve.
    #[error("authority basis is ambiguous: more than one candidate mandate")]
    AmbiguousAuthority,
    /// The sole candidate mandate carries no bounded authority grants
    /// (a "pending-grants" institutional-memory record).
    #[error("authority mandate carries no bounded grants")]
    UnboundAuthority,
    /// The sole candidate mandate is not live (expired by status or deadline,
    /// revoked, or already discharged).
    #[error("authority mandate is not live (expired, revoked, or discharged)")]
    InactiveAuthority,
}

/// A thin standing authority object for a governed jurisdiction.
///
/// Keyed by the existing [`GovernanceDomainId`] (it references, and never
/// renames or forks, [`crate::domain::GovernanceDomain`]). It carries only the
/// owning entity class, an optional adopted charter reference, and a single
/// adopted-policy pointer. A domain with `current_policy == None` is a valid,
/// declared-but-unbound domain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstitutionalDomain {
    /// The governance domain (decision space) this authority object wraps.
    pub domain_id: GovernanceDomainId,
    /// The governed entity taxonomy: `Federation` / `Cooperative` /
    /// `Community` / `Individual`. Reuses the existing governance-layer enum
    /// rather than introducing a new one (ADR-0083).
    pub owning_entity_class: BootstrapEntityType,
    /// Optional reference to the domain's adopted founding charter.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub charter_ref: Option<CharterId>,
    /// The single adopted-policy pointer. `None` for a declared-but-unbound
    /// domain. There is at most one current policy per domain; prior versions
    /// are history.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_policy: Option<DomainPolicyRef>,
}

impl InstitutionalDomain {
    /// Declare a governed domain over an existing [`GovernanceDomainId`],
    /// owned by `owning_entity_class`. The domain is declared-but-unbound: it
    /// holds no charter and no policy.
    pub fn declare(
        domain_id: GovernanceDomainId,
        owning_entity_class: BootstrapEntityType,
    ) -> Self {
        Self {
            domain_id,
            owning_entity_class,
            charter_ref: None,
            current_policy: None,
        }
    }

    /// Builder: attach an adopted charter reference.
    pub fn with_charter(mut self, charter_ref: CharterId) -> Self {
        self.charter_ref = Some(charter_ref);
        self
    }

    /// The currently-adopted policy reference, if any.
    pub fn current_policy(&self) -> Option<&DomainPolicyRef> {
        self.current_policy.as_ref()
    }

    /// Whether `policy_ref` is this domain's currently-adopted policy.
    ///
    /// This is the **structural** inertness check: a ref confers authority
    /// only when it equals `current_policy`. Any other ref returns `false`.
    pub fn has_adopted(&self, policy_ref: &DomainPolicyRef) -> bool {
        self.current_policy.as_ref() == Some(policy_ref)
    }

    /// Adopt `policy` as this domain's current policy, gated fail-closed by the
    /// presented authority basis.
    ///
    /// `authority` is the candidate-mandate set a resolver would present at act
    /// time. The transition succeeds only when the basis resolves to a single,
    /// live, grant-bearing [`Mandate`]; otherwise it is refused and the
    /// domain's policy state is left unchanged. On success `current_policy` is
    /// set to `policy.policy_ref()` (replacing any prior policy) and that ref
    /// is returned.
    ///
    /// Fail-closed rules, in order:
    /// - the policy must target this domain ([`InstitutionalDomainError::PolicyForOtherDomain`]);
    /// - an empty basis is missing authority ([`InstitutionalDomainError::MissingAuthority`]);
    /// - more than one candidate is ambiguous ([`InstitutionalDomainError::AmbiguousAuthority`]);
    /// - a sole mandate with no grants is unbound ([`InstitutionalDomainError::UnboundAuthority`]);
    /// - a sole mandate that is not live is inactive ([`InstitutionalDomainError::InactiveAuthority`]).
    ///
    /// The liveness checks reuse the same primitives the apps-layer
    /// `MandateGate` uses ([`Mandate::has_no_grants`], [`MandateStatus`]
    /// liveness, [`Mandate::is_past_deadline`]). Resolving a grant's
    /// `TypedScope.domain` against this domain (which needs the grant store) is
    /// deferred to the gate-wired follow-up.
    pub fn adopt_policy(
        &mut self,
        policy: &DomainPolicy,
        authority: &[Mandate],
        now: Timestamp,
    ) -> Result<DomainPolicyRef, InstitutionalDomainError> {
        if policy.domain_id != self.domain_id {
            return Err(InstitutionalDomainError::PolicyForOtherDomain);
        }

        let mandate = match authority {
            [] => return Err(InstitutionalDomainError::MissingAuthority),
            [single] => single,
            _ => return Err(InstitutionalDomainError::AmbiguousAuthority),
        };

        if mandate.has_no_grants() {
            return Err(InstitutionalDomainError::UnboundAuthority);
        }
        if !matches!(
            mandate.status,
            MandateStatus::Pending | MandateStatus::InProgress
        ) {
            return Err(InstitutionalDomainError::InactiveAuthority);
        }
        if mandate.is_past_deadline(now) {
            return Err(InstitutionalDomainError::InactiveAuthority);
        }

        let policy_ref = policy.policy_ref();
        self.current_policy = Some(policy_ref.clone());
        Ok(policy_ref)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::authority::{AuthorityGrantId, DecisionProvenance};
    use crate::bootstrap::BootstrapEntityType;
    use crate::charter::CharterId;
    use crate::domain::GovernanceDomainId;
    use crate::mandate::{Mandate, MandateStatus};

    fn domain_id(s: &str) -> GovernanceDomainId {
        GovernanceDomainId::new(s)
    }

    fn decision() -> DecisionProvenance {
        DecisionProvenance {
            proposal_id: "prop-1".into(),
            decision_hash: [7u8; 32],
        }
    }

    /// A single, live, grant-bearing mandate — the only shape that authorizes.
    fn live_mandate() -> Mandate {
        Mandate::new(
            decision(),
            [9u8; 32],
            vec![AuthorityGrantId::new()],
            None,
            None,
            100,
        )
        .expect("grant-bearing mandate is valid")
    }

    #[test]
    fn declare_creates_unbound_domain() {
        let d =
            InstitutionalDomain::declare(domain_id("coop-alpha"), BootstrapEntityType::Cooperative);
        assert_eq!(d.domain_id, domain_id("coop-alpha"));
        assert_eq!(d.owning_entity_class, BootstrapEntityType::Cooperative);
        assert!(d.charter_ref.is_none());
        assert!(
            d.current_policy().is_none(),
            "a freshly declared domain holds no policy"
        );
    }

    #[test]
    fn with_charter_sets_charter_ref() {
        let charter = CharterId::from_content(b"founding charter");
        let d =
            InstitutionalDomain::declare(domain_id("coop-alpha"), BootstrapEntityType::Cooperative)
                .with_charter(charter.clone());
        assert_eq!(d.charter_ref, Some(charter));
    }

    #[test]
    fn adopt_valid_policy_sets_current_policy() {
        let mut d =
            InstitutionalDomain::declare(domain_id("coop-alpha"), BootstrapEntityType::Cooperative);
        let policy = DomainPolicy::new(domain_id("coop-alpha"), b"policy v1");

        let adopted = d
            .adopt_policy(&policy, &[live_mandate()], 200)
            .expect("a single live grant-bearing mandate authorizes adoption");

        assert_eq!(adopted, policy.policy_ref());
        assert_eq!(d.current_policy(), Some(&policy.policy_ref()));
        assert!(d.has_adopted(&policy.policy_ref()));
    }

    #[test]
    fn unadopted_policy_is_inert() {
        let mut d =
            InstitutionalDomain::declare(domain_id("coop-alpha"), BootstrapEntityType::Cooperative);
        let adopted = DomainPolicy::new(domain_id("coop-alpha"), b"policy v1");
        let other = DomainPolicy::new(domain_id("coop-alpha"), b"policy v2");

        d.adopt_policy(&adopted, &[live_mandate()], 200).unwrap();

        // A ref that is not `current_policy` yields no authority: it is not
        // adopted, and there is no other surface through which it could act.
        assert!(!d.has_adopted(&other.policy_ref()));
        assert_ne!(d.current_policy(), Some(&other.policy_ref()));
    }

    #[test]
    fn adopting_replaces_prior_current_policy() {
        let mut d =
            InstitutionalDomain::declare(domain_id("coop-alpha"), BootstrapEntityType::Cooperative);
        let v1 = DomainPolicy::new(domain_id("coop-alpha"), b"policy v1");
        let v2 = DomainPolicy::new(domain_id("coop-alpha"), b"policy v2");

        d.adopt_policy(&v1, &[live_mandate()], 200).unwrap();
        d.adopt_policy(&v2, &[live_mandate()], 300).unwrap();

        // At most one current policy per domain; the prior version is history.
        assert_eq!(d.current_policy(), Some(&v2.policy_ref()));
        assert!(!d.has_adopted(&v1.policy_ref()));
    }

    #[test]
    fn policy_for_another_domain_cannot_be_adopted() {
        let mut d =
            InstitutionalDomain::declare(domain_id("coop-alpha"), BootstrapEntityType::Cooperative);
        let foreign = DomainPolicy::new(domain_id("coop-beta"), b"policy v1");

        let err = d
            .adopt_policy(&foreign, &[live_mandate()], 200)
            .expect_err("a policy authored for another domain must be refused");
        assert_eq!(err, InstitutionalDomainError::PolicyForOtherDomain);
        assert!(
            d.current_policy().is_none(),
            "state is unchanged on rejection"
        );
    }

    #[test]
    fn missing_authority_fails_closed() {
        let mut d =
            InstitutionalDomain::declare(domain_id("coop-alpha"), BootstrapEntityType::Cooperative);
        let policy = DomainPolicy::new(domain_id("coop-alpha"), b"policy v1");

        let err = d
            .adopt_policy(&policy, &[], 200)
            .expect_err("no authority basis must fail closed");
        assert_eq!(err, InstitutionalDomainError::MissingAuthority);
        assert!(
            d.current_policy().is_none(),
            "state is unchanged on rejection"
        );
    }

    #[test]
    fn ambiguous_authority_fails_closed() {
        let mut d =
            InstitutionalDomain::declare(domain_id("coop-alpha"), BootstrapEntityType::Cooperative);
        let policy = DomainPolicy::new(domain_id("coop-alpha"), b"policy v1");

        let err = d
            .adopt_policy(&policy, &[live_mandate(), live_mandate()], 200)
            .expect_err("more than one candidate mandate is ambiguous and must fail closed");
        assert_eq!(err, InstitutionalDomainError::AmbiguousAuthority);
        assert!(
            d.current_policy().is_none(),
            "state is unchanged on rejection"
        );
    }

    #[test]
    fn unbound_authority_fails_closed() {
        let mut d =
            InstitutionalDomain::declare(domain_id("coop-alpha"), BootstrapEntityType::Cooperative);
        let policy = DomainPolicy::new(domain_id("coop-alpha"), b"policy v1");

        // A "pending-grants" mandate attests a decision occurred but carries no
        // bounded authority — the same record `MandateGate` rejects fail-closed.
        let unbound = Mandate::new_pending_grants(decision(), [9u8; 32], None, None, 100);
        assert!(unbound.has_no_grants());

        let err = d
            .adopt_policy(&policy, &[unbound], 200)
            .expect_err("an empty-grant mandate carries no authority and must fail closed");
        assert_eq!(err, InstitutionalDomainError::UnboundAuthority);
        assert!(
            d.current_policy().is_none(),
            "state is unchanged on rejection"
        );
    }

    #[test]
    fn revoked_authority_fails_closed() {
        let mut d =
            InstitutionalDomain::declare(domain_id("coop-alpha"), BootstrapEntityType::Cooperative);
        let policy = DomainPolicy::new(domain_id("coop-alpha"), b"policy v1");

        let mut revoked = live_mandate();
        revoked.status = MandateStatus::Revoked;

        let err = d
            .adopt_policy(&policy, &[revoked], 200)
            .expect_err("a non-live mandate must fail closed");
        assert_eq!(err, InstitutionalDomainError::InactiveAuthority);
        assert!(
            d.current_policy().is_none(),
            "state is unchanged on rejection"
        );
    }

    #[test]
    fn past_deadline_authority_fails_closed() {
        let mut d =
            InstitutionalDomain::declare(domain_id("coop-alpha"), BootstrapEntityType::Cooperative);
        let policy = DomainPolicy::new(domain_id("coop-alpha"), b"policy v1");

        let mut expiring = live_mandate();
        expiring.deadline = Some(150);

        // The mandate-level deadline is authoritative for expiry even when the
        // status has not yet transitioned — mirrors `MandateGate`.
        let err = d
            .adopt_policy(&policy, &[expiring], 200)
            .expect_err("a past-deadline mandate must fail closed");
        assert_eq!(err, InstitutionalDomainError::InactiveAuthority);
        assert!(
            d.current_policy().is_none(),
            "state is unchanged on rejection"
        );
    }

    #[test]
    fn policy_id_is_content_addressed() {
        let a1 = DomainPolicy::new(domain_id("coop-alpha"), b"policy v1");
        let a2 = DomainPolicy::new(domain_id("coop-alpha"), b"policy v1");
        let b = DomainPolicy::new(domain_id("coop-alpha"), b"policy v2");

        assert_eq!(a1.id, a2.id, "identical content yields the same id");
        assert_ne!(a1.id, b.id, "different content yields a different id");
    }

    #[test]
    fn institutional_domain_roundtrips_serde() {
        let mut d =
            InstitutionalDomain::declare(domain_id("coop-alpha"), BootstrapEntityType::Cooperative)
                .with_charter(CharterId::from_content(b"charter"));
        let policy = DomainPolicy::new(domain_id("coop-alpha"), b"policy v1");
        d.adopt_policy(&policy, &[live_mandate()], 200).unwrap();

        let json = serde_json::to_string(&d).unwrap();
        let back: InstitutionalDomain = serde_json::from_str(&json).unwrap();
        assert_eq!(d, back);
    }
}
