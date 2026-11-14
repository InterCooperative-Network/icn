//! Independent verification of an authorization, and target-side admission.
//!
//! The writer's signature contributes *order and attribution* of the authorization, never its
//! truth: [`verify_authorization`] re-verifies every vote through the existing N4 evidence-bundle
//! verifier, re-checks every standing statement, re-evaluates the adopted rule, and requires the
//! result to equal the signed verdict over exactly the signed inputs.

use std::collections::{BTreeMap, BTreeSet};

use ed25519_dalek::SigningKey;
use icn_identity::authority_log::{DeviceCapability, PrincipalKey, SubjectId};
use icn_identity::device_authority::VerifiedDeviceAct;
use icn_identity::evidence_bundle::{verify_evidence_bundle, EvidenceVerifyError};
use thiserror::Error;

use super::codec::AuthzCodecError;
use super::statements::{
    roster_hash, sha256, target_id, AuthorizationEvidenceV0, AuthorizationV0, CanonicalV0,
    ContextRef, OutcomeReceiptV0, Signed, StandingV0, Verdict,
};

// ---------------------------------------------------------------------------------------------
// Adopted rule
// ---------------------------------------------------------------------------------------------

/// What a rule's acts must be bound to: the context and the matter decided.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleBinding {
    /// The context the authorization is in.
    pub context: ContextRef,
    /// The authorization's `subject_matter`.
    pub subject_matter: [u8; 32],
}

/// Why a rule refused to evaluate. Generic kinds only; rule meaning lives with the rule.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RuleError {
    /// The rule's own parameters are malformed.
    #[error("malformed rule artifact: {0}")]
    MalformedArtifact(&'static str),
    /// An act's payload is not a well-formed act for this rule.
    #[error("act payload is malformed for this rule")]
    MalformedAct {
        /// The act.
        act_id: [u8; 32],
    },
    /// An act is bound to another context or another subject matter (replay).
    #[error("act is bound to another context or subject matter")]
    ActNotBound {
        /// The act.
        act_id: [u8; 32],
    },
    /// One Subject supplied more than one act and the rule refuses that.
    #[error("subject supplied more than one act")]
    DuplicateSubject(SubjectId),
    /// An act's Subject is not in the eligible set.
    #[error("act subject is not eligible")]
    IneligibleSubject(SubjectId),
}

/// A pure, deterministic rule a context has adopted.
///
/// Implementations live **outside** this crate (governance meaning stays in its domain crate).
/// `evaluate` must depend only on its arguments.
pub trait AdoptedRule {
    /// `SHA-256` of the rule artifact this instance was built from.
    fn rule_hash(&self) -> [u8; 32];
    /// The standing role a Subject must hold for its act to count.
    fn required_role(&self) -> &str;
    /// Evaluate over `acts` (already verified, all from eligible Subjects, ascending by act id).
    fn evaluate(
        &self,
        binding: &RuleBinding,
        eligible: &BTreeSet<SubjectId>,
        acts: &[VerifiedDeviceAct],
    ) -> Result<Verdict, RuleError>;
}

// ---------------------------------------------------------------------------------------------
// Verification
// ---------------------------------------------------------------------------------------------

/// Which writer-signed statement a refusal is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatementKind {
    /// A [`StandingV0`].
    Standing,
    /// The rule adoption.
    RuleAdoption,
    /// The authorization.
    Authorization,
}

/// Why an authorization did not verify. Each variant names one failed check.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AuthzRefusal {
    /// The evidence bytes did not decode.
    #[error("evidence did not decode: {0}")]
    Codec(#[from] AuthzCodecError),
    /// The inception violates its structural invariants.
    #[error("invalid inception: {0}")]
    InvalidInception(AuthzCodecError),
    /// The designation names another context.
    #[error("designation is for another context")]
    DesignationContextMismatch,
    /// A designation signer is not a founder.
    #[error("designation signed by a non-founder")]
    DesignationSignerNotFounder,
    /// A founder signature on the designation does not verify.
    #[error("designation founder signature does not verify")]
    DesignationBadSignature,
    /// Signatures are not strictly ascending by founder (unsorted or repeated signer).
    #[error("designation signatures are not canonical")]
    DesignationSignaturesNotCanonical,
    /// Fewer valid founder signatures than the inception threshold.
    #[error("designation has {valid} founder signatures, threshold {threshold}")]
    DesignationBelowThreshold {
        /// Valid distinct founder signatures.
        valid: usize,
        /// Required.
        threshold: u8,
    },
    /// A writer-signed statement names another context.
    #[error("{0:?} is for another context")]
    ContextMismatch(StatementKind),
    /// A writer-signed statement is from another epoch than the designation.
    #[error("{0:?} is from another epoch than the designated writer")]
    EpochMismatch(StatementKind),
    /// A writer-signed statement's signature does not verify under the designated writer.
    #[error("{0:?} is not signed by the designated writer")]
    BadWriterSignature(StatementKind),
    /// `subject_matter` is not the commitment to the authorization's effect and operation id, so
    /// the votes would not bind what is being authorized.
    #[error("subject matter does not commit to the effect and operation id")]
    SubjectMatterNotBoundToEffect,
    /// The supplied standing statements are not the roster the authorization commits to (one was
    /// stripped, or an uncommitted one — e.g. an omitted member's — was attached).
    #[error("standing statements do not match the committed roster")]
    RosterMismatch,
    /// The authorization's rule is not the adopted rule.
    #[error("authorization rule is not the adopted rule")]
    RuleNotAdopted,
    /// The carried rule artifact does not hash to the adopted rule hash.
    #[error("rule artifact does not hash to the adopted rule")]
    RuleArtifactMismatch,
    /// The rule implementation supplied by the verifier is not the adopted rule.
    #[error("rule implementation is not the adopted rule")]
    RuleImplementationMismatch,
    /// Two standing statements for one Subject.
    #[error("duplicate standing for one subject")]
    DuplicateStanding(SubjectId),
    /// The authorization's inputs are not strictly ascending by act id.
    #[error("authorization inputs are not canonical")]
    InputsNotCanonical,
    /// Two supplied vote bundles carry the same act.
    #[error("duplicate vote act")]
    DuplicateAct([u8; 32]),
    /// The authorization counts an act whose bundle was not supplied.
    #[error("counted act has no supplied bundle")]
    InputActMissing([u8; 32]),
    /// A supplied, well-formed act is absent from the authorization's inputs — evidence that the
    /// writer omitted it (censorship) or that a stray act was attached. Never silently dropped.
    #[error("supplied act is not counted by the authorization")]
    UnlistedAct([u8; 32]),
    /// The existing N1 + N4-A verifier refused a vote.
    #[error("vote refused by the N4 verifier: {error}")]
    VoteRefused {
        /// The act.
        act_id: [u8; 32],
        /// The N4 verdict.
        error: EvidenceVerifyError,
    },
    /// A vote exercises a capability other than `Sign`.
    #[error("vote does not exercise the Sign capability")]
    WrongCapability([u8; 32]),
    /// A voter has no standing statement.
    #[error("voter has no standing")]
    NoStanding(SubjectId),
    /// A voter's standing has expired.
    #[error("voter standing expired at {not_after} (now {now})")]
    StandingExpired {
        /// The voter.
        subject: SubjectId,
        /// Standing end.
        not_after: u64,
        /// Verification time.
        now: u64,
    },
    /// A voter's standing is for another role than the rule requires.
    #[error("voter standing is not for the rule's role")]
    StandingRoleMismatch(SubjectId),
    /// The adopted rule refused to evaluate.
    #[error("rule refused: {0}")]
    RuleRefused(RuleError),
    /// Re-evaluating the adopted rule did not reproduce the signed verdict.
    #[error("signed verdict {signed:?} but the rule evaluates to {evaluated:?}")]
    VerdictMismatch {
        /// What the writer signed.
        signed: Verdict,
        /// What the verifier computed.
        evaluated: Verdict,
    },
}

/// An authorization every check of [`verify_authorization`] passed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedAuthorization {
    authorization: Signed<AuthorizationV0>,
    authorization_hash: [u8; 32],
    writer: PrincipalKey,
}

impl VerifiedAuthorization {
    /// The verified authorization body.
    pub fn authorization(&self) -> &AuthorizationV0 {
        &self.authorization.body
    }

    /// The signed authorization.
    pub fn signed(&self) -> &Signed<AuthorizationV0> {
        &self.authorization
    }

    /// `SHA-256(canonical AuthorizationV0)`.
    pub fn authorization_hash(&self) -> [u8; 32] {
        self.authorization_hash
    }

    /// The designated writer that signed it.
    pub fn writer(&self) -> PrincipalKey {
        self.writer
    }
}

fn check_writer_statement<T: CanonicalV0>(
    statement: &Signed<T>,
    context: ContextRef,
    epoch: u64,
    stmt_context: ContextRef,
    stmt_epoch: u64,
    writer: &PrincipalKey,
    kind: StatementKind,
) -> Result<(), AuthzRefusal> {
    if stmt_context != context {
        return Err(AuthzRefusal::ContextMismatch(kind));
    }
    if stmt_epoch != epoch {
        return Err(AuthzRefusal::EpochMismatch(kind));
    }
    if !statement.verifies_under(writer) {
        return Err(AuthzRefusal::BadWriterSignature(kind));
    }
    Ok(())
}

/// Verify an authorization from its evidence alone. No node, store, network or secret.
///
/// `rule` is the verifier's own implementation of the adopted rule (built from
/// `evidence.rule_artifact`); `now` is the verifier's clock, used only for standing expiry.
///
/// Order, each fail-closed: inception → designation (founder threshold) → authorization and rule
/// adoption under the designated writer → rule hash chain → standing → input set equals supplied
/// set → each vote through the **unchanged** N4 verifier at the writer-signed (subject, position)
/// → standing in force for each voter → rule re-evaluation → verdict equality.
pub fn verify_authorization(
    evidence: &AuthorizationEvidenceV0,
    rule: &dyn AdoptedRule,
    now: u64,
) -> Result<VerifiedAuthorization, AuthzRefusal> {
    // Context.
    evidence
        .inception
        .validate()
        .map_err(AuthzRefusal::InvalidInception)?;
    let context = evidence.inception.context_ref();

    // Writer designation: a threshold of distinct founders.
    let designation = &evidence.designation;
    if designation.body.context != context {
        return Err(AuthzRefusal::DesignationContextMismatch);
    }
    if !designation
        .signatures
        .windows(2)
        .all(|w| w[0].founder.as_bytes() < w[1].founder.as_bytes())
    {
        return Err(AuthzRefusal::DesignationSignaturesNotCanonical);
    }
    let designation_bytes = designation.body.canonical_bytes();
    for s in &designation.signatures {
        if !evidence.inception.is_founder(&s.founder) {
            return Err(AuthzRefusal::DesignationSignerNotFounder);
        }
        s.founder
            .verifying_key()
            .verify_strict(
                &designation_bytes,
                &ed25519_dalek::Signature::from_bytes(&s.signature),
            )
            .map_err(|_| AuthzRefusal::DesignationBadSignature)?;
    }
    if designation.signatures.len() < usize::from(evidence.inception.threshold) {
        return Err(AuthzRefusal::DesignationBelowThreshold {
            valid: designation.signatures.len(),
            threshold: evidence.inception.threshold,
        });
    }
    let epoch = designation.body.epoch;
    let writer = designation.body.writer;

    // The authorization and the rule adoption, under the designated writer.
    let auth = &evidence.authorization.body;
    check_writer_statement(
        &evidence.authorization,
        context,
        epoch,
        auth.context,
        auth.epoch,
        &writer,
        StatementKind::Authorization,
    )?;
    let adoption = &evidence.rule_adoption.body;
    check_writer_statement(
        &evidence.rule_adoption,
        context,
        epoch,
        adoption.context,
        adoption.epoch,
        &writer,
        StatementKind::RuleAdoption,
    )?;

    // The votes must bind the exact effect and operation id being authorized.
    if auth.subject_matter != auth.effect.commitment(context, auth.operation_id) {
        return Err(AuthzRefusal::SubjectMatterNotBoundToEffect);
    }

    // Rule hash chain: authorization → adoption → artifact → implementation.
    if auth.rule_hash != adoption.rule_hash {
        return Err(AuthzRefusal::RuleNotAdopted);
    }
    if sha256(&evidence.rule_artifact) != adoption.rule_hash {
        return Err(AuthzRefusal::RuleArtifactMismatch);
    }
    if rule.rule_hash() != adoption.rule_hash {
        return Err(AuthzRefusal::RuleImplementationMismatch);
    }

    // Standing.
    let mut standing: BTreeMap<SubjectId, &StandingV0> = BTreeMap::new();
    for s in &evidence.standing {
        check_writer_statement(
            s,
            context,
            epoch,
            s.body.context,
            s.body.epoch,
            &writer,
            StatementKind::Standing,
        )?;
        if standing.insert(s.body.subject, &s.body).is_some() {
            return Err(AuthzRefusal::DuplicateStanding(s.body.subject));
        }
    }
    let bodies: Vec<StandingV0> = evidence.standing.iter().map(|s| s.body.clone()).collect();
    if roster_hash(&bodies) != auth.roster_hash {
        return Err(AuthzRefusal::RosterMismatch);
    }

    // The supplied vote set must equal the signed input set.
    if !auth.inputs_canonical() {
        return Err(AuthzRefusal::InputsNotCanonical);
    }
    let mut supplied = BTreeMap::new();
    for bundle in &evidence.vote_bundles {
        let act_id = bundle.act().act.act_id();
        if supplied.insert(act_id, bundle).is_some() {
            return Err(AuthzRefusal::DuplicateAct(act_id));
        }
    }
    // Supplied ⊆ listed here; listed ⊆ supplied is enforced by the lookup in the loop below.
    let listed: BTreeSet<[u8; 32]> = auth.inputs.iter().map(|i| i.act_id).collect();
    if let Some(unlisted) = supplied.keys().find(|id| !listed.contains(*id)) {
        return Err(AuthzRefusal::UnlistedAct(*unlisted));
    }

    // Each vote through the unchanged N1 + N4-A verifier, at the writer-signed relying position.
    let mut acts = Vec::with_capacity(auth.inputs.len());
    for input in &auth.inputs {
        // A counted act with no supplied bundle (stripped in transit, or never attached).
        let bundle = supplied
            .get(&input.act_id)
            .ok_or(AuthzRefusal::InputActMissing(input.act_id))?;
        let verified =
            verify_evidence_bundle(bundle, input.subject, input.position).map_err(|error| {
                AuthzRefusal::VoteRefused {
                    act_id: input.act_id,
                    error,
                }
            })?;
        if verified.act().capability != DeviceCapability::Sign {
            return Err(AuthzRefusal::WrongCapability(input.act_id));
        }
        acts.push(verified);
    }

    // Standing in force, for the rule's role, for every voter.
    let role = rule.required_role();
    let eligible: BTreeSet<SubjectId> = standing
        .values()
        .filter(|s| now <= s.not_after && s.role == role)
        .map(|s| s.subject)
        .collect();
    for act in &acts {
        let subject = act.act().subject;
        let s = standing
            .get(&subject)
            .ok_or(AuthzRefusal::NoStanding(subject))?;
        if now > s.not_after {
            return Err(AuthzRefusal::StandingExpired {
                subject,
                not_after: s.not_after,
                now,
            });
        }
        if s.role != role {
            return Err(AuthzRefusal::StandingRoleMismatch(subject));
        }
    }

    // Re-evaluate; the signed verdict must be reproduced.
    let binding = RuleBinding {
        context,
        subject_matter: auth.subject_matter,
    };
    let evaluated = rule
        .evaluate(&binding, &eligible, &acts)
        .map_err(AuthzRefusal::RuleRefused)?;
    if evaluated != auth.verdict {
        return Err(AuthzRefusal::VerdictMismatch {
            signed: auth.verdict,
            evaluated,
        });
    }

    Ok(VerifiedAuthorization {
        authorization_hash: auth.hash(),
        authorization: evidence.authorization.clone(),
        writer,
    })
}

/// [`AuthorizationEvidenceV0::decode`] then [`verify_authorization`], for a holder of bytes only.
pub fn verify_authorization_bytes(
    bytes: &[u8],
    rule: &dyn AdoptedRule,
    now: u64,
) -> Result<VerifiedAuthorization, AuthzRefusal> {
    let evidence = AuthorizationEvidenceV0::decode(bytes)?;
    verify_authorization(&evidence, rule, now)
}

// ---------------------------------------------------------------------------------------------
// Consumption (v0: trait + in-memory; durable consumption is ADR-0091's, not anticipated here)
// ---------------------------------------------------------------------------------------------

/// What a target remembers about a consumed operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsumedEntry {
    /// The authorization that consumed it.
    pub authorization: Signed<AuthorizationV0>,
    /// The writer that signed it.
    pub writer: PrincipalKey,
}

/// A consumed-set backend failed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("consumed-set backend failure: {0}")]
pub struct ConsumedSetError(pub String);

/// Operation ids a target has consumed, keyed by `(context, operation_id)`.
pub trait ConsumedSet {
    /// Atomically: if `(context, operation_id)` is absent, record `entry` and return `None`;
    /// otherwise leave it unchanged and return the existing entry.
    fn consume(
        &mut self,
        context: ContextRef,
        operation_id: [u8; 32],
        entry: ConsumedEntry,
    ) -> Result<Option<ConsumedEntry>, ConsumedSetError>;
}

/// In-memory [`ConsumedSet`]. Lost on restart — **not** durable replay protection.
#[derive(Debug, Default)]
pub struct InMemoryConsumedSet {
    entries: BTreeMap<(ContextRef, [u8; 32]), ConsumedEntry>,
}

impl ConsumedSet for InMemoryConsumedSet {
    fn consume(
        &mut self,
        context: ContextRef,
        operation_id: [u8; 32],
        entry: ConsumedEntry,
    ) -> Result<Option<ConsumedEntry>, ConsumedSetError> {
        match self.entries.get(&(context, operation_id)) {
            Some(existing) => Ok(Some(existing.clone())),
            None => {
                self.entries.insert((context, operation_id), entry);
                Ok(None)
            }
        }
    }
}

/// Two different authorizations, each signed by a designated writer of one context, claiming one
/// operation id. Verifiable by a third party holding the two writer keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquivocationProof {
    /// The earlier-seen authorization and its writer.
    pub first: ConsumedEntry,
    /// The conflicting authorization and its writer.
    pub second: ConsumedEntry,
}

impl EquivocationProof {
    /// Both signatures verify under their writers, both name one context and one operation id,
    /// and the bodies differ. (Whether each writer was designated needs the designations; v0
    /// leaves that to the holder.)
    pub fn is_valid(&self) -> bool {
        let (a, b) = (&self.first.authorization, &self.second.authorization);
        a.verifies_under(&self.first.writer)
            && b.verifies_under(&self.second.writer)
            && a.body.context == b.body.context
            && a.body.operation_id == b.body.operation_id
            && a.body != b.body
    }
}

/// Why a target refused to admit.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AdmissionRefusal {
    /// The authorization did not verify.
    #[error("verification refused: {0}")]
    Verify(#[from] AuthzRefusal),
    /// The evidence is for a context this target does not serve. Anyone can found a context;
    /// only the target's own pinned context may direct it.
    #[error("authorization is for a context this target does not serve")]
    UntrustedContext,
    /// It verified, but the verdict is not `Pass`.
    #[error("verdict is not Pass")]
    VerdictNotPass,
    /// The effect names another target.
    #[error("effect targets another custodian")]
    WrongTarget,
    /// `now > not_after`.
    #[error("authorization expired at {not_after} (now {now})")]
    Expired {
        /// Authorization end.
        not_after: u64,
        /// Admission time.
        now: u64,
    },
    /// Issued under an epoch older than one this target already knows.
    #[error("epoch {epoch} is older than the latest known {latest_known}")]
    StaleEpoch {
        /// Authorization epoch.
        epoch: u64,
        /// Latest epoch the target knows.
        latest_known: u64,
    },
    /// This exact authorization already consumed its operation id.
    #[error("operation already consumed by this authorization")]
    Replayed,
    /// A different authorization already consumed this operation id.
    #[error("operation id already consumed by a different authorization")]
    Equivocation(Box<EquivocationProof>),
    /// The consumed-set backend failed; fail closed.
    #[error("{0}")]
    ConsumedSet(ConsumedSetError),
}

/// A successful, consumed admission. The only way to obtain one is [`admit`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Admission {
    verified: VerifiedAuthorization,
}

impl Admission {
    /// The admitted authorization.
    pub fn verified(&self) -> &VerifiedAuthorization {
        &self.verified
    }
}

/// Target-side admission. Requires the evidence to be for `expected_context` — the context this
/// target is configured to serve, which is its trust anchor — then re-verifies everything
/// ([`verify_authorization`]) and requires `Pass`, `effect.target == my_target`,
/// `now <= not_after`, `epoch >= latest_epoch_known` (the latest epoch known **for
/// `expected_context`**), and only then consumes `(context, operation_id)`. No check consumes on
/// refusal.
///
/// Without the pin, anyone could found a context with their own founders, writer and members, pass
/// a vote in it, and name this target.
#[allow(clippy::too_many_arguments)]
pub fn admit(
    evidence: &AuthorizationEvidenceV0,
    rule: &dyn AdoptedRule,
    expected_context: ContextRef,
    my_target: [u8; 32],
    now: u64,
    latest_epoch_known: u64,
    consumed: &mut dyn ConsumedSet,
) -> Result<Admission, AdmissionRefusal> {
    if evidence.inception.context_ref() != expected_context {
        return Err(AdmissionRefusal::UntrustedContext);
    }
    let verified = verify_authorization(evidence, rule, now)?;
    let auth = verified.authorization();
    if auth.verdict != Verdict::Pass {
        return Err(AdmissionRefusal::VerdictNotPass);
    }
    if auth.effect.target != my_target {
        return Err(AdmissionRefusal::WrongTarget);
    }
    if now > auth.not_after {
        return Err(AdmissionRefusal::Expired {
            not_after: auth.not_after,
            now,
        });
    }
    if auth.epoch < latest_epoch_known {
        return Err(AdmissionRefusal::StaleEpoch {
            epoch: auth.epoch,
            latest_known: latest_epoch_known,
        });
    }
    let entry = ConsumedEntry {
        authorization: verified.signed().clone(),
        writer: verified.writer(),
    };
    match consumed
        .consume(auth.context, auth.operation_id, entry.clone())
        .map_err(AdmissionRefusal::ConsumedSet)?
    {
        None => Ok(Admission { verified }),
        Some(prior) if prior.authorization.body == entry.authorization.body => {
            Err(AdmissionRefusal::Replayed)
        }
        Some(prior) => Err(AdmissionRefusal::Equivocation(Box::new(
            EquivocationProof {
                first: prior,
                second: entry,
            },
        ))),
    }
}

// ---------------------------------------------------------------------------------------------
// Outcome receipt (boundary only: v0 runs no real effect)
// ---------------------------------------------------------------------------------------------

/// Issue a receipt for an admission, signed by the custodian.
pub fn issue_outcome_receipt(
    admission: &Admission,
    custodian_key: &SigningKey,
    result_hash: [u8; 32],
) -> Result<Signed<OutcomeReceiptV0>, AuthzCodecError> {
    let custodian = PrincipalKey::try_from_verifying_key(custodian_key.verifying_key())
        .map_err(|_| AuthzCodecError::InvalidPrincipal { field: "custodian" })?;
    let auth = admission.verified.authorization();
    Ok(Signed::sign(
        OutcomeReceiptV0 {
            operation_id: auth.operation_id,
            authorization_hash: admission.verified.authorization_hash(),
            custodian,
            result_hash,
        },
        custodian_key,
    ))
}

/// Why a receipt did not verify.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum ReceiptRefusal {
    /// The custodian signature does not verify.
    #[error("receipt signature does not verify")]
    BadSignature,
    /// The signer is not the authorization's target.
    #[error("receipt signer is not the authorization's target")]
    WrongCustodian,
    /// Another operation.
    #[error("receipt is for another operation")]
    OperationMismatch,
    /// Another authorization.
    #[error("receipt is for another authorization")]
    AuthorizationMismatch,
}

/// A third party checks a receipt against a verified authorization.
pub fn verify_outcome_receipt(
    receipt: &Signed<OutcomeReceiptV0>,
    authorization: &VerifiedAuthorization,
) -> Result<(), ReceiptRefusal> {
    let body = &receipt.body;
    if !receipt.verifies_under(&body.custodian) {
        return Err(ReceiptRefusal::BadSignature);
    }
    let auth = authorization.authorization();
    if target_id(&body.custodian) != auth.effect.target {
        return Err(ReceiptRefusal::WrongCustodian);
    }
    if body.operation_id != auth.operation_id {
        return Err(ReceiptRefusal::OperationMismatch);
    }
    if body.authorization_hash != authorization.authorization_hash() {
        return Err(ReceiptRefusal::AuthorizationMismatch);
    }
    Ok(())
}
