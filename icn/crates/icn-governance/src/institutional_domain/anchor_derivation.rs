//! A trust anchor derived from a stored institutional authority binding (design spec §2.2
//! "EXTEND: anchor derivation", roadmap row P3). EXPERIMENTAL, unratified.
//!
//! # Two anchor kinds, deliberately distinct
//!
//! - [`InstitutionalTrustAnchorV0`] built by [`InstitutionalTrustAnchorV0::new`] is a
//!   **configured** anchor: the relying party asserts "domain `D` is served by context `C`" from
//!   its own configuration. Nothing checks that assertion. It is unchanged by this module, keeps
//!   its V0 meaning, and is never converted into a [`BindingDerivedAnchorV1`].
//! - [`BindingDerivedAnchorV1`] is minted **only** by [`BindingDerivedAnchorV1::from_binding`],
//!   which checks that a binding, a V1 inception and a founder-signed genesis are **mutually
//!   consistent**. Its fields are private and there is no other constructor, no `From`
//!   conversion and no `Default`, so a configured anchor cannot be presented as a derived one.
//!
//! The derived type is **binding-consistent, not provenance-verified** (see "Founder identity is
//! self-certified" below). Until the bundle verifier (P6) exists, nothing may treat a
//! `&BindingDerivedAnchorV1` as stronger institutional authority than a configured anchor. Pass
//! [`BindingDerivedAnchorV1::anchor`] to the existing admission machinery
//! ([`crate::institutional_admission`]), which is unchanged.
//!
//! # What `from_binding` checks
//!
//! All of these must hold, or nothing is returned:
//!
//! 1. the binding is internally consistent: `served_subject == served_domain_commitment(domain)`
//!    (re-checked here, not assumed);
//! 2. the inception is **V1**. A V0 inception has no served-domain commitment and cannot back a
//!    binding (design spec Part 6);
//! 3. the inception validates structurally;
//! 4. `inception.context_ref() == binding.context()`;
//! 5. `inception.served_subject == binding.served_subject()`, so the founders' context commits to
//!    exactly the binding's domain, compared as exact bytes (no Unicode normalization);
//! 6. the presented genesis replays as entry 1 of that context under the CAL's own genesis rules,
//!    including the founder quorum, and its head equals `binding.genesis_head()`;
//! 7. the relying frontier is at least position 1, and a checkpoint pinned at position 1 is the
//!    genesis head;
//! 8. everything [`InstitutionalTrustAnchorV0::new`] checks (rules, pin position, non-empty
//!    domain).
//!
//! Cheap comparisons (1–5) run before any signature is verified (6).
//!
//! # What it does NOT establish (read before citing)
//!
//! - **Founder identity is self-certified.** The founder set is whatever the inception says.
//!   Anyone can generate keys, found a context whose inception commits to
//!   `served_domain_commitment(D)`, sign its genesis, and build a matching binding (the binding
//!   constructor is public, and a raw-store holder can store one). `from_binding` **accepts** such
//!   a fully self-consistent rogue triple; this is pinned by the LIMIT test
//!   `limit_a_fully_rogue_consistent_triple_is_accepted`. What it refuses is *inconsistency*:
//!   a binding, inception and genesis that do not all name the same `D`, `C` and genesis. The
//!   trust root is therefore still whoever chooses the evidence, as for a configured anchor.
//! - **The binding's basis.** Neither a `PredecessorCutover` nor a `DeclaredGenesis` basis is
//!   verified here: the predecessor's consent, the seal and the declaration decision are the
//!   bundle verifier's job (P6). A derived anchor shows that the stored record, the inception and
//!   the founders agree that `C` serves `D`; it does **not** show that `D`'s institution chose
//!   `C`. It is not a [`crate::institutional_admission::Legitimacy`] upgrade: recognition
//!   requirements and legitimacy reporting are unchanged.
//! - **Uniqueness across nodes.** Two nodes with different bindings for `D` derive different
//!   anchors; detecting that needs evidence comparison (design spec Part 6).
//! - **Freshness.** `min_seq` and `pinned` are the relying party's, as for a configured anchor.
//!
//! Nothing here writes, persists or encodes anything: a derived anchor has no encoding and no
//! hash.

use icn_authz::effect_v0::{
    replay_context_log, AdoptedRule, AuthzCodecError, ContextRef, LogRefusal, RuleResolver,
};

/// The founding-evidence types [`BindingDerivedAnchorV1::from_binding`] takes (and those needed to
/// build them), re-exported so an app-layer caller needs no direct `icn-authz` dependency.
pub use icn_authz::effect_v0::{
    ContextInception, ContextInceptionV1, LogCheckpointV0, LogEntryV0, SignedLogEntryV0,
    TransitionV0,
};
use thiserror::Error;

use super::authority_binding::InstitutionalAuthorityBindingV1;
use crate::authz_threshold::ThresholdRuleV0;
use crate::domain::{served_domain_commitment, GovernanceDomainId, ServedDomainError};
use crate::institutional_admission::{
    InstitutionalTrustAnchorV0, RecognitionRequirement, TrustAnchorError,
};

/// Why an anchor could not be derived from a binding. Every variant is a refusal.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AnchorDerivationError {
    /// The binding's own `served_subject` is not the commitment of its domain.
    #[error("binding is internally inconsistent: served subject is not its domain's commitment")]
    InconsistentBinding,
    /// The binding's domain has no served-domain commitment.
    #[error("binding domain has no served-domain commitment: {0}")]
    UncommittableDomain(ServedDomainError),
    /// The inception is V0, which carries no served-domain commitment.
    #[error("a V0 inception cannot back an institutional authority binding")]
    V0Inception,
    /// The inception does not validate.
    #[error("invalid inception: {0}")]
    InvalidInception(AuthzCodecError),
    /// The inception is not the binding's context.
    #[error("inception is not the bound authority context")]
    ContextMismatch,
    /// The inception commits to a different served subject than the binding.
    #[error("inception's served subject is not the bound domain's commitment")]
    ServedSubjectMismatch,
    /// The presented genesis does not replay as entry 1 of the bound context.
    #[error("genesis does not verify for the bound context: {0}")]
    InvalidGenesis(LogRefusal),
    /// The genesis verifies but is not the one the binding names.
    #[error("genesis head is not the bound genesis head")]
    GenesisHeadMismatch,
    /// The relying frontier is below position 1.
    #[error("relying frontier must be at least position 1")]
    ZeroFrontier,
    /// A checkpoint pinned at position 1 is not the bound genesis head.
    #[error("pinned checkpoint at position 1 is not the bound genesis head")]
    PinnedGenesisMismatch,
    /// The anchor itself is refused (rules, pin, domain).
    #[error("trust anchor refused: {0}")]
    Anchor(TrustAnchorError),
}

/// An institutional trust anchor whose domain→context binding is consistent with the context's
/// V1 inception and founder-signed genesis. **Not provenance-verified**: founder identity is
/// self-certified (see the module docs), so this is no stronger than a configured anchor until
/// P6 verifies the binding's basis.
///
/// The only constructor is [`Self::from_binding`]. A dependent crate can neither build one by
/// struct literal nor convert a configured anchor into one:
///
/// ```compile_fail,E0451
/// use icn_governance::institutional_admission::InstitutionalTrustAnchorV0;
/// use icn_governance::institutional_domain::anchor_derivation::BindingDerivedAnchorV1;
/// fn forge(anchor: InstitutionalTrustAnchorV0) -> BindingDerivedAnchorV1 {
///     BindingDerivedAnchorV1 { anchor, genesis_head: [0; 32] }
/// }
/// ```
///
/// ```compile_fail,E0277
/// use icn_governance::institutional_admission::InstitutionalTrustAnchorV0;
/// use icn_governance::institutional_domain::anchor_derivation::BindingDerivedAnchorV1;
/// fn forge(anchor: InstitutionalTrustAnchorV0) -> BindingDerivedAnchorV1 {
///     anchor.into()
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindingDerivedAnchorV1 {
    anchor: InstitutionalTrustAnchorV0,
    genesis_head: [u8; 32],
}

/// Resolves nothing. A genesis-only replay never consults a rule, so any lookup is refused.
struct NoRules;

impl RuleResolver for NoRules {
    fn resolve(&self, _rule_hash: &[u8; 32]) -> Option<&dyn AdoptedRule> {
        None
    }
}

impl BindingDerivedAnchorV1 {
    /// Derive an anchor from `binding`, the bound context's `inception` and its founder-signed
    /// `genesis`, with the relying party's own frontier, rules and recognition requirement.
    ///
    /// `binding` normally comes from the app layer's typed store read; it is treated as an
    /// untrusted record and checked for consistency. `inception` and `genesis` are checked against
    /// it and against each other; the founders themselves are not authenticated (module docs).
    pub fn from_binding(
        binding: &InstitutionalAuthorityBindingV1,
        inception: &ContextInception,
        genesis: &SignedLogEntryV0,
        min_seq: u64,
        pinned: Option<LogCheckpointV0>,
        rules: Vec<ThresholdRuleV0>,
        recognition: RecognitionRequirement,
    ) -> Result<Self, AnchorDerivationError> {
        let domain: &GovernanceDomainId = binding.domain();
        let commitment =
            served_domain_commitment(domain).map_err(AnchorDerivationError::UncommittableDomain)?;
        if binding.served_subject() != commitment {
            return Err(AnchorDerivationError::InconsistentBinding);
        }

        let ContextInception::V1(v1) = inception else {
            return Err(AnchorDerivationError::V0Inception);
        };
        v1.validate()
            .map_err(AnchorDerivationError::InvalidInception)?;
        let context: ContextRef = v1.context_ref();
        if context != binding.context() {
            return Err(AnchorDerivationError::ContextMismatch);
        }
        if v1.served_subject != binding.served_subject() {
            return Err(AnchorDerivationError::ServedSubjectMismatch);
        }
        if min_seq < 1 {
            return Err(AnchorDerivationError::ZeroFrontier);
        }
        if pinned.is_some_and(|p| p.seq == 1 && p.head != binding.genesis_head()) {
            return Err(AnchorDerivationError::PinnedGenesisMismatch);
        }

        // Founder quorum, context, position and chain: the CAL's own genesis verification.
        let replayed = replay_context_log(v1, std::slice::from_ref(genesis), &NoRules)
            .map_err(AnchorDerivationError::InvalidGenesis)?;
        if replayed.head_at(1) != Some(binding.genesis_head()) {
            return Err(AnchorDerivationError::GenesisHeadMismatch);
        }

        let anchor = InstitutionalTrustAnchorV0::new(
            domain.clone(),
            context,
            min_seq,
            pinned,
            rules,
            recognition,
        )
        .map_err(AnchorDerivationError::Anchor)?;
        Ok(Self {
            anchor,
            genesis_head: binding.genesis_head(),
        })
    }

    /// The derived anchor, for the existing admission machinery
    /// ([`crate::institutional_admission::admit_institutional`],
    /// [`crate::institutional_admission::InstitutionalAdmissionVerifierV0`]).
    pub fn anchor(&self) -> &InstitutionalTrustAnchorV0 {
        &self.anchor
    }

    /// The governance domain.
    pub fn domain(&self) -> &GovernanceDomainId {
        self.anchor.domain()
    }

    /// The bound authority context.
    pub fn context(&self) -> ContextRef {
        self.anchor.context()
    }

    /// The verified genesis head of the bound context.
    pub fn genesis_head(&self) -> [u8; 32] {
        self.genesis_head
    }
}
