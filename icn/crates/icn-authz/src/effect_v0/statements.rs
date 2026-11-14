//! The v0 statements: context inception, writer designation, standing, rule adoption, effect,
//! authorization, outcome receipt, and the evidence container that carries them.
//!
//! Every signed body begins with its own `icn.authz.v0.*` domain string and the v0 version, so a
//! signature over one kind of body can never verify as another kind, nor as an N1 event or an N4
//! device act (whose domains are `icn.n1.*` / `icn.n4.*`).

use ed25519_dalek::{Signature, Signer, SigningKey};
use icn_identity::authority_log::{PrincipalKey, SubjectId};
use icn_identity::evidence_bundle::EvidenceBundle;
use sha2::{Digest, Sha256};

use super::codec::{AuthzCodecError, Reader, Writer};

/// Version carried by every v0 encoding.
pub const AUTHZ_V0_VERSION: u16 = 0;

/// Domain for [`ContextRef`] derivation.
pub const CONTEXT_REF_DOMAIN: &[u8] = b"icn.authz.v0.context";
/// Domain of [`ContextInceptionV0`] bytes.
pub const INCEPTION_DOMAIN: &[u8] = b"icn.authz.v0.inception";
/// Domain of [`WriterDesignationV0`] bytes.
pub const DESIGNATION_DOMAIN: &[u8] = b"icn.authz.v0.designation";
/// Domain of [`StandingV0`] bytes.
pub const STANDING_DOMAIN: &[u8] = b"icn.authz.v0.standing";
/// Domain of [`RuleAdoptionV0`] bytes.
pub const RULE_ADOPTION_DOMAIN: &[u8] = b"icn.authz.v0.rule-adoption";
/// Domain of [`AuthorizationV0`] bytes.
pub const AUTHORIZATION_DOMAIN: &[u8] = b"icn.authz.v0.authorization";
/// Domain of [`OutcomeReceiptV0`] bytes.
pub const OUTCOME_DOMAIN: &[u8] = b"icn.authz.v0.outcome";
/// Domain of [`AuthorizationEvidenceV0`] bytes.
pub const EVIDENCE_DOMAIN: &[u8] = b"icn.authz.v0.evidence";
/// Domain of [`target_id`] derivation.
pub const TARGET_DOMAIN: &[u8] = b"icn.authz.v0.target";
/// Domain of [`EffectV0::commitment`].
pub const SUBJECT_MATTER_DOMAIN: &[u8] = b"icn.authz.v0.subject-matter";
/// Domain of [`roster_hash`].
pub const ROSTER_DOMAIN: &[u8] = b"icn.authz.v0.roster";

/// Maximum founders of one context.
pub const MAX_FOUNDERS: usize = 64;
/// Maximum bytes of a role label.
pub const MAX_ROLE_LEN: usize = 64;
/// Maximum bytes of an effect action label.
pub const MAX_ACTION_LEN: usize = 64;
/// Maximum inputs to one authorization, and standing statements / vote bundles per evidence.
pub const MAX_INPUTS: usize = 1024;
/// Maximum bytes of a rule artifact.
pub const MAX_RULE_ARTIFACT: usize = 4096;
/// Maximum bytes of one carried vote bundle.
pub const MAX_VOTE_BUNDLE: usize = 1 << 20;
/// Maximum bytes of one carried signed statement.
const MAX_STATEMENT: usize = 64 * 1024;

pub(crate) fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

fn write_principal(w: &mut Writer, key: &PrincipalKey) {
    w.b32(&key.as_bytes());
}

fn read_principal(
    r: &mut Reader<'_>,
    field: &'static str,
) -> Result<PrincipalKey, AuthzCodecError> {
    PrincipalKey::try_from_bytes(r.b32(field)?)
        .map_err(|_| AuthzCodecError::InvalidPrincipal { field })
}

fn check_len(field: &'static str, len: usize, max: usize) -> Result<(), AuthzCodecError> {
    if len > max {
        return Err(AuthzCodecError::TooLong { field, len, max });
    }
    Ok(())
}

/// The id of an execution target that is a single custodian principal:
/// `SHA-256(lp(TARGET_DOMAIN) || key)`. An [`EffectV0::target`] naming a custodian uses this, and
/// [`super::verify_outcome_receipt`] checks a receipt's signer against it.
pub fn target_id(custodian: &PrincipalKey) -> [u8; 32] {
    let mut w = Writer::new();
    w.lp(TARGET_DOMAIN);
    write_principal(&mut w, custodian);
    sha256(&w.finish())
}

/// A body type with one canonical encoding.
pub trait CanonicalV0: Sized {
    /// The canonical bytes — the exact bytes signed and hashed.
    fn canonical_bytes(&self) -> Vec<u8>;
    /// Strict decode; the inverse of [`CanonicalV0::canonical_bytes`].
    fn decode(bytes: &[u8]) -> Result<Self, AuthzCodecError>;
    /// `SHA-256(canonical_bytes)`.
    fn hash(&self) -> [u8; 32] {
        sha256(&self.canonical_bytes())
    }
}

// ---------------------------------------------------------------------------------------------
// Context
// ---------------------------------------------------------------------------------------------

/// A context's identity: `SHA-256(lp(CONTEXT_REF_DOMAIN) || canonical(ContextInceptionV0))`.
/// Self-certifying: nobody signs an inception; whoever holds it can recompute the ref.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContextRef(pub [u8; 32]);

/// The founding statement of a context: who may designate its writer, and how many must agree.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextInceptionV0 {
    /// Founder principals, strictly ascending by key bytes, 1..=[`MAX_FOUNDERS`].
    pub founders: Vec<PrincipalKey>,
    /// Founder signatures required for a writer designation: `founders.len() / 2 < threshold <=
    /// founders.len()` (any two founder quorums intersect).
    pub threshold: u8,
    /// Hash of an off-protocol label (name, charter text). Bound, never interpreted.
    pub label_hash: [u8; 32],
}

impl ContextInceptionV0 {
    /// Build an inception, sorting founders. Refuses an empty, oversized or duplicated founder set
    /// and an out-of-range threshold.
    pub fn new(
        mut founders: Vec<PrincipalKey>,
        threshold: u8,
        label_hash: [u8; 32],
    ) -> Result<Self, AuthzCodecError> {
        founders.sort_by_key(|k| k.as_bytes());
        let inception = ContextInceptionV0 {
            founders,
            threshold,
            label_hash,
        };
        inception.validate()?;
        Ok(inception)
    }

    /// Check the structural invariants that construction and decode enforce.
    pub fn validate(&self) -> Result<(), AuthzCodecError> {
        if self.founders.is_empty() {
            return Err(AuthzCodecError::InvalidValue { field: "founders" });
        }
        check_len("founders", self.founders.len(), MAX_FOUNDERS)?;
        if !self
            .founders
            .windows(2)
            .all(|w| w[0].as_bytes() < w[1].as_bytes())
        {
            return Err(AuthzCodecError::NotStrictlyAscending { field: "founders" });
        }
        // Quorum intersection: `2·threshold > founders`. Any two founder quorums then share a
        // founder, so two conflicting writer designations for one epoch always carry a signature
        // from someone who signed both (`designation_equivocators`). Without it, disjoint founder
        // sets designate different writers with nobody misbehaving. Arrangements that want
        // non-intersecting quorums (e.g. 1-of-2) need a multi-writer model, not this one.
        let t = usize::from(self.threshold);
        if t == 0 || t > self.founders.len() || 2 * t <= self.founders.len() {
            return Err(AuthzCodecError::InvalidValue { field: "threshold" });
        }
        Ok(())
    }

    /// This context's [`ContextRef`].
    pub fn context_ref(&self) -> ContextRef {
        let mut w = Writer::new();
        w.lp(CONTEXT_REF_DOMAIN);
        w.lp(&self.canonical_bytes());
        ContextRef(sha256(&w.finish()))
    }

    /// Whether `key` is a founder.
    pub fn is_founder(&self, key: &PrincipalKey) -> bool {
        self.founders.iter().any(|f| f.as_bytes() == key.as_bytes())
    }
}

impl CanonicalV0 for ContextInceptionV0 {
    fn canonical_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.lp(INCEPTION_DOMAIN);
        w.u16(AUTHZ_V0_VERSION);
        w.count(self.founders.len());
        for f in &self.founders {
            write_principal(&mut w, f);
        }
        w.u8(self.threshold);
        w.b32(&self.label_hash);
        w.finish()
    }

    fn decode(bytes: &[u8]) -> Result<Self, AuthzCodecError> {
        let mut r = Reader::new(bytes);
        r.expect_domain(INCEPTION_DOMAIN, "domain")?;
        r.expect_version(AUTHZ_V0_VERSION)?;
        let n = r.count("founders", MAX_FOUNDERS)?;
        let mut founders = Vec::with_capacity(n);
        for _ in 0..n {
            founders.push(read_principal(&mut r, "founder")?);
        }
        let threshold = r.u8("threshold")?;
        let label_hash = r.b32("label_hash")?;
        r.finish()?;
        let inception = ContextInceptionV0 {
            founders,
            threshold,
            label_hash,
        };
        inception.validate()?;
        Ok(inception)
    }
}

// ---------------------------------------------------------------------------------------------
// Writer designation (provisional single writer per context and epoch)
// ---------------------------------------------------------------------------------------------

/// Founders designate the one writer of `context` for `epoch`. **Provisional**: the single-writer
/// model is a working hypothesis (writer replacement, T9, is unvalidated).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WriterDesignationV0 {
    /// The context.
    pub context: ContextRef,
    /// The epoch, `>= 1`. A later epoch supersedes an earlier one.
    pub epoch: u64,
    /// The designated writer.
    pub writer: PrincipalKey,
    /// The head of the writer's sequence at the start of the epoch. Bound, not interpreted in v0.
    pub start_head: [u8; 32],
}

impl CanonicalV0 for WriterDesignationV0 {
    fn canonical_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.lp(DESIGNATION_DOMAIN);
        w.u16(AUTHZ_V0_VERSION);
        w.b32(&self.context.0);
        w.u64(self.epoch);
        write_principal(&mut w, &self.writer);
        w.b32(&self.start_head);
        w.finish()
    }

    fn decode(bytes: &[u8]) -> Result<Self, AuthzCodecError> {
        let mut r = Reader::new(bytes);
        r.expect_domain(DESIGNATION_DOMAIN, "domain")?;
        r.expect_version(AUTHZ_V0_VERSION)?;
        let context = ContextRef(r.b32("context")?);
        let epoch = r.u64("epoch")?;
        if epoch == 0 {
            return Err(AuthzCodecError::InvalidValue { field: "epoch" });
        }
        let writer = read_principal(&mut r, "writer")?;
        let start_head = r.b32("start_head")?;
        r.finish()?;
        Ok(WriterDesignationV0 {
            context,
            epoch,
            writer,
            start_head,
        })
    }
}

/// One founder's signature over a designation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FounderSignature {
    /// The signing founder.
    pub founder: PrincipalKey,
    /// Ed25519 signature over the designation's canonical bytes.
    pub signature: [u8; 64],
}

/// A designation with founder signatures, strictly ascending by founder key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedDesignation {
    /// The designation.
    pub body: WriterDesignationV0,
    /// Founder signatures, strictly ascending by founder key bytes (no signer twice).
    pub signatures: Vec<FounderSignature>,
}

impl SignedDesignation {
    /// Sign `body` with each founder key, ordering signatures canonically.
    pub fn sign(
        body: WriterDesignationV0,
        founder_keys: &[&SigningKey],
    ) -> Result<Self, AuthzCodecError> {
        let bytes = body.canonical_bytes();
        let mut signatures = Vec::with_capacity(founder_keys.len());
        for key in founder_keys {
            let founder = PrincipalKey::try_from_verifying_key(key.verifying_key())
                .map_err(|_| AuthzCodecError::InvalidPrincipal { field: "founder" })?;
            signatures.push(FounderSignature {
                founder,
                signature: key.sign(&bytes).to_bytes(),
            });
        }
        signatures.sort_by_key(|s| s.founder.as_bytes());
        Ok(SignedDesignation { body, signatures })
    }

    /// Canonical bytes: `lp(body) || count || (founder || sig)*`.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.lp(&self.body.canonical_bytes());
        w.count(self.signatures.len());
        for s in &self.signatures {
            write_principal(&mut w, &s.founder);
            w.b64(&s.signature);
        }
        w.finish()
    }

    /// Strict decode. Refuses signatures that are not strictly ascending by founder.
    pub fn decode(bytes: &[u8]) -> Result<Self, AuthzCodecError> {
        let mut r = Reader::new(bytes);
        let body = WriterDesignationV0::decode(r.lp("designation", MAX_STATEMENT)?)?;
        let n = r.count("designation_signatures", MAX_FOUNDERS)?;
        let mut signatures = Vec::with_capacity(n);
        for _ in 0..n {
            let founder = read_principal(&mut r, "founder")?;
            let signature = r.b64("founder_signature")?;
            signatures.push(FounderSignature { founder, signature });
        }
        r.finish()?;
        if !signatures
            .windows(2)
            .all(|w| w[0].founder.as_bytes() < w[1].founder.as_bytes())
        {
            return Err(AuthzCodecError::NotStrictlyAscending {
                field: "designation_signatures",
            });
        }
        Ok(SignedDesignation { body, signatures })
    }
}

// ---------------------------------------------------------------------------------------------
// Writer-signed statements
// ---------------------------------------------------------------------------------------------

/// The writer's statement that `subject` holds `role` in `context` until `not_after`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StandingV0 {
    /// The context.
    pub context: ContextRef,
    /// The writer epoch this statement was issued in.
    pub epoch: u64,
    /// The Subject (N1/N4 `SubjectId`; not a legacy `Did`).
    pub subject: SubjectId,
    /// An opaque role label, at most [`MAX_ROLE_LEN`] bytes. Compared, never interpreted.
    pub role: String,
    /// Caller-clock instant after which the standing is not in force.
    pub not_after: u64,
}

impl CanonicalV0 for StandingV0 {
    fn canonical_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.lp(STANDING_DOMAIN);
        w.u16(AUTHZ_V0_VERSION);
        w.b32(&self.context.0);
        w.u64(self.epoch);
        w.b32(self.subject.as_bytes());
        w.lp(self.role.as_bytes());
        w.u64(self.not_after);
        w.finish()
    }

    fn decode(bytes: &[u8]) -> Result<Self, AuthzCodecError> {
        let mut r = Reader::new(bytes);
        r.expect_domain(STANDING_DOMAIN, "domain")?;
        r.expect_version(AUTHZ_V0_VERSION)?;
        let context = ContextRef(r.b32("context")?);
        let epoch = r.u64("epoch")?;
        let subject = SubjectId::from_bytes(r.b32("subject")?);
        let role = r.text("role", MAX_ROLE_LEN)?;
        let not_after = r.u64("not_after")?;
        r.finish()?;
        Ok(StandingV0 {
            context,
            epoch,
            subject,
            role,
            not_after,
        })
    }
}

/// The writer's statement that the rule whose artifact hashes to `rule_hash` is adopted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleAdoptionV0 {
    /// The context.
    pub context: ContextRef,
    /// The writer epoch.
    pub epoch: u64,
    /// `SHA-256(rule artifact)`.
    pub rule_hash: [u8; 32],
}

impl CanonicalV0 for RuleAdoptionV0 {
    fn canonical_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.lp(RULE_ADOPTION_DOMAIN);
        w.u16(AUTHZ_V0_VERSION);
        w.b32(&self.context.0);
        w.u64(self.epoch);
        w.b32(&self.rule_hash);
        w.finish()
    }

    fn decode(bytes: &[u8]) -> Result<Self, AuthzCodecError> {
        let mut r = Reader::new(bytes);
        r.expect_domain(RULE_ADOPTION_DOMAIN, "domain")?;
        r.expect_version(AUTHZ_V0_VERSION)?;
        let context = ContextRef(r.b32("context")?);
        let epoch = r.u64("epoch")?;
        let rule_hash = r.b32("rule_hash")?;
        r.finish()?;
        Ok(RuleAdoptionV0 {
            context,
            epoch,
            rule_hash,
        })
    }
}

/// The outcome of evaluating an adopted rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// The rule is satisfied.
    Pass,
    /// The rule is not satisfied.
    Fail,
}

impl Verdict {
    fn tag(self) -> u8 {
        match self {
            Verdict::Pass => 1,
            Verdict::Fail => 2,
        }
    }

    fn from_tag(tag: u8) -> Result<Self, AuthzCodecError> {
        match tag {
            1 => Ok(Verdict::Pass),
            2 => Ok(Verdict::Fail),
            _ => Err(AuthzCodecError::InvalidValue { field: "verdict" }),
        }
    }
}

/// What the authorization permits, and where.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectV0 {
    /// The execution target: a custodian's [`target_id`] (or another 32-byte target id).
    pub target: [u8; 32],
    /// An opaque action label, at most [`MAX_ACTION_LEN`] bytes.
    pub action: String,
    /// Hash of the action parameters. Bound, never interpreted here.
    pub params_hash: [u8; 32],
}

impl EffectV0 {
    /// The subject matter voters must vote on for this effect to be authorized:
    /// `SHA-256(lp(SUBJECT_MATTER_DOMAIN) || context || target || lp(action) || params_hash ||
    /// operation_id)`.
    ///
    /// [`super::verify_authorization`] requires `AuthorizationV0.subject_matter` to equal this, and
    /// rules bind every act to `subject_matter`. So a vote set authorizes exactly one effect under
    /// exactly one operation id: a writer cannot reuse a passing vote set for another effect, or
    /// for the same effect again under a fresh operation id.
    pub fn commitment(&self, context: ContextRef, operation_id: [u8; 32]) -> [u8; 32] {
        let mut w = Writer::new();
        w.lp(SUBJECT_MATTER_DOMAIN);
        w.b32(&context.0);
        w.b32(&self.target);
        w.lp(self.action.as_bytes());
        w.b32(&self.params_hash);
        w.b32(&operation_id);
        sha256(&w.finish())
    }
}

/// The commitment to a context's full standing roster for one authorization:
/// `SHA-256(lp(ROSTER_DOMAIN) || count || (StandingV0 hash)*)`, ordered by Subject.
///
/// The eligible set (the quorum denominator) is derived from the roster, so the authorization
/// commits to it: nobody can drop or add a standing statement after the writer signs. It does
/// **not** prove the roster is complete — a writer can sign a roster that omits a member; the
/// omitted member's own standing statement, attached to the evidence, is then refused as a roster
/// mismatch. Completeness needs a sequenced membership log (not in v0).
pub fn roster_hash(standing: &[StandingV0]) -> [u8; 32] {
    let mut sorted: Vec<&StandingV0> = standing.iter().collect();
    sorted.sort_by_key(|s| s.subject);
    let mut w = Writer::new();
    w.lp(ROSTER_DOMAIN);
    w.count(sorted.len());
    for s in sorted {
        w.b32(&s.hash());
    }
    sha256(&w.finish())
}

/// One vote the writer counted: which act, for which Subject, evaluated at which N1 position.
///
/// The position is the **relying-party position** handed to the existing N4 verifier. It comes
/// from the writer's signed statement, not from the voter's own bytes. Its **freshness is
/// writer-attested, not verified**: a bundle carries facts only up to the position, so a revocation
/// after it is invisible to the verifier. A writer that names a stale position can count a vote
/// from a device revoked since. Verified freshness needs the Subject's current head (not in v0).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputActV0 {
    /// `DeviceActV1::act_id` of the vote.
    pub act_id: [u8; 32],
    /// The Subject the act must name.
    pub subject: SubjectId,
    /// The N1 position the act must be evaluated at.
    pub position: u64,
}

/// The writer's signed claim that `rule_hash` evaluated to `verdict` over exactly `inputs`, and the
/// effect that the verdict authorizes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationV0 {
    /// The context.
    pub context: ContextRef,
    /// The writer epoch.
    pub epoch: u64,
    /// The matter decided (e.g. a proposal id hash). Rules bind acts to it.
    pub subject_matter: [u8; 32],
    /// The adopted rule.
    pub rule_hash: [u8; 32],
    /// The writer's verdict. **Re-derived by every verifier**, never trusted.
    pub verdict: Verdict,
    /// Counted inputs, strictly ascending by `act_id`.
    pub inputs: Vec<InputActV0>,
    /// [`roster_hash`] of the standing statements the eligible set is derived from.
    pub roster_hash: [u8; 32],
    /// The effect authorized. `subject_matter` must equal [`EffectV0::commitment`].
    pub effect: EffectV0,
    /// A writer-chosen id for this authorization's one admission.
    pub operation_id: [u8; 32],
    /// Caller-clock instant after which the authorization may not be admitted.
    pub not_after: u64,
}

impl AuthorizationV0 {
    /// Whether `inputs` is strictly ascending by `act_id`.
    pub fn inputs_canonical(&self) -> bool {
        self.inputs.windows(2).all(|w| w[0].act_id < w[1].act_id)
    }
}

impl CanonicalV0 for AuthorizationV0 {
    fn canonical_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.lp(AUTHORIZATION_DOMAIN);
        w.u16(AUTHZ_V0_VERSION);
        w.b32(&self.context.0);
        w.u64(self.epoch);
        w.b32(&self.subject_matter);
        w.b32(&self.rule_hash);
        w.u8(self.verdict.tag());
        w.count(self.inputs.len());
        for i in &self.inputs {
            w.b32(&i.act_id);
            w.b32(i.subject.as_bytes());
            w.u64(i.position);
        }
        w.b32(&self.roster_hash);
        w.b32(&self.effect.target);
        w.lp(self.effect.action.as_bytes());
        w.b32(&self.effect.params_hash);
        w.b32(&self.operation_id);
        w.u64(self.not_after);
        w.finish()
    }

    fn decode(bytes: &[u8]) -> Result<Self, AuthzCodecError> {
        let mut r = Reader::new(bytes);
        r.expect_domain(AUTHORIZATION_DOMAIN, "domain")?;
        r.expect_version(AUTHZ_V0_VERSION)?;
        let context = ContextRef(r.b32("context")?);
        let epoch = r.u64("epoch")?;
        let subject_matter = r.b32("subject_matter")?;
        let rule_hash = r.b32("rule_hash")?;
        let verdict = Verdict::from_tag(r.u8("verdict")?)?;
        let n = r.count("inputs", MAX_INPUTS)?;
        let mut inputs = Vec::with_capacity(n);
        for _ in 0..n {
            inputs.push(InputActV0 {
                act_id: r.b32("input_act_id")?,
                subject: SubjectId::from_bytes(r.b32("input_subject")?),
                position: r.u64("input_position")?,
            });
        }
        let roster_hash = r.b32("roster_hash")?;
        let target = r.b32("target")?;
        let action = r.text("action", MAX_ACTION_LEN)?;
        let params_hash = r.b32("params_hash")?;
        let operation_id = r.b32("operation_id")?;
        let not_after = r.u64("not_after")?;
        r.finish()?;
        let auth = AuthorizationV0 {
            context,
            epoch,
            subject_matter,
            rule_hash,
            verdict,
            inputs,
            roster_hash,
            effect: EffectV0 {
                target,
                action,
                params_hash,
            },
            operation_id,
            not_after,
        };
        if !auth.inputs_canonical() {
            return Err(AuthzCodecError::NotStrictlyAscending { field: "inputs" });
        }
        Ok(auth)
    }
}

/// A custodian's signed statement that it executed (or attempted) the admitted operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutcomeReceiptV0 {
    /// The admitted operation.
    pub operation_id: [u8; 32],
    /// `SHA-256(canonical AuthorizationV0)` of the admitted authorization.
    pub authorization_hash: [u8; 32],
    /// The custodian that signs.
    pub custodian: PrincipalKey,
    /// Hash of the result. Bound, not interpreted. (v0 runs no real effect.)
    pub result_hash: [u8; 32],
}

impl CanonicalV0 for OutcomeReceiptV0 {
    fn canonical_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.lp(OUTCOME_DOMAIN);
        w.u16(AUTHZ_V0_VERSION);
        w.b32(&self.operation_id);
        w.b32(&self.authorization_hash);
        write_principal(&mut w, &self.custodian);
        w.b32(&self.result_hash);
        w.finish()
    }

    fn decode(bytes: &[u8]) -> Result<Self, AuthzCodecError> {
        let mut r = Reader::new(bytes);
        r.expect_domain(OUTCOME_DOMAIN, "domain")?;
        r.expect_version(AUTHZ_V0_VERSION)?;
        let operation_id = r.b32("operation_id")?;
        let authorization_hash = r.b32("authorization_hash")?;
        let custodian = read_principal(&mut r, "custodian")?;
        let result_hash = r.b32("result_hash")?;
        r.finish()?;
        Ok(OutcomeReceiptV0 {
            operation_id,
            authorization_hash,
            custodian,
            result_hash,
        })
    }
}

/// A body with one Ed25519 signature over its canonical bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Signed<T> {
    /// The signed body.
    pub body: T,
    /// Ed25519 signature over `body.canonical_bytes()`.
    pub signature: [u8; 64],
}

impl<T: CanonicalV0> Signed<T> {
    /// Sign `body` with `key`.
    pub fn sign(body: T, key: &SigningKey) -> Self {
        let signature = key.sign(&body.canonical_bytes()).to_bytes();
        Signed { body, signature }
    }

    /// Whether the signature verifies (strictly) under `key`.
    pub fn verifies_under(&self, key: &PrincipalKey) -> bool {
        key.verifying_key()
            .verify_strict(
                &self.body.canonical_bytes(),
                &Signature::from_bytes(&self.signature),
            )
            .is_ok()
    }

    /// Canonical bytes: `lp(body) || signature`.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.lp(&self.body.canonical_bytes());
        w.b64(&self.signature);
        w.finish()
    }

    /// Strict decode.
    pub fn decode(bytes: &[u8]) -> Result<Self, AuthzCodecError> {
        let mut r = Reader::new(bytes);
        let body = T::decode(r.lp("body", MAX_STATEMENT)?)?;
        let signature = r.b64("signature")?;
        r.finish()?;
        Ok(Signed { body, signature })
    }
}

/// Everything a target needs to verify an authorization offline: bytes in, rule artifact in, no
/// node, store or secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationEvidenceV0 {
    /// The context's founding statement.
    pub inception: ContextInceptionV0,
    /// The designation of the writer for the authorization's epoch.
    pub designation: SignedDesignation,
    /// Standing statements, strictly ascending by Subject in canonical bytes.
    pub standing: Vec<Signed<StandingV0>>,
    /// The rule adoption.
    pub rule_adoption: Signed<RuleAdoptionV0>,
    /// The rule artifact; `SHA-256(rule_artifact)` must equal the adopted `rule_hash`.
    pub rule_artifact: Vec<u8>,
    /// N4 evidence bundles, one per vote, strictly ascending by act id in canonical bytes.
    pub vote_bundles: Vec<EvidenceBundle>,
    /// The writer-signed authorization.
    pub authorization: Signed<AuthorizationV0>,
}

impl AuthorizationEvidenceV0 {
    /// Put `standing` and `vote_bundles` in canonical order (by Subject; by act id).
    pub fn canonicalize(&mut self) {
        self.standing.sort_by_key(|s| s.body.subject);
        self.vote_bundles.sort_by_key(|b| b.act().act.act_id());
    }

    /// Canonical bytes. Encode a value after [`Self::canonicalize`]; bytes for an unsorted value
    /// are refused by [`Self::decode`].
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.lp(EVIDENCE_DOMAIN);
        w.u16(AUTHZ_V0_VERSION);
        w.lp(&self.inception.canonical_bytes());
        w.lp(&self.designation.encode());
        w.count(self.standing.len());
        for s in &self.standing {
            w.lp(&s.encode());
        }
        w.lp(&self.rule_adoption.encode());
        w.lp(&self.rule_artifact);
        w.count(self.vote_bundles.len());
        for b in &self.vote_bundles {
            w.lp(&b.canonical_bytes());
        }
        w.lp(&self.authorization.encode());
        w.finish()
    }

    /// Strict decode. Refuses unsorted or duplicated standing (by Subject) and vote bundles (by
    /// act id), and any vote bundle the N4 decoder refuses.
    pub fn decode(bytes: &[u8]) -> Result<Self, AuthzCodecError> {
        let mut r = Reader::new(bytes);
        r.expect_domain(EVIDENCE_DOMAIN, "domain")?;
        r.expect_version(AUTHZ_V0_VERSION)?;
        let inception = ContextInceptionV0::decode(r.lp("inception", MAX_STATEMENT)?)?;
        let designation = SignedDesignation::decode(r.lp("designation", MAX_STATEMENT)?)?;
        let n = r.count("standing", MAX_INPUTS)?;
        let mut standing: Vec<Signed<StandingV0>> = Vec::with_capacity(n);
        for _ in 0..n {
            standing.push(Signed::decode(r.lp("standing", MAX_STATEMENT)?)?);
        }
        let rule_adoption = Signed::decode(r.lp("rule_adoption", MAX_STATEMENT)?)?;
        let rule_artifact = r.lp("rule_artifact", MAX_RULE_ARTIFACT)?.to_vec();
        let n = r.count("vote_bundles", MAX_INPUTS)?;
        let mut vote_bundles = Vec::with_capacity(n);
        for index in 0..n {
            let raw = r.lp("vote_bundle", MAX_VOTE_BUNDLE)?;
            let bundle = EvidenceBundle::decode(raw).map_err(|e| AuthzCodecError::VoteBundle {
                index,
                reason: e.to_string(),
            })?;
            vote_bundles.push(bundle);
        }
        let authorization = Signed::decode(r.lp("authorization", MAX_STATEMENT)?)?;
        r.finish()?;
        if !standing
            .windows(2)
            .all(|w| w[0].body.subject < w[1].body.subject)
        {
            return Err(AuthzCodecError::NotStrictlyAscending { field: "standing" });
        }
        if !vote_bundles
            .windows(2)
            .all(|w| w[0].act().act.act_id() < w[1].act().act.act_id())
        {
            return Err(AuthzCodecError::NotStrictlyAscending {
                field: "vote_bundles",
            });
        }
        Ok(AuthorizationEvidenceV0 {
            inception,
            designation,
            standing,
            rule_adoption,
            rule_artifact,
            vote_bundles,
            authorization,
        })
    }
}
