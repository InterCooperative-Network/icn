//! # Context Authority Log v0 (EXPERIMENTAL)
//!
//! A position-numbered, hash-chained log of a context's institutional transitions, from which any
//! holder derives the context's standing, rules in force and writer **without trusting the
//! writer's account of them**.
//!
//! ## What the writer can and cannot do
//!
//! The writer *sequences* transitions: it signs each entry, which fixes order and makes it
//! accountable for that order. It does not *authorize* them. Every entry after genesis is
//! accepted only if a decision under the **transition rule in force at the previous position**,
//! evaluated over the **electorate derived from the previous position**, passes. So the writer
//! cannot grant standing, revoke it, replace a rule, or replace itself on its own word, and a new
//! rule can never approve its own adoption (it is not in force until the entry that adopts it is
//! accepted).
//!
//! ## Bootstrap, operation, succession (the v0 experimental profile)
//!
//! - **Bootstrap.** Entry 1 is a [`TransitionV0::Genesis`] signed by a threshold of the context's
//!   founders (the existing [`ContextInceptionV0`] quorum). It fixes the initial standing, the two
//!   rules and the first writer. Founder authority ends there: afterwards founders act only
//!   through standing they hold. A context that wants founders to keep a veto expresses that in
//!   its transition rule; v0 does not choose it for them.
//! - **Operation.** Standing, rules and writer change only through authorized transitions.
//! - **Succession.** A [`TransitionV0::AppointWriter`] entry may be sequenced by the outgoing
//!   *or the incoming* writer, so an absent or hostile writer cannot block its own replacement.
//!   Anyone holding the log bytes can verify and continue it; no provider secret is involved.
//!
//! Intersecting founder quorums and the single sequencing writer are properties of this
//! experimental profile, not constitutional law. No rule, threshold or role is chosen here: rules
//! are [`AdoptedRule`] implementations supplied by domain crates through a [`RuleResolver`].
//!
//! ## What the log does NOT prove
//!
//! - **Completeness / no hidden alternative.** A hash chain proves only that a presented prefix is
//!   internally consistent. A writer can sign a *different* history from any position. A relying
//!   party detects that only if it holds an independently retained checkpoint
//!   ([`RelyingFrontierV0::pinned`]) or sees both histories ([`find_log_fork`]). Detection is not
//!   prevention: a fork proof attributes the conflict; it does not undo effects admitted meanwhile.
//! - **Freshness.** "Current" means "at or after the relying party's frontier". The frontier is an
//!   input (the party's own retained head, a witness, a peer); v0 discovers nothing and sets no
//!   universal maximum age.
//! - **Censorship.** A transition the writer never sequences is invisible here. Detecting it
//!   needs external evidence of submission (a writer-signed receipt, a witness), which v0 lacks.
//! - **Vote freshness.** Votes are still evaluated at the N1 position the writer lists (known gap
//!   D in #2884): a device revoked after that position can still vote.
//!
//! Formats are v0 experimental (`icn.authz.v0.*`), not persisted or gossiped by any daemon, and
//! droppable without migration.

use std::collections::BTreeSet;

use ed25519_dalek::{Signature, Signer, SigningKey};
use icn_identity::authority_log::{PrincipalKey, SubjectId};
use icn_identity::evidence_bundle::EvidenceBundle;
use thiserror::Error;

use super::codec::{AuthzCodecError, Reader, Writer};
use super::statements::{
    read_principal, sha256, write_principal, CanonicalV0, ContextInceptionV0, ContextRef, EffectV0,
    FounderSignature, InputActV0, Signed, Verdict, AUTHZ_V0_VERSION, MAX_ACTION_LEN, MAX_FOUNDERS,
    MAX_INPUTS, MAX_ROLE_LEN, MAX_STATEMENT, MAX_VOTE_BUNDLE,
};
use super::verify::{verify_input_acts, AdoptedRule, AuthzRefusal, RuleBinding, RuleError};

/// Domain of [`LogEntryV0`] bytes (signed by founders or the writer; hashed into the chain).
pub const LOG_ENTRY_DOMAIN: &[u8] = b"icn.authz.v0.log-entry";
/// Domain of [`transition_subject_matter`]: what voters vote on to authorize one entry.
pub const LOG_TRANSITION_DOMAIN: &[u8] = b"icn.authz.v0.log-transition";
/// Domain of [`CheckpointedAuthorizationV0`] bytes.
pub const CHECKPOINTED_AUTHORIZATION_DOMAIN: &[u8] = b"icn.authz.v0.checkpointed-authorization";

/// Maximum standing pairs in a genesis entry.
pub const MAX_GENESIS_STANDING: usize = MAX_INPUTS;

// ---------------------------------------------------------------------------------------------
// Entries
// ---------------------------------------------------------------------------------------------

/// Which of the context's two rules an adoption replaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleSlot {
    /// Authorizes log transitions, including the replacement of itself.
    Transition,
    /// Authorizes effects ([`CheckpointedAuthorizationV0`]).
    Effect,
}

impl RuleSlot {
    fn tag(self) -> u8 {
        match self {
            RuleSlot::Transition => 1,
            RuleSlot::Effect => 2,
        }
    }

    fn from_tag(tag: u8) -> Result<Self, AuthzCodecError> {
        match tag {
            1 => Ok(RuleSlot::Transition),
            2 => Ok(RuleSlot::Effect),
            _ => Err(AuthzCodecError::InvalidValue { field: "rule_slot" }),
        }
    }
}

/// One institutional transition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionV0 {
    /// Entry 1 only, founder-signed: the initial state.
    Genesis {
        /// The first writer.
        writer: PrincipalKey,
        /// `SHA-256` of the initial transition rule's artifact.
        transition_rule: [u8; 32],
        /// `SHA-256` of the initial effect rule's artifact.
        effect_rule: [u8; 32],
        /// Initial `(subject, role)` standing, strictly ascending.
        standing: Vec<(SubjectId, String)>,
    },
    /// `subject` gains `role`. Refused if already held.
    GrantStanding {
        /// The Subject.
        subject: SubjectId,
        /// The role label (opaque).
        role: String,
    },
    /// `subject` loses `role`. Refused if not held.
    RevokeStanding {
        /// The Subject.
        subject: SubjectId,
        /// The role label (opaque).
        role: String,
    },
    /// The rule in `slot` becomes `rule_hash`. Refused if already in force.
    AdoptRule {
        /// The slot replaced.
        slot: RuleSlot,
        /// `SHA-256` of the new rule's artifact.
        rule_hash: [u8; 32],
    },
    /// `writer` sequences every later entry. Refused if already the writer.
    AppointWriter {
        /// The incoming writer.
        writer: PrincipalKey,
    },
}

/// A log entry body. `seq` starts at 1; entry 1's `prev` is the context ref, every later entry's
/// `prev` is the previous entry's [`LogEntryV0::head`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogEntryV0 {
    /// The context.
    pub context: ContextRef,
    /// Position, `>= 1`.
    pub seq: u64,
    /// The head this entry extends.
    pub prev: [u8; 32],
    /// The transition.
    pub transition: TransitionV0,
}

impl LogEntryV0 {
    /// The log head after this entry: `SHA-256(canonical bytes)`.
    pub fn head(&self) -> [u8; 32] {
        self.hash()
    }
}

impl CanonicalV0 for LogEntryV0 {
    fn canonical_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.lp(LOG_ENTRY_DOMAIN);
        w.u16(AUTHZ_V0_VERSION);
        w.b32(&self.context.0);
        w.u64(self.seq);
        w.b32(&self.prev);
        match &self.transition {
            TransitionV0::Genesis {
                writer,
                transition_rule,
                effect_rule,
                standing,
            } => {
                w.u8(0);
                write_principal(&mut w, writer);
                w.b32(transition_rule);
                w.b32(effect_rule);
                w.count(standing.len());
                for (subject, role) in standing {
                    w.b32(subject.as_bytes());
                    w.lp(role.as_bytes());
                }
            }
            TransitionV0::GrantStanding { subject, role } => {
                w.u8(1);
                w.b32(subject.as_bytes());
                w.lp(role.as_bytes());
            }
            TransitionV0::RevokeStanding { subject, role } => {
                w.u8(2);
                w.b32(subject.as_bytes());
                w.lp(role.as_bytes());
            }
            TransitionV0::AdoptRule { slot, rule_hash } => {
                w.u8(3);
                w.u8(slot.tag());
                w.b32(rule_hash);
            }
            TransitionV0::AppointWriter { writer } => {
                w.u8(4);
                write_principal(&mut w, writer);
            }
        }
        w.finish()
    }

    fn decode(bytes: &[u8]) -> Result<Self, AuthzCodecError> {
        let mut r = Reader::new(bytes);
        r.expect_domain(LOG_ENTRY_DOMAIN, "domain")?;
        r.expect_version(AUTHZ_V0_VERSION)?;
        let context = ContextRef(r.b32("context")?);
        let seq = r.u64("seq")?;
        if seq == 0 {
            return Err(AuthzCodecError::InvalidValue { field: "seq" });
        }
        let prev = r.b32("prev")?;
        let transition = match r.u8("transition")? {
            0 => {
                let writer = read_principal(&mut r, "writer")?;
                let transition_rule = r.b32("transition_rule")?;
                let effect_rule = r.b32("effect_rule")?;
                let n = r.count("standing", MAX_GENESIS_STANDING)?;
                let mut standing = Vec::with_capacity(n);
                for _ in 0..n {
                    let subject = SubjectId::from_bytes(r.b32("subject")?);
                    standing.push((subject, r.text("role", MAX_ROLE_LEN)?));
                }
                if !standing.windows(2).all(|p| p[0] < p[1]) {
                    return Err(AuthzCodecError::NotStrictlyAscending { field: "standing" });
                }
                TransitionV0::Genesis {
                    writer,
                    transition_rule,
                    effect_rule,
                    standing,
                }
            }
            1 => TransitionV0::GrantStanding {
                subject: SubjectId::from_bytes(r.b32("subject")?),
                role: r.text("role", MAX_ROLE_LEN)?,
            },
            2 => TransitionV0::RevokeStanding {
                subject: SubjectId::from_bytes(r.b32("subject")?),
                role: r.text("role", MAX_ROLE_LEN)?,
            },
            3 => TransitionV0::AdoptRule {
                slot: RuleSlot::from_tag(r.u8("rule_slot")?)?,
                rule_hash: r.b32("rule_hash")?,
            },
            4 => TransitionV0::AppointWriter {
                writer: read_principal(&mut r, "writer")?,
            },
            _ => {
                return Err(AuthzCodecError::InvalidValue {
                    field: "transition",
                })
            }
        };
        r.finish()?;
        Ok(LogEntryV0 {
            context,
            seq,
            prev,
            transition,
        })
    }
}

/// What voters vote on to authorize `entry`: `SHA-256(lp(LOG_TRANSITION_DOMAIN) || entry head)`.
/// It binds context, position, the head extended and the exact transition, so a decision cannot
/// be replayed at another position, onto another history, or for another transition.
pub fn transition_subject_matter(entry: &LogEntryV0) -> [u8; 32] {
    let mut w = Writer::new();
    w.lp(LOG_TRANSITION_DOMAIN);
    w.b32(&entry.head());
    sha256(&w.finish())
}

/// Why an entry is accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryAuthorityV0 {
    /// Genesis only: founder signatures over the entry, strictly ascending by founder.
    Founders(Vec<FounderSignature>),
    /// Every later entry: the sequencing writer's signature and the decision that authorizes it.
    Decision(Box<DecisionAuthorityV0>),
}

/// The authority of every entry after genesis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecisionAuthorityV0 {
    /// The sequencing writer (the writer in force; for `AppointWriter`, also the incoming one).
    pub signer: PrincipalKey,
    /// Ed25519 signature by `signer` over the entry's canonical bytes.
    pub signature: [u8; 64],
    /// The counted acts, strictly ascending by `act_id`.
    pub inputs: Vec<InputActV0>,
    /// One N4 evidence bundle per counted act.
    pub vote_bundles: Vec<EvidenceBundle>,
}

/// An entry with the authority that makes it acceptable. Transferable as bytes
/// ([`SignedLogEntryV0::encode`]); verifying it needs no provider, node or secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedLogEntryV0 {
    /// The entry.
    pub body: LogEntryV0,
    /// Its authority.
    pub authority: EntryAuthorityV0,
}

impl SignedLogEntryV0 {
    /// A genesis entry signed by `founders`.
    pub fn genesis(body: LogEntryV0, founders: &[&SigningKey]) -> Result<Self, AuthzCodecError> {
        let bytes = body.canonical_bytes();
        let mut signatures = Vec::with_capacity(founders.len());
        for key in founders {
            let founder = PrincipalKey::try_from_verifying_key(key.verifying_key())
                .map_err(|_| AuthzCodecError::InvalidPrincipal { field: "founder" })?;
            signatures.push(FounderSignature {
                founder,
                signature: key.sign(&bytes).to_bytes(),
            });
        }
        signatures.sort_by_key(|s| s.founder.as_bytes());
        Ok(SignedLogEntryV0 {
            body,
            authority: EntryAuthorityV0::Founders(signatures),
        })
    }

    /// An entry sequenced by `writer` and authorized by the acts in `vote_bundles`.
    pub fn decided(
        body: LogEntryV0,
        writer: &SigningKey,
        mut vote_bundles: Vec<EvidenceBundle>,
    ) -> Result<Self, AuthzCodecError> {
        let signer = PrincipalKey::try_from_verifying_key(writer.verifying_key())
            .map_err(|_| AuthzCodecError::InvalidPrincipal { field: "signer" })?;
        let signature = writer.sign(&body.canonical_bytes()).to_bytes();
        vote_bundles.sort_by_key(|b| b.act().act.act_id());
        let inputs = vote_bundles
            .iter()
            .map(|b| InputActV0 {
                act_id: b.act().act.act_id(),
                subject: b.subject(),
                position: b.evaluation_position(),
            })
            .collect();
        Ok(SignedLogEntryV0 {
            body,
            authority: EntryAuthorityV0::Decision(Box::new(DecisionAuthorityV0 {
                signer,
                signature,
                inputs,
                vote_bundles,
            })),
        })
    }

    /// Canonical transport bytes: `lp(body) || tag || authority`.
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.lp(&self.body.canonical_bytes());
        match &self.authority {
            EntryAuthorityV0::Founders(signatures) => {
                w.u8(1);
                w.count(signatures.len());
                for s in signatures {
                    write_principal(&mut w, &s.founder);
                    w.b64(&s.signature);
                }
            }
            EntryAuthorityV0::Decision(d) => {
                let DecisionAuthorityV0 {
                    signer,
                    signature,
                    inputs,
                    vote_bundles,
                } = d.as_ref();
                w.u8(2);
                write_principal(&mut w, signer);
                w.b64(signature);
                w.count(inputs.len());
                for i in inputs {
                    w.b32(&i.act_id);
                    w.b32(i.subject.as_bytes());
                    w.u64(i.position);
                }
                w.count(vote_bundles.len());
                for b in vote_bundles {
                    w.lp(&b.canonical_bytes());
                }
            }
        }
        w.finish()
    }

    /// Strict decode. Ordering is checked by replay, not here, so a reordered entry decodes and is
    /// then refused with a precise reason.
    pub fn decode(bytes: &[u8]) -> Result<Self, AuthzCodecError> {
        let mut r = Reader::new(bytes);
        let body = LogEntryV0::decode(r.lp("entry", MAX_STATEMENT)?)?;
        let authority = match r.u8("authority")? {
            1 => {
                let n = r.count("founder_signatures", MAX_FOUNDERS)?;
                let mut signatures = Vec::with_capacity(n);
                for _ in 0..n {
                    let founder = read_principal(&mut r, "founder")?;
                    let signature = r.b64("founder_signature")?;
                    signatures.push(FounderSignature { founder, signature });
                }
                EntryAuthorityV0::Founders(signatures)
            }
            2 => {
                let signer = read_principal(&mut r, "signer")?;
                let signature = r.b64("signature")?;
                let n = r.count("inputs", MAX_INPUTS)?;
                let mut inputs = Vec::with_capacity(n);
                for _ in 0..n {
                    inputs.push(InputActV0 {
                        act_id: r.b32("input_act_id")?,
                        subject: SubjectId::from_bytes(r.b32("input_subject")?),
                        position: r.u64("input_position")?,
                    });
                }
                let n = r.count("vote_bundles", MAX_INPUTS)?;
                let mut vote_bundles = Vec::with_capacity(n);
                for index in 0..n {
                    let raw = r.lp("vote_bundle", MAX_VOTE_BUNDLE)?;
                    vote_bundles.push(EvidenceBundle::decode(raw).map_err(|e| {
                        AuthzCodecError::VoteBundle {
                            index,
                            reason: e.to_string(),
                        }
                    })?);
                }
                EntryAuthorityV0::Decision(Box::new(DecisionAuthorityV0 {
                    signer,
                    signature,
                    inputs,
                    vote_bundles,
                }))
            }
            _ => return Err(AuthzCodecError::InvalidValue { field: "authority" }),
        };
        r.finish()?;
        Ok(SignedLogEntryV0 { body, authority })
    }
}

// ---------------------------------------------------------------------------------------------
// Derived state
// ---------------------------------------------------------------------------------------------

/// A position in a context's log: `(seq, head)`. Two checkpoints with one `seq` and different
/// heads are two histories.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogCheckpointV0 {
    /// Position.
    pub seq: u64,
    /// Head after the entry at `seq`.
    pub head: [u8; 32],
}

/// The institutional state derived from a log prefix. Nothing in it is taken from the writer's
/// word: each field is the result of accepted transitions only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContextStateV0 {
    /// The context.
    pub context: ContextRef,
    /// The position this state is at.
    pub checkpoint: LogCheckpointV0,
    /// The writer that sequences the next entry.
    pub writer: PrincipalKey,
    /// Rule authorizing transitions.
    pub transition_rule: [u8; 32],
    /// Rule authorizing effects.
    pub effect_rule: [u8; 32],
    /// `(subject, role)` pairs in force.
    pub standing: BTreeSet<(SubjectId, String)>,
}

impl ContextStateV0 {
    /// Subjects holding `role` at this position.
    pub fn eligible(&self, role: &str) -> BTreeSet<SubjectId> {
        self.standing
            .iter()
            .filter(|(_, r)| r == role)
            .map(|(s, _)| *s)
            .collect()
    }
}

/// Implementations of adopted rules, by artifact hash. A verifier that cannot resolve a rule
/// in force fails closed.
pub trait RuleResolver {
    /// The implementation whose [`AdoptedRule::rule_hash`] is `rule_hash`, if this verifier has it.
    fn resolve(&self, rule_hash: &[u8; 32]) -> Option<&dyn AdoptedRule>;
}

/// Why a log, entry, checkpoint or checkpointed authorization was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LogRefusal {
    /// The inception does not validate.
    #[error("invalid inception: {0}")]
    InvalidInception(AuthzCodecError),
    /// The inception is not the context the relying party trusts.
    #[error("not the trusted context")]
    UntrustedContext,
    /// No entries.
    #[error("empty log")]
    Empty,
    /// An entry names another context.
    #[error("entry {seq} names another context")]
    ContextMismatch {
        /// Position.
        seq: u64,
    },
    /// Positions are not `1, 2, 3, ...`.
    #[error("expected position {expected}, found {found}")]
    SequenceGap {
        /// Expected.
        expected: u64,
        /// Found.
        found: u64,
    },
    /// `prev` is not the previous head (or the context ref, for entry 1).
    #[error("entry {seq} does not extend the previous head")]
    BrokenChain {
        /// Position.
        seq: u64,
    },
    /// Entry 1 is not a genesis, or a genesis appears later.
    #[error("entry {seq}: genesis must be exactly entry 1")]
    GenesisPosition {
        /// Position.
        seq: u64,
    },
    /// The authority kind does not match the entry (founders after genesis, votes at genesis).
    #[error("entry {seq}: wrong kind of authority")]
    WrongAuthority {
        /// Position.
        seq: u64,
    },
    /// A malformed genesis.
    #[error("invalid genesis: {0}")]
    InvalidGenesis(&'static str),
    /// Founder signatures not strictly ascending by founder.
    #[error("founder signatures not canonical")]
    FounderSignaturesNotCanonical,
    /// A genesis signer is not a founder.
    #[error("genesis signer is not a founder")]
    NotAFounder,
    /// A founder signature does not verify.
    #[error("founder signature does not verify")]
    BadFounderSignature,
    /// Fewer founder signatures than the inception threshold.
    #[error("{valid} founder signatures, threshold {threshold}")]
    FounderQuorumNotMet {
        /// Valid signatures.
        valid: usize,
        /// Required.
        threshold: u8,
    },
    /// The entry's sequencer is not the writer in force (nor, for an appointment, the incoming
    /// writer).
    #[error("entry {seq} is sequenced by a principal that is not the writer")]
    NotTheWriter {
        /// Position.
        seq: u64,
    },
    /// The sequencer's signature does not verify.
    #[error("entry {seq}: sequencer signature does not verify")]
    BadWriterSignature {
        /// Position.
        seq: u64,
    },
    /// The transition changes nothing (grant held, revoke absent, rule or writer unchanged).
    #[error("entry {seq} is a no-op transition")]
    NoOpTransition {
        /// Position.
        seq: u64,
    },
    /// This verifier has no implementation of a rule in force.
    #[error("rule {rule_hash:02x?} is not available to this verifier")]
    RuleUnavailable {
        /// The rule.
        rule_hash: [u8; 32],
    },
    /// The resolved implementation reports another hash.
    #[error("rule implementation does not match its hash")]
    RuleImplementationMismatch,
    /// A vote failed the shared N1/N4 vote path.
    #[error("at {at}: vote refused: {refusal}")]
    Vote {
        /// Position the decision is evaluated at.
        at: u64,
        /// Why.
        refusal: Box<AuthzRefusal>,
    },
    /// A voter holds no standing for the rule's role at the evaluation position.
    #[error("at {at}: voter holds no standing for the rule's role")]
    IneligibleVoter {
        /// Position.
        at: u64,
        /// The Subject.
        subject: SubjectId,
    },
    /// The rule refused to evaluate.
    #[error("at {at}: rule refused: {error}")]
    RuleRefused {
        /// Position.
        at: u64,
        /// Why.
        error: RuleError,
    },
    /// The transition rule in force did not pass the entry.
    #[error("entry {seq} is not authorized by the transition rule in force")]
    TransitionNotAuthorized {
        /// Position.
        seq: u64,
    },
    /// The relying party's pinned checkpoint is beyond the presented log.
    #[error("pinned checkpoint {seq} is not in the presented log")]
    PinnedCheckpointMissing {
        /// Pinned position.
        seq: u64,
    },
    /// The presented log has another head at the pinned position: another history.
    #[error("presented log disagrees with the pinned checkpoint at {seq}")]
    PinnedCheckpointMismatch {
        /// Pinned position.
        seq: u64,
    },
    /// The authorization's checkpoint is not a position of the presented log.
    #[error("authorization checkpoint is not in the presented log")]
    CheckpointNotInLog,
    /// The authorization relies on a position below the relying party's frontier.
    #[error("checkpoint {checkpoint} is below the frontier {frontier}")]
    CheckpointBelowFrontier {
        /// The authorization's position.
        checkpoint: u64,
        /// The relying party's floor.
        frontier: u64,
    },
    /// The authorization names another context.
    #[error("authorization names another context")]
    AuthorizationContextMismatch,
    /// The authorization is not signed by the writer in force at its checkpoint.
    #[error("authorization is not signed by the writer at its checkpoint")]
    AuthorizationNotByWriter,
    /// The authorization cites a rule that is not the effect rule at its checkpoint.
    #[error("authorization rule is not the effect rule in force")]
    EffectRuleNotInForce,
    /// `subject_matter` is not the effect's commitment.
    #[error("subject matter is not bound to the effect")]
    SubjectMatterNotBoundToEffect,
    /// Re-evaluation did not reproduce the signed verdict.
    #[error("signed verdict {signed:?} but the rule evaluates to {evaluated:?}")]
    VerdictMismatch {
        /// Signed.
        signed: Verdict,
        /// Evaluated.
        evaluated: Verdict,
    },
}

fn verify_founder_quorum(
    inception: &ContextInceptionV0,
    bytes: &[u8],
    signatures: &[FounderSignature],
) -> Result<(), LogRefusal> {
    if !signatures
        .windows(2)
        .all(|w| w[0].founder.as_bytes() < w[1].founder.as_bytes())
    {
        return Err(LogRefusal::FounderSignaturesNotCanonical);
    }
    for s in signatures {
        if !inception.is_founder(&s.founder) {
            return Err(LogRefusal::NotAFounder);
        }
        s.founder
            .verifying_key()
            .verify_strict(bytes, &Signature::from_bytes(&s.signature))
            .map_err(|_| LogRefusal::BadFounderSignature)?;
    }
    if signatures.len() < usize::from(inception.threshold) {
        return Err(LogRefusal::FounderQuorumNotMet {
            valid: signatures.len(),
            threshold: inception.threshold,
        });
    }
    Ok(())
}

/// Evaluate a decision under `rule_hash` over the electorate of `state`.
fn decide(
    state: &ContextStateV0,
    rule_hash: [u8; 32],
    subject_matter: [u8; 32],
    inputs: &[InputActV0],
    bundles: &[EvidenceBundle],
    rules: &dyn RuleResolver,
) -> Result<Verdict, LogRefusal> {
    let at = state.checkpoint.seq;
    let rule = rules
        .resolve(&rule_hash)
        .ok_or(LogRefusal::RuleUnavailable { rule_hash })?;
    if rule.rule_hash() != rule_hash {
        return Err(LogRefusal::RuleImplementationMismatch);
    }
    let acts = verify_input_acts(inputs, bundles).map_err(|refusal| LogRefusal::Vote {
        at,
        refusal: Box::new(refusal),
    })?;
    let eligible = state.eligible(rule.required_role());
    for act in &acts {
        let subject = act.act().subject;
        if !eligible.contains(&subject) {
            return Err(LogRefusal::IneligibleVoter { at, subject });
        }
    }
    rule.evaluate(
        &RuleBinding {
            context: state.context,
            subject_matter,
        },
        &eligible,
        &acts,
    )
    .map_err(|error| LogRefusal::RuleRefused { at, error })
}

fn genesis_state(
    inception: &ContextInceptionV0,
    entry: &SignedLogEntryV0,
) -> Result<ContextStateV0, LogRefusal> {
    let context = inception.context_ref();
    let body = &entry.body;
    if body.context != context {
        return Err(LogRefusal::ContextMismatch { seq: body.seq });
    }
    if body.seq != 1 {
        return Err(LogRefusal::SequenceGap {
            expected: 1,
            found: body.seq,
        });
    }
    if body.prev != context.0 {
        return Err(LogRefusal::BrokenChain { seq: 1 });
    }
    let TransitionV0::Genesis {
        writer,
        transition_rule,
        effect_rule,
        standing,
    } = &body.transition
    else {
        return Err(LogRefusal::GenesisPosition { seq: 1 });
    };
    let EntryAuthorityV0::Founders(signatures) = &entry.authority else {
        return Err(LogRefusal::WrongAuthority { seq: 1 });
    };
    if !standing.windows(2).all(|p| p[0] < p[1]) {
        return Err(LogRefusal::InvalidGenesis(
            "standing not strictly ascending",
        ));
    }
    if standing.iter().any(|(_, role)| role.len() > MAX_ROLE_LEN) {
        return Err(LogRefusal::InvalidGenesis("role too long"));
    }
    verify_founder_quorum(inception, &body.canonical_bytes(), signatures)?;
    Ok(ContextStateV0 {
        context,
        checkpoint: LogCheckpointV0 {
            seq: 1,
            head: body.head(),
        },
        writer: *writer,
        transition_rule: *transition_rule,
        effect_rule: *effect_rule,
        standing: standing.iter().cloned().collect(),
    })
}

/// Accept `entry` on top of `state`, or refuse it. The authority of `entry` is judged entirely
/// by `state` (the previous position): its writer, its transition rule, its electorate.
pub fn apply_entry(
    state: &ContextStateV0,
    entry: &SignedLogEntryV0,
    rules: &dyn RuleResolver,
) -> Result<ContextStateV0, LogRefusal> {
    let body = &entry.body;
    let expected = state
        .checkpoint
        .seq
        .checked_add(1)
        .ok_or(LogRefusal::SequenceGap {
            expected: u64::MAX,
            found: body.seq,
        })?;
    if body.context != state.context {
        return Err(LogRefusal::ContextMismatch { seq: body.seq });
    }
    if body.seq != expected {
        return Err(LogRefusal::SequenceGap {
            expected,
            found: body.seq,
        });
    }
    if body.prev != state.checkpoint.head {
        return Err(LogRefusal::BrokenChain { seq: body.seq });
    }
    let seq = body.seq;
    let EntryAuthorityV0::Decision(decision) = &entry.authority else {
        return Err(LogRefusal::WrongAuthority { seq });
    };
    let DecisionAuthorityV0 {
        signer,
        signature,
        inputs,
        vote_bundles,
    } = decision.as_ref();

    // The transition must change something, and its own shape decides who may sequence it.
    let mut next = state.clone();
    let mut may_sequence = vec![state.writer];
    match &body.transition {
        TransitionV0::Genesis { .. } => return Err(LogRefusal::GenesisPosition { seq }),
        TransitionV0::GrantStanding { subject, role } => {
            if role.len() > MAX_ROLE_LEN || !next.standing.insert((*subject, role.clone())) {
                return Err(LogRefusal::NoOpTransition { seq });
            }
        }
        TransitionV0::RevokeStanding { subject, role } => {
            if !next.standing.remove(&(*subject, role.clone())) {
                return Err(LogRefusal::NoOpTransition { seq });
            }
        }
        TransitionV0::AdoptRule { slot, rule_hash } => {
            let current = match slot {
                RuleSlot::Transition => &mut next.transition_rule,
                RuleSlot::Effect => &mut next.effect_rule,
            };
            if current == rule_hash {
                return Err(LogRefusal::NoOpTransition { seq });
            }
            *current = *rule_hash;
        }
        TransitionV0::AppointWriter { writer } => {
            if *writer == state.writer {
                return Err(LogRefusal::NoOpTransition { seq });
            }
            // Succession must not depend on the outgoing writer's cooperation.
            may_sequence.push(*writer);
            next.writer = *writer;
        }
    }

    if !may_sequence.contains(signer) {
        return Err(LogRefusal::NotTheWriter { seq });
    }
    signer
        .verifying_key()
        .verify_strict(&body.canonical_bytes(), &Signature::from_bytes(signature))
        .map_err(|_| LogRefusal::BadWriterSignature { seq })?;

    // Authorized under the PREVIOUS state's transition rule and electorate. A rule adopted by this
    // entry is not yet in force, so it cannot approve itself.
    let verdict = decide(
        state,
        state.transition_rule,
        transition_subject_matter(body),
        inputs,
        vote_bundles,
        rules,
    )?;
    if verdict != Verdict::Pass {
        return Err(LogRefusal::TransitionNotAuthorized { seq });
    }

    next.checkpoint = LogCheckpointV0 {
        seq,
        head: body.head(),
    };
    Ok(next)
}

/// A fully replayed log: every entry accepted from genesis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedContextLog {
    heads: Vec<[u8; 32]>,
    states: Vec<ContextStateV0>,
}

impl VerifiedContextLog {
    /// The state after the last entry.
    pub fn tip(&self) -> &ContextStateV0 {
        // Non-empty by construction: `replay_context_log` refuses an empty log.
        &self.states[self.states.len() - 1]
    }

    /// The head at `seq`, if the log reaches it.
    pub fn head_at(&self, seq: u64) -> Option<[u8; 32]> {
        let index = usize::try_from(seq.checked_sub(1)?).ok()?;
        self.heads.get(index).copied()
    }

    /// The state at `seq`, if the log reaches it.
    pub fn state_at(&self, seq: u64) -> Option<&ContextStateV0> {
        let index = usize::try_from(seq.checked_sub(1)?).ok()?;
        self.states.get(index)
    }
}

/// Replay `entries` from genesis, accepting each only under the state before it.
pub fn replay_context_log(
    inception: &ContextInceptionV0,
    entries: &[SignedLogEntryV0],
    rules: &dyn RuleResolver,
) -> Result<VerifiedContextLog, LogRefusal> {
    inception.validate().map_err(LogRefusal::InvalidInception)?;
    let (first, rest) = entries.split_first().ok_or(LogRefusal::Empty)?;
    let mut state = genesis_state(inception, first)?;
    let mut heads = Vec::with_capacity(entries.len());
    let mut states = Vec::with_capacity(entries.len());
    heads.push(state.checkpoint.head);
    states.push(state.clone());
    for entry in rest {
        state = apply_entry(&state, entry, rules)?;
        heads.push(state.checkpoint.head);
        states.push(state.clone());
    }
    Ok(VerifiedContextLog { heads, states })
}

// ---------------------------------------------------------------------------------------------
// Relying-party frontier and checkpointed authorization
// ---------------------------------------------------------------------------------------------

/// What a relying party (custodian) brings, independent of the writer.
///
/// Every field is external to the presented evidence: the trusted context root, the lowest
/// position it will act on, and optionally a checkpoint it retained itself or obtained from a
/// witness. v0 does not say where those come from or how old they may be; a consequential action
/// should carry a higher `min_seq` than a low-risk one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RelyingFrontierV0 {
    /// The context the relying party serves.
    pub context: ContextRef,
    /// No authorization relying on a position below this is accepted.
    pub min_seq: u64,
    /// A checkpoint the presented log must contain verbatim (and not rely below).
    pub pinned: Option<LogCheckpointV0>,
}

/// The writer's signed claim that, **at `checkpoint`**, the effect rule in force evaluated to
/// `verdict` over exactly `inputs`. The electorate and rule are derived from the log, never
/// from this statement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointedAuthorizationV0 {
    /// The context.
    pub context: ContextRef,
    /// The position whose state the decision is evaluated at.
    pub checkpoint: LogCheckpointV0,
    /// Must equal `effect.commitment(context, operation_id)`.
    pub subject_matter: [u8; 32],
    /// Must equal the effect rule at `checkpoint`.
    pub rule_hash: [u8; 32],
    /// Re-derived by every verifier.
    pub verdict: Verdict,
    /// Counted acts, strictly ascending by `act_id`.
    pub inputs: Vec<InputActV0>,
    /// The effect authorized.
    pub effect: EffectV0,
    /// The one admission this authorizes.
    pub operation_id: [u8; 32],
    /// Caller-clock expiry.
    pub not_after: u64,
}

impl CanonicalV0 for CheckpointedAuthorizationV0 {
    fn canonical_bytes(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.lp(CHECKPOINTED_AUTHORIZATION_DOMAIN);
        w.u16(AUTHZ_V0_VERSION);
        w.b32(&self.context.0);
        w.u64(self.checkpoint.seq);
        w.b32(&self.checkpoint.head);
        w.b32(&self.subject_matter);
        w.b32(&self.rule_hash);
        w.u8(self.verdict.tag());
        w.count(self.inputs.len());
        for i in &self.inputs {
            w.b32(&i.act_id);
            w.b32(i.subject.as_bytes());
            w.u64(i.position);
        }
        w.b32(&self.effect.target);
        w.lp(self.effect.action.as_bytes());
        w.b32(&self.effect.params_hash);
        w.b32(&self.operation_id);
        w.u64(self.not_after);
        w.finish()
    }

    fn decode(bytes: &[u8]) -> Result<Self, AuthzCodecError> {
        let mut r = Reader::new(bytes);
        r.expect_domain(CHECKPOINTED_AUTHORIZATION_DOMAIN, "domain")?;
        r.expect_version(AUTHZ_V0_VERSION)?;
        let context = ContextRef(r.b32("context")?);
        let checkpoint = LogCheckpointV0 {
            seq: r.u64("checkpoint_seq")?,
            head: r.b32("checkpoint_head")?,
        };
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
        let target = r.b32("target")?;
        let action = r.text("action", MAX_ACTION_LEN)?;
        let params_hash = r.b32("params_hash")?;
        let operation_id = r.b32("operation_id")?;
        let not_after = r.u64("not_after")?;
        r.finish()?;
        if !inputs.windows(2).all(|w| w[0].act_id < w[1].act_id) {
            return Err(AuthzCodecError::NotStrictlyAscending { field: "inputs" });
        }
        Ok(CheckpointedAuthorizationV0 {
            context,
            checkpoint,
            subject_matter,
            rule_hash,
            verdict,
            inputs,
            effect: EffectV0 {
                target,
                action,
                params_hash,
            },
            operation_id,
            not_after,
        })
    }
}

/// Everything needed to verify a checkpointed authorization offline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CheckpointedEvidenceV0 {
    /// The context's founding statement.
    pub inception: ContextInceptionV0,
    /// The log from genesis through at least the checkpoint (and the pinned position).
    pub log: Vec<SignedLogEntryV0>,
    /// The writer-signed authorization.
    pub authorization: Signed<CheckpointedAuthorizationV0>,
    /// One N4 evidence bundle per counted act.
    pub vote_bundles: Vec<EvidenceBundle>,
}

/// A checkpointed authorization every check passed. Historical validity at `checkpoint`, at or
/// above the relying party's frontier. Not admitted, not consumed, not executed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedCheckpointedAuthorization {
    /// The authorization.
    pub authorization: Signed<CheckpointedAuthorizationV0>,
    /// The writer in force at the checkpoint, who signed it.
    pub writer: PrincipalKey,
    /// The tip of the presented log (may be beyond the checkpoint).
    pub presented_tip: LogCheckpointV0,
}

/// Verify `evidence` against the relying party's `frontier`. Order, each fail-closed:
/// context root → full log replay → pinned checkpoint present and equal → authorization's
/// checkpoint in the log and at or above the frontier → writer at that checkpoint signed it →
/// rule is the effect rule there → subject matter bound to the effect → votes over the
/// electorate at that checkpoint → verdict reproduced.
pub fn verify_checkpointed_authorization(
    evidence: &CheckpointedEvidenceV0,
    frontier: &RelyingFrontierV0,
    rules: &dyn RuleResolver,
) -> Result<VerifiedCheckpointedAuthorization, LogRefusal> {
    evidence
        .inception
        .validate()
        .map_err(LogRefusal::InvalidInception)?;
    let context = evidence.inception.context_ref();
    if context != frontier.context {
        return Err(LogRefusal::UntrustedContext);
    }
    let log = replay_context_log(&evidence.inception, &evidence.log, rules)?;

    let mut floor = frontier.min_seq;
    if let Some(pinned) = frontier.pinned {
        match log.head_at(pinned.seq) {
            None => return Err(LogRefusal::PinnedCheckpointMissing { seq: pinned.seq }),
            Some(head) if head != pinned.head => {
                return Err(LogRefusal::PinnedCheckpointMismatch { seq: pinned.seq })
            }
            Some(_) => floor = floor.max(pinned.seq),
        }
    }

    let auth = &evidence.authorization.body;
    if auth.context != context {
        return Err(LogRefusal::AuthorizationContextMismatch);
    }
    if log.head_at(auth.checkpoint.seq) != Some(auth.checkpoint.head) {
        return Err(LogRefusal::CheckpointNotInLog);
    }
    if auth.checkpoint.seq < floor {
        return Err(LogRefusal::CheckpointBelowFrontier {
            checkpoint: auth.checkpoint.seq,
            frontier: floor,
        });
    }
    let state = log
        .state_at(auth.checkpoint.seq)
        .ok_or(LogRefusal::CheckpointNotInLog)?;
    if !evidence.authorization.verifies_under(&state.writer) {
        return Err(LogRefusal::AuthorizationNotByWriter);
    }
    if auth.rule_hash != state.effect_rule {
        return Err(LogRefusal::EffectRuleNotInForce);
    }
    if auth.subject_matter != auth.effect.commitment(context, auth.operation_id) {
        return Err(LogRefusal::SubjectMatterNotBoundToEffect);
    }
    let evaluated = decide(
        state,
        state.effect_rule,
        auth.subject_matter,
        &auth.inputs,
        &evidence.vote_bundles,
        rules,
    )?;
    if evaluated != auth.verdict {
        return Err(LogRefusal::VerdictMismatch {
            signed: auth.verdict,
            evaluated,
        });
    }
    Ok(VerifiedCheckpointedAuthorization {
        authorization: evidence.authorization.clone(),
        writer: state.writer,
        presented_tip: log.tip().checkpoint,
    })
}

// ---------------------------------------------------------------------------------------------
// Fork evidence
// ---------------------------------------------------------------------------------------------

/// Two entries that both claim the same position of the same history. Checkable by anyone
/// holding the inception; needs neither log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogForkProofV0 {
    /// One entry.
    pub first: SignedLogEntryV0,
    /// A different entry at the same `(context, seq, prev)`.
    pub second: SignedLogEntryV0,
}

fn valid_signers(inception: &ContextInceptionV0, entry: &SignedLogEntryV0) -> Vec<PrincipalKey> {
    let bytes = entry.body.canonical_bytes();
    let verifies = |key: &PrincipalKey, sig: &[u8; 64]| {
        key.verifying_key()
            .verify_strict(&bytes, &Signature::from_bytes(sig))
            .is_ok()
    };
    match &entry.authority {
        EntryAuthorityV0::Founders(signatures) => signatures
            .iter()
            .filter(|s| inception.is_founder(&s.founder) && verifies(&s.founder, &s.signature))
            .map(|s| s.founder)
            .collect(),
        EntryAuthorityV0::Decision(d) => {
            if verifies(&d.signer, &d.signature) {
                vec![d.signer]
            } else {
                Vec::new()
            }
        }
    }
}

impl LogForkProofV0 {
    /// The principals that signed **both** entries (the equivocators), or `None` if this is not
    /// a conflict: different context, position or predecessor, identical bodies, or an entry
    /// with no valid signature.
    ///
    /// An empty list is still a conflict: two different principals each sequenced one side (a
    /// contested succession). The proof attributes signatures only; whether either side was
    /// *authorized* is decided by replaying it ([`replay_context_log`]).
    pub fn equivocators(&self, inception: &ContextInceptionV0) -> Option<Vec<PrincipalKey>> {
        let (a, b) = (&self.first.body, &self.second.body);
        let context = inception.context_ref();
        if a.context != context || b.context != context {
            return None;
        }
        if a.seq != b.seq || a.prev != b.prev || a.head() == b.head() {
            return None;
        }
        let first = valid_signers(inception, &self.first);
        let second = valid_signers(inception, &self.second);
        if first.is_empty() || second.is_empty() {
            return None;
        }
        Some(first.into_iter().filter(|k| second.contains(k)).collect())
    }
}

/// The first position at which two presented histories of one context disagree, as a proof.
/// `None` if one is a prefix of the other, or the first disagreement is not a checkable conflict
/// (see [`LogForkProofV0::equivocators`]).
pub fn find_log_fork(
    inception: &ContextInceptionV0,
    a: &[SignedLogEntryV0],
    b: &[SignedLogEntryV0],
) -> Option<LogForkProofV0> {
    let (x, y) = a
        .iter()
        .zip(b)
        .find(|(x, y)| x.body.head() != y.body.head())?;
    let proof = LogForkProofV0 {
        first: x.clone(),
        second: y.clone(),
    };
    proof.equivocators(inception).map(|_| proof)
}

/// Sign a [`CheckpointedAuthorizationV0`] (a convenience over [`Signed::sign`]).
pub fn sign_checkpointed_authorization(
    body: CheckpointedAuthorizationV0,
    writer: &SigningKey,
) -> Signed<CheckpointedAuthorizationV0> {
    Signed::sign(body, writer)
}
