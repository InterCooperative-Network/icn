//! Institutional admission for the EXPERIMENTAL authority-to-effect spine v0 (`icn_authz::effect_v0`).
//!
//! The Context Authority Log (CAL) re-derives *who held standing* at a checkpoint and *what the
//! rule in force decided*. It does not know which governance domain a `SubjectId` belongs to, or
//! whether that domain recognised it: a decided `GrantStanding` for a Subject incepted for a
//! foreign domain is accepted by the log (pinned by
//! `the_log_alone_accepts_standing_for_a_foreign_or_unrecognized_subject` in
//! `tests/institutional_authority_proof.rs`). That is correct for a domain-blind substrate and
//! wrong for an institution. This module adds the institutional check on the governance side,
//! keeping the authz layer domain-blind (Meaning Firewall).
//!
//! # Responsibilities kept separate
//!
//! | Fact | Owner | Consumed here as |
//! |---|---|---|
//! | Which Subject and device | GEN-A / N1 / N4 (`icn-identity`) | re-verified genesis, N4 acts |
//! | Recognition by a domain | GEN-B ([`crate::subject_recognition`]) | re-derived from each genesis |
//! | Standing and transitions | CAL (`icn_authz::effect_v0::log`) | replayed, never asserted |
//! | Which standing decides what | the adopted rule ([`ThresholdRuleV0`]) | trusted by hash |
//! | Which context serves which domain, and how fresh | **the relying party** | [`InstitutionalTrustAnchorV0`] |
//!
//! Recognition is not membership; it is a precondition the institution's electorate must meet.
//! Standing still comes only from decided log transitions. Holding a key confers nothing.
//!
//! # The trust anchor is the relying party's, never the requester's
//!
//! [`InstitutionalTrustAnchorV0`] binds a [`GovernanceDomainId`] to one `ContextRef` (which is
//! the hash of the context's inception, so the inception is bound too), a frontier, the governing
//! rules, and whether rehearsal recognition is acceptable. It is constructed from the relying
//! party's own configuration; no field of it is read from presented evidence. It has **no
//! encoding and no hash**: it is not a protocol format and not a persisted record. The inception's
//! `label_hash` is never interpreted (the harness's `label_hash` convention is not reused).
//!
//! # How the gate reaches the effect target without changing it
//!
//! The target's admission path (`preflight_checkpointed`, used by
//! `icn_authz_store::LocalResourceReservations::reserve_checkpointed`) evaluates every decision
//! through the relying party's own `RuleResolver`. [`admit_institutional`] hands the target a
//! **recognition-gated** resolver: it resolves exactly the anchor's rules (same rule hashes) and
//! refuses any decision whose electorate contains a Subject without recognition by the anchor's
//! domain. The target therefore re-runs the institutional gate inside its own pre-consumption
//! checks; a refusal there happens before the operation id is consumed or capacity changes.
//!
//! # What this does NOT establish (read before citing)
//!
//! - **Legitimacy.** The only recognition basis is GEN-B `BootstrapRehearsalV1`, which is
//!   non-production. With [`RecognitionRequirement::ProductionGrade`] every v1 recognition is
//!   refused. A [`VerifiedInstitutionalAuthorization`] under
//!   [`RecognitionRequirement::RehearsalPermitted`] reports [`Legitimacy::RehearsalOnly`].
//! - **Mandatory enforcement via [`admit_institutional`].** The closure path hands the target a
//!   gated resolver, but the target cannot tell it from an ungated one. Mandatory enforcement is
//!   [`InstitutionalAdmissionVerifierV0`]: the custodian installs it in the store and declares
//!   its [`InstitutionalAdmissionVerifierV0::policy_ref`] on an (experimental) V1 pool, and the
//!   store then refuses every caller-resolved reservation against that pool, including one
//!   arriving through [`admit_institutional`].
//! - **Anchor correctness.** Mandatory enforcement makes the *installed* anchor binding
//!   unavoidable; it does not make that binding correct. A custodian that installs a verifier
//!   anchored on the wrong context gets exactly that context's authority.
//! - **Freshness from the network.** The frontier is whatever the relying party configured. Where
//!   it comes from after a restore is an open seam (ADR-0092 review, B10).
//! - **Institutional continuity.** `InstitutionalDomain` is keyed by `GovernanceDomainId` and has
//!   no institution identity or predecessor link, so a legacy → successor binding cannot be read
//!   from an authoritative owner yet. The anchor is an explicitly trusted binding.

use std::collections::{BTreeMap, BTreeSet};

use std::any::Any;

use icn_authz::effect_v0::{
    preflight_checkpointed, verify_checkpointed_authorization, AdoptedRule, CheckpointedEvidenceV0,
    ContextRef, LogCheckpointV0, LogRefusal, RelyingFrontierV0, RuleBinding, RuleError,
    RuleResolver, Verdict, VerifiedCheckpointedAuthorization,
};
use icn_authz_store::{
    AdmissionGradeV0, AdmissionPolicyRefV0, AdmissionRefusalV0, CustodianAdmissionVerifierV0,
    VerifiedAdmissionV0,
};
use icn_identity::authority_log::SubjectId;
use icn_identity::device_authority::VerifiedDeviceAct;
use icn_identity::subject_context::SubjectContextGenesisV1;
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::authz_threshold::ThresholdRuleV0;
use crate::domain::GovernanceDomainId;
use crate::subject_recognition::{RecognitionError, RecognitionHash, SubjectRecognitionV1};

/// Upper bound on recognition evidence items in one presentation. Each item costs a GEN-A
/// verification (two signature checks), so the bound caps work an untrusted presenter can cause.
pub const MAX_RECOGNITIONS: usize = 4096;

/// Upper bound on rules one anchor trusts.
pub const MAX_ANCHOR_RULES: usize = 16;

/// Which recognition bases the relying party accepts as supporting an electorate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecognitionRequirement {
    /// Accept non-production rehearsal recognition. Verdicts are [`Legitimacy::RehearsalOnly`].
    RehearsalPermitted,
    /// Accept only bases whose `is_production_grade()` is true. No v1 basis is, so this refuses
    /// every presentation today. That is the intended fail-closed behaviour.
    ProductionGrade,
}

/// How strong the recognition behind an admitted electorate is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Legitimacy {
    /// At least one counted recognition is non-production. Not evidence of an institution's
    /// legitimate decision.
    RehearsalOnly,
    /// Every recognition rests on a production-grade basis. Unreachable with GEN-B v1.
    ProductionGrade,
}

/// Why a trust anchor could not be constructed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum TrustAnchorError {
    /// The governance domain id is empty.
    #[error("governance domain id must not be empty")]
    EmptyDomain,
    /// No governing rule was supplied.
    #[error("an anchor must trust at least one governing rule")]
    NoRules,
    /// More rules than [`MAX_ANCHOR_RULES`].
    #[error("too many governing rules")]
    TooManyRules,
    /// Two supplied rules have the same rule hash.
    #[error("duplicate governing rule")]
    DuplicateRule,
    /// A supplied rule is malformed (it would refuse every evaluation).
    #[error("malformed governing rule: {0}")]
    MalformedRule(RuleError),
    /// A pinned checkpoint at position 0 (positions start at 1).
    #[error("pinned checkpoint position must be at least 1")]
    PinnedAtZero,
}

/// The relying party's out-of-band trust for one institutional authority context.
///
/// Fields are private and set only by [`InstitutionalTrustAnchorV0::new`] from the relying
/// party's own configuration. Every check in this module takes its trust inputs from here, never
/// from the presented evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstitutionalTrustAnchorV0 {
    domain: GovernanceDomainId,
    context: ContextRef,
    min_seq: u64,
    pinned: Option<LogCheckpointV0>,
    rules: Vec<ThresholdRuleV0>,
    recognition: RecognitionRequirement,
}

impl InstitutionalTrustAnchorV0 {
    /// Bind `domain` to the authority context `context`.
    ///
    /// - `context` must come from the relying party's own retained knowledge (e.g. the
    ///   institution's decision receipt that founded the context), not from a presenter.
    /// - `min_seq` / `pinned` are the relying party's frontier: no authorization relying on an
    ///   earlier position is accepted, and a presented log must contain `pinned` verbatim.
    /// - `rules` are the governing rules the relying party will evaluate. A log whose transition
    ///   or effect rule is not among them fails closed (`RuleUnavailable`).
    pub fn new(
        domain: GovernanceDomainId,
        context: ContextRef,
        min_seq: u64,
        pinned: Option<LogCheckpointV0>,
        rules: Vec<ThresholdRuleV0>,
        recognition: RecognitionRequirement,
    ) -> Result<Self, TrustAnchorError> {
        if domain.0.is_empty() {
            return Err(TrustAnchorError::EmptyDomain);
        }
        if rules.is_empty() {
            return Err(TrustAnchorError::NoRules);
        }
        if rules.len() > MAX_ANCHOR_RULES {
            return Err(TrustAnchorError::TooManyRules);
        }
        let mut hashes = BTreeSet::new();
        for rule in &rules {
            // Strict re-decode of the artifact runs the rule's own validation.
            ThresholdRuleV0::from_artifact(&rule.artifact())
                .map_err(TrustAnchorError::MalformedRule)?;
            if !hashes.insert(rule.rule_hash()) {
                return Err(TrustAnchorError::DuplicateRule);
            }
        }
        if pinned.is_some_and(|p| p.seq == 0) {
            return Err(TrustAnchorError::PinnedAtZero);
        }
        Ok(Self {
            domain,
            context,
            min_seq,
            pinned,
            rules,
            recognition,
        })
    }

    /// The governance domain this anchor serves.
    pub fn domain(&self) -> &GovernanceDomainId {
        &self.domain
    }

    /// The trusted authority context.
    pub fn context(&self) -> ContextRef {
        self.context
    }

    /// The relying party's frontier, derived from the anchor (never from evidence).
    pub fn frontier(&self) -> RelyingFrontierV0 {
        RelyingFrontierV0 {
            context: self.context,
            min_seq: self.min_seq,
            pinned: self.pinned,
        }
    }

    /// The recognition bases this anchor accepts.
    pub fn recognition_requirement(&self) -> RecognitionRequirement {
        self.recognition
    }
}

/// What a presenter supplies: the CAL evidence and one GEN-A genesis per Subject whose
/// recognition supports the electorate. GEN-A v1 has no wire codec, so this is an in-process
/// value; no new transport format is introduced here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstitutionalEvidenceV0 {
    /// The checkpointed authorization and the log it relies on.
    pub authorization: CheckpointedEvidenceV0,
    /// GEN-A genesis bundles from which GEN-B recognition is re-derived for the anchor's domain.
    pub recognitions: Vec<SubjectContextGenesisV1>,
}

/// Why an institutional authorization was refused. Every variant is a refusal; nothing is
/// consumed, reserved or executed when one is returned.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InstitutionalRefusal {
    /// The evidence is for a context other than the anchor's.
    #[error("evidence is not for the trusted authority context")]
    UntrustedContext,
    /// More recognition items than [`MAX_RECOGNITIONS`].
    #[error("too many recognition items")]
    TooManyRecognitions,
    /// A recognition item does not verify for the anchor's domain (foreign domain, wrong kind,
    /// bad GEN-A evidence).
    #[error("recognition item {index} is invalid for the trusted domain: {error}")]
    InvalidRecognition {
        /// Position in [`InstitutionalEvidenceV0::recognitions`].
        index: usize,
        /// Why.
        error: RecognitionError,
    },
    /// Two recognition items name the same Subject.
    #[error("recognition item {index} repeats a Subject already presented")]
    DuplicateRecognition {
        /// Position of the repeat in [`InstitutionalEvidenceV0::recognitions`].
        index: usize,
    },
    /// A recognition rests on a non-production basis and the anchor requires production grade.
    #[error("recognition item {index} is rehearsal-only; production grade required")]
    RehearsalNotProduction {
        /// Position in [`InstitutionalEvidenceV0::recognitions`].
        index: usize,
    },
    /// A Subject holding standing in an electorate the evidence relies on has no recognition by
    /// the anchor's domain.
    #[error("at {at}: subject in the electorate is not recognised by the trusted domain")]
    UnrecognizedStanding {
        /// Log position whose electorate contained it.
        at: u64,
        /// The Subject.
        subject: SubjectId,
    },
    /// The CAL refused the evidence (replay, pin, frontier, writer, rule, votes, verdict...).
    #[error("authority log refused: {0}")]
    Log(LogRefusal),
    /// The re-derived verdict is not `Pass`.
    #[error("verdict is not Pass")]
    VerdictNotPass,
}

/// A checkpointed authorization that passed the institutional gate. Historical validity only:
/// not admitted, not consumed, not executed. Fields are private; the only constructor is
/// [`verify_institutional_authorization`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedInstitutionalAuthorization {
    verified: VerifiedCheckpointedAuthorization,
    domain: GovernanceDomainId,
    voters: Vec<(SubjectId, RecognitionHash)>,
    legitimacy: Legitimacy,
}

impl VerifiedInstitutionalAuthorization {
    /// The CAL-verified authorization.
    pub fn verified(&self) -> &VerifiedCheckpointedAuthorization {
        &self.verified
    }

    /// The domain whose recognitions supported the electorate.
    pub fn domain(&self) -> &GovernanceDomainId {
        &self.domain
    }

    /// Each counted voter with the recognition that supports it, in input order.
    pub fn voters(&self) -> &[(SubjectId, RecognitionHash)] {
        &self.voters
    }

    /// How strong the supporting recognition is.
    pub fn legitimacy(&self) -> Legitimacy {
        self.legitimacy
    }
}

/// Recognitions re-derived for the anchor's domain, keyed by Subject.
struct RecognizedSubjects {
    by_subject: BTreeMap<SubjectId, RecognitionHash>,
    all_production: bool,
}

fn recognize(
    anchor: &InstitutionalTrustAnchorV0,
    geneses: &[SubjectContextGenesisV1],
) -> Result<RecognizedSubjects, InstitutionalRefusal> {
    if geneses.len() > MAX_RECOGNITIONS {
        return Err(InstitutionalRefusal::TooManyRecognitions);
    }
    let mut by_subject = BTreeMap::new();
    let mut all_production = true;
    for (index, genesis) in geneses.iter().enumerate() {
        // The Subject is derived by GEN-A verification against the anchor's domain bytes; the
        // presenter cannot name it.
        let recognition =
            SubjectRecognitionV1::recognize_bootstrap_rehearsal(&anchor.domain, genesis)
                .map_err(|error| InstitutionalRefusal::InvalidRecognition { index, error })?;
        let production = recognition.basis_kind().is_production_grade();
        if anchor.recognition == RecognitionRequirement::ProductionGrade && !production {
            return Err(InstitutionalRefusal::RehearsalNotProduction { index });
        }
        all_production &= production;
        // One item per Subject: which of two recognitions "counts" must not be the presenter's
        // ordering choice.
        if by_subject
            .insert(*recognition.subject(), *recognition.recognition_hash())
            .is_some()
        {
            return Err(InstitutionalRefusal::DuplicateRecognition { index });
        }
    }
    Ok(RecognizedSubjects {
        by_subject,
        all_production,
    })
}

/// An anchor rule that additionally refuses any electorate containing an unrecognised Subject.
/// Same rule hash as the wrapped rule: it is the relying party's implementation of that adopted
/// rule under its institutional precondition, not a different rule.
struct GatedRule<'a> {
    inner: &'a ThresholdRuleV0,
    recognized: &'a BTreeMap<SubjectId, RecognitionHash>,
}

impl AdoptedRule for GatedRule<'_> {
    fn rule_hash(&self) -> [u8; 32] {
        self.inner.rule_hash()
    }

    fn required_role(&self) -> &str {
        self.inner.required_role()
    }

    fn evaluate(
        &self,
        binding: &RuleBinding,
        eligible: &BTreeSet<SubjectId>,
        acts: &[VerifiedDeviceAct],
    ) -> Result<Verdict, RuleError> {
        // The whole electorate, not only the voters: quorum is computed over it, so one
        // unrecognised member changes what every vote means. Refuse rather than exclude.
        if let Some(subject) = eligible.iter().find(|s| !self.recognized.contains_key(s)) {
            return Err(RuleError::IneligibleSubject(*subject));
        }
        self.inner.evaluate(binding, eligible, acts)
    }
}

/// The resolver the gate hands to every verifier it runs, including the target's. Private: a
/// caller cannot assemble one around a recognition set of its choosing.
struct GatedRules<'a> {
    rules: Vec<GatedRule<'a>>,
}

impl<'a> GatedRules<'a> {
    fn new(anchor: &'a InstitutionalTrustAnchorV0, recognized: &'a RecognizedSubjects) -> Self {
        Self {
            rules: anchor
                .rules
                .iter()
                .map(|inner| GatedRule {
                    inner,
                    recognized: &recognized.by_subject,
                })
                .collect(),
        }
    }
}

impl RuleResolver for GatedRules<'_> {
    fn resolve(&self, rule_hash: &[u8; 32]) -> Option<&dyn AdoptedRule> {
        self.rules
            .iter()
            .find(|r| &r.rule_hash() == rule_hash)
            .map(|r| r as &dyn AdoptedRule)
    }
}

/// Translate the gate's refusal out of the CAL's generic error. Only an `IneligibleSubject` naming
/// a Subject the gate did not recognise is the gate's; anything else stays a log refusal.
fn classify(refusal: LogRefusal, recognized: &RecognizedSubjects) -> InstitutionalRefusal {
    match refusal {
        LogRefusal::RuleRefused {
            at,
            error: RuleError::IneligibleSubject(subject),
        } if !recognized.by_subject.contains_key(&subject) => {
            InstitutionalRefusal::UnrecognizedStanding { at, subject }
        }
        other => InstitutionalRefusal::Log(other),
    }
}

fn verify_with(
    anchor: &InstitutionalTrustAnchorV0,
    authorization: &CheckpointedEvidenceV0,
    recognized: &RecognizedSubjects,
) -> Result<VerifiedInstitutionalAuthorization, InstitutionalRefusal> {
    let rules = GatedRules::new(anchor, recognized);
    let verified = verify_checkpointed_authorization(authorization, &anchor.frontier(), &rules)
        .map_err(|r| classify(r, recognized))?;
    let auth = verified.authorization();
    if auth.verdict != Verdict::Pass {
        return Err(InstitutionalRefusal::VerdictNotPass);
    }
    // Every counted voter is in the electorate at the checkpoint (the CAL checked) and the whole
    // electorate is recognised (the gate checked), so each lookup succeeds; fail closed anyway.
    let voters = auth
        .inputs
        .iter()
        .map(|input| {
            recognized
                .by_subject
                .get(&input.subject)
                .map(|hash| (input.subject, *hash))
                .ok_or(InstitutionalRefusal::UnrecognizedStanding {
                    at: auth.checkpoint.seq,
                    subject: input.subject,
                })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(VerifiedInstitutionalAuthorization {
        domain: anchor.domain.clone(),
        legitimacy: if recognized.all_production {
            Legitimacy::ProductionGrade
        } else {
            Legitimacy::RehearsalOnly
        },
        verified,
        voters,
    })
}

/// Verify `evidence` against the relying party's `anchor`. Order, each fail-closed:
/// evidence context is the anchor's → every recognition item re-derives for the anchor's domain
/// (and meets the anchor's basis requirement) → full CAL verification with the anchor's frontier
/// and recognition-gated rules (every decision in the log and the authorization itself is
/// refused if its electorate contains an unrecognised Subject) → verdict `Pass`.
pub fn verify_institutional_authorization(
    anchor: &InstitutionalTrustAnchorV0,
    evidence: &InstitutionalEvidenceV0,
) -> Result<VerifiedInstitutionalAuthorization, InstitutionalRefusal> {
    if evidence.authorization.inception.context_ref() != anchor.context {
        return Err(InstitutionalRefusal::UntrustedContext);
    }
    let recognized = recognize(anchor, &evidence.recognitions)?;
    verify_with(anchor, &evidence.authorization, &recognized)
}

/// Why an institutional admission did not complete.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum InstitutionalAdmissionError<E> {
    /// The institutional gate refused before the target was invoked.
    #[error("institutional gate refused: {0}")]
    Refused(InstitutionalRefusal),
    /// The target ran (under the gate's frontier and gated rules) and returned an error.
    #[error("target refused or failed")]
    Target(E),
}

/// The institutional admission boundary: verify under `anchor`, then — only on success — call
/// `target` with the **anchor's** frontier and the **recognition-gated** resolver.
///
/// `target` is the custodian's admission entry point, e.g.
/// `|ev, frontier, rules| reservations.reserve_checkpointed(ev, frontier, rules, now, &request)`.
/// Because the target re-verifies with the gated resolver, the recognition gate runs inside the
/// target's own checks before any consumption or effect, not merely before the call.
///
/// The requester supplies only `evidence`. It cannot supply the domain, context, frontier, rules
/// or recognition requirement.
pub fn admit_institutional<T, E>(
    anchor: &InstitutionalTrustAnchorV0,
    evidence: &InstitutionalEvidenceV0,
    target: impl FnOnce(&CheckpointedEvidenceV0, &RelyingFrontierV0, &dyn RuleResolver) -> Result<T, E>,
) -> Result<(VerifiedInstitutionalAuthorization, T), InstitutionalAdmissionError<E>> {
    if evidence.authorization.inception.context_ref() != anchor.context {
        return Err(InstitutionalAdmissionError::Refused(
            InstitutionalRefusal::UntrustedContext,
        ));
    }
    let recognized =
        recognize(anchor, &evidence.recognitions).map_err(InstitutionalAdmissionError::Refused)?;
    let verified = verify_with(anchor, &evidence.authorization, &recognized)
        .map_err(InstitutionalAdmissionError::Refused)?;
    let rules = GatedRules::new(anchor, &recognized);
    let outcome = target(&evidence.authorization, &anchor.frontier(), &rules)
        .map_err(InstitutionalAdmissionError::Target)?;
    Ok((verified, outcome))
}

/// Domain separator for [`InstitutionalAdmissionVerifierV0::policy_ref`].
pub const INSTITUTIONAL_ADMISSION_POLICY_DOMAIN: &[u8] =
    b"icn.governance.institutional-admission-policy.v0";
/// Version of the institutional admission policy implemented here.
pub const INSTITUTIONAL_ADMISSION_POLICY_VERSION: u32 = 0;

/// The caller's untrusted input to [`InstitutionalAdmissionVerifierV0`]: the recognitions of the
/// electorate. Passed to the store opaquely as the reservation `supplement`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstitutionalSupplementV0 {
    pub recognitions: Vec<SubjectContextGenesisV1>,
}

/// Mandatory institutional admission at the resource target.
///
/// The custodian constructs this from its own [`InstitutionalTrustAnchorV0`] and installs it in
/// `icn_authz_store::LocalResourceReservations::with_admission`. A pool that declares
/// [`Self::policy_ref`] then accepts only reservations this verifier admitted, under the anchor's
/// frontier and the recognition-gated rules, and refuses caller-resolved reservations.
#[derive(Debug, Clone)]
pub struct InstitutionalAdmissionVerifierV0 {
    anchor: InstitutionalTrustAnchorV0,
}

impl InstitutionalAdmissionVerifierV0 {
    pub fn new(anchor: InstitutionalTrustAnchorV0) -> Self {
        Self { anchor }
    }

    pub fn anchor(&self) -> &InstitutionalTrustAnchorV0 {
        &self.anchor
    }

    /// The policy identity a pool declares to require *this* anchor's domain and context.
    ///
    /// It binds the governance domain id and the authority context, so a verifier anchored on
    /// one institution cannot satisfy a pool that names another. It does not bind the frontier
    /// or rules, which the custodian may advance without re-declaring its pools.
    pub fn policy_ref(anchor: &InstitutionalTrustAnchorV0) -> AdmissionPolicyRefV0 {
        // `u32be(len) || bytes` framing, matching the governance `header` convention.
        let mut h = Sha256::new();
        h.update((INSTITUTIONAL_ADMISSION_POLICY_DOMAIN.len() as u32).to_be_bytes());
        h.update(INSTITUTIONAL_ADMISSION_POLICY_DOMAIN);
        h.update((anchor.domain.0.len() as u32).to_be_bytes());
        h.update(anchor.domain.0.as_bytes());
        h.update(anchor.context.0);
        AdmissionPolicyRefV0 {
            id: h.finalize().into(),
            version: INSTITUTIONAL_ADMISSION_POLICY_VERSION,
        }
    }

    /// Full institutional verification plus the target's non-consuming preflight, both under the
    /// anchor's frontier and recognition-gated rules.
    pub fn verify(
        &self,
        authorization: &CheckpointedEvidenceV0,
        supplement: &InstitutionalSupplementV0,
        target: [u8; 32],
        now: u64,
    ) -> Result<(VerifiedInstitutionalAuthorization, VerifiedAdmissionV0), AdmissionRefusalV0> {
        let refuse = |e: InstitutionalRefusal| AdmissionRefusalV0(e.to_string());
        if authorization.inception.context_ref() != self.anchor.context {
            return Err(refuse(InstitutionalRefusal::UntrustedContext));
        }
        let recognized = recognize(&self.anchor, &supplement.recognitions).map_err(refuse)?;
        let verified = verify_with(&self.anchor, authorization, &recognized).map_err(refuse)?;
        let rules = GatedRules::new(&self.anchor, &recognized);
        let preflight =
            preflight_checkpointed(authorization, &self.anchor.frontier(), &rules, target, now)
                .map_err(|e| AdmissionRefusalV0(format!("target preflight refused: {e}")))?;
        let grade = match verified.legitimacy {
            Legitimacy::ProductionGrade => AdmissionGradeV0::Production,
            Legitimacy::RehearsalOnly => AdmissionGradeV0::Rehearsal,
        };
        Ok((verified, VerifiedAdmissionV0 { preflight, grade }))
    }
}

impl CustodianAdmissionVerifierV0 for InstitutionalAdmissionVerifierV0 {
    fn policy(&self) -> AdmissionPolicyRefV0 {
        Self::policy_ref(&self.anchor)
    }

    fn preflight(
        &self,
        evidence: &CheckpointedEvidenceV0,
        supplement: &dyn Any,
        target: [u8; 32],
        now: u64,
    ) -> Result<VerifiedAdmissionV0, AdmissionRefusalV0> {
        let Some(supplement) = supplement.downcast_ref::<InstitutionalSupplementV0>() else {
            return Err(AdmissionRefusalV0(
                "supplement is not an InstitutionalSupplementV0".into(),
            ));
        };
        self.verify(evidence, supplement, target, now)
            .map(|(_, admitted)| admitted)
    }
}
