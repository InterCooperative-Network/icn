//! Authority-to-effect spine v0 (EXPERIMENTAL): positive and adversarial scenarios.
//!
//! Signed vote (N4 `DeviceActV1`, reused) → verified attribution (N4 evidence bundle, reused) →
//! standing → adopted `ThresholdRuleV0` → writer-signed authorization → custodian admission →
//! outcome receipt.
//!
//! Every adversarial case asserts one specific refusal variant. Fixture identities only: every key
//! is derived from a seed byte; nothing here is a real person, device or cooperative.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use ed25519_dalek::SigningKey;
use icn_authz::effect_v0::{
    admit, issue_outcome_receipt, target_id, verify_authorization, verify_authorization_bytes,
    verify_outcome_receipt, AdmissionRefusal, AdoptedRule, AuthorizationEvidenceV0,
    AuthorizationV0, AuthzRefusal, ContextInceptionV0, ContextRef, EffectV0, InMemoryConsumedSet,
    InputActV0, OutcomeReceiptV0, ReceiptRefusal, RuleAdoptionV0, RuleError, Signed,
    SignedDesignation, StandingV0, StatementKind, Verdict, WriterDesignationV0,
};
use icn_governance::authz_threshold::{GovernanceActV0, Ratio, ThresholdRuleV0};
use icn_governance::vote::VoteChoice;
use icn_identity::authority_log::{
    authorize_event, revoke_event, AuthorityStore, CapabilitySet, ContextNonce, ContinuityRoot,
    DeviceCapability, EstablishmentKind, PrincipalKey, SignedAuthorityEvent, SubjectId,
};
use icn_identity::device_authority::{
    sign_device_act, DeviceActV1, DeviceActVerifyError, DeviceAuthorityRefusal,
};
use icn_identity::evidence_bundle::{EvidenceBundle, EvidenceVerifyError};
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------------------------

const NOW: u64 = 1_000;
const STANDING_UNTIL: u64 = 2_000;
const AUTH_UNTIL: u64 = 1_500;
const PROPOSAL: [u8; 32] = [0x50; 32];
const OTHER_PROPOSAL: [u8; 32] = [0x51; 32];
const ROLE: &str = "member";

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn principal(seed: u8) -> PrincipalKey {
    PrincipalKey::try_from_verifying_key(key(seed).verifying_key()).unwrap()
}

/// A Subject with one device. The authority log is `[inception, authorize device @1, extra...]`.
struct Member {
    root: ContinuityRoot,
    subject: SubjectId,
    events: Vec<SignedAuthorityEvent>,
    device_key: SigningKey,
    device: PrincipalKey,
}

impl Member {
    fn new(seed: u8, caps: CapabilitySet) -> Self {
        let root = ContinuityRoot::with_plan(
            [seed; 32],
            ContextNonce::from_bytes([seed ^ 0xa5; 32]),
            vec![EstablishmentKind::Rotate; 2],
        );
        let inception = root.incept().unwrap();
        let subject = inception.body.subject();
        let device_key = key(seed.wrapping_add(0x40));
        let device = principal(seed.wrapping_add(0x40));
        let authorize = authorize_event(
            &root.authority_signing_key(0),
            subject,
            1,
            inception.body.event_id(),
            device,
            caps,
            None,
        );
        Member {
            root,
            subject,
            events: vec![inception, authorize],
            device_key,
            device,
        }
    }

    fn standard(seed: u8) -> Self {
        Member::new(seed, CapabilitySet::new([DeviceCapability::Sign]))
    }

    /// Append `revoke device @2`.
    fn revoke_device(&mut self) {
        let prev = self.events.last().unwrap().body.event_id();
        self.events.push(revoke_event(
            &self.root.authority_signing_key(0),
            self.subject,
            2,
            prev,
            self.device,
        ));
    }

    fn store(&self) -> AuthorityStore {
        let mut store = AuthorityStore::new();
        store.ingest_all(self.events.iter());
        store
    }

    /// A signed governance act, transported as an N4 evidence bundle.
    fn vote(
        &self,
        context: ContextRef,
        proposal: [u8; 32],
        choice: VoteChoice,
        nonce: u8,
        position: u64,
        capability: DeviceCapability,
    ) -> EvidenceBundle {
        let payload = GovernanceActV0 {
            context: context.0,
            proposal,
            choice,
            act_nonce: [nonce; 32],
        }
        .canonical_bytes();
        let act =
            DeviceActV1::new(self.subject, self.device, capability, position, payload).unwrap();
        let signed = sign_device_act(&act, &self.device_key).unwrap();
        EvidenceBundle::assemble(&self.store(), &signed).unwrap()
    }
}

struct World {
    founders: [SigningKey; 3],
    inception: ContextInceptionV0,
    ctx: ContextRef,
    writer: SigningKey,
    designation: SignedDesignation,
    members: Vec<Member>,
    rule: ThresholdRuleV0,
    custodian: SigningKey,
    target: [u8; 32],
}

fn rule() -> ThresholdRuleV0 {
    ThresholdRuleV0 {
        role: ROLE.into(),
        quorum: Ratio { num: 2, den: 3 },
        approval: Ratio { num: 2, den: 3 },
    }
}

fn world() -> World {
    let founders = [key(0xF1), key(0xF2), key(0xF3)];
    let inception = ContextInceptionV0::new(
        founders
            .iter()
            .map(|k| PrincipalKey::try_from_verifying_key(k.verifying_key()).unwrap())
            .collect(),
        2,
        [0x1a; 32],
    )
    .unwrap();
    let ctx = inception.context_ref();
    let writer = key(0xA1);
    let designation = designate(&founders[..2], ctx, 1, &writer);
    World {
        founders,
        inception,
        ctx,
        writer,
        designation,
        members: vec![
            Member::standard(0x01),
            Member::standard(0x02),
            Member::standard(0x03),
        ],
        rule: rule(),
        custodian: key(0xC1),
        target: target_id(&principal(0xC1)),
    }
}

fn designate(
    signers: &[SigningKey],
    ctx: ContextRef,
    epoch: u64,
    writer: &SigningKey,
) -> SignedDesignation {
    let refs: Vec<&SigningKey> = signers.iter().collect();
    SignedDesignation::sign(
        WriterDesignationV0 {
            context: ctx,
            epoch,
            writer: PrincipalKey::try_from_verifying_key(writer.verifying_key()).unwrap(),
            start_head: [epoch as u8; 32],
        },
        &refs,
    )
    .unwrap()
}

/// Everything the writer chooses, editable by a test before [`Draft::seal`] signs it.
struct Draft {
    inception: ContextInceptionV0,
    designation: SignedDesignation,
    writer: SigningKey,
    epoch: u64,
    standing: Vec<StandingV0>,
    rule_artifact: Vec<u8>,
    adopted_hash: [u8; 32],
    bundles: Vec<EvidenceBundle>,
    inputs: Vec<InputActV0>,
    verdict: Verdict,
    subject_matter: [u8; 32],
    effect: EffectV0,
    operation_id: [u8; 32],
    not_after: u64,
    auth_rule_hash: Option<[u8; 32]>,
}

fn inputs_of(bundles: &[EvidenceBundle]) -> Vec<InputActV0> {
    let mut inputs: Vec<InputActV0> = bundles
        .iter()
        .map(|b| InputActV0 {
            act_id: b.act().act.act_id(),
            subject: b.subject(),
            position: b.evaluation_position(),
        })
        .collect();
    inputs.sort_by_key(|i| i.act_id);
    inputs
}

impl World {
    fn standing_for(&self, subject: SubjectId) -> StandingV0 {
        StandingV0 {
            context: self.ctx,
            epoch: 1,
            subject,
            role: ROLE.into(),
            not_after: STANDING_UNTIL,
        }
    }

    fn vote(&self, member: usize, choice: VoteChoice) -> EvidenceBundle {
        self.members[member].vote(self.ctx, PROPOSAL, choice, 0, 1, DeviceCapability::Sign)
    }

    /// The honest draft over `bundles`: standing for all three members, verdict Pass.
    fn draft(&self, bundles: Vec<EvidenceBundle>) -> Draft {
        let artifact = self.rule.artifact();
        Draft {
            inception: self.inception.clone(),
            designation: self.designation.clone(),
            writer: self.writer.clone(),
            epoch: 1,
            standing: self
                .members
                .iter()
                .map(|m| self.standing_for(m.subject))
                .collect(),
            adopted_hash: Sha256::digest(&artifact).into(),
            rule_artifact: artifact,
            inputs: inputs_of(&bundles),
            bundles,
            verdict: Verdict::Pass,
            subject_matter: PROPOSAL,
            effect: EffectV0 {
                target: self.target,
                action: "noop".into(),
                params_hash: [0xee; 32],
            },
            operation_id: [0x09; 32],
            not_after: AUTH_UNTIL,
            auth_rule_hash: None,
        }
    }

    /// Two For votes from members 0 and 1.
    fn passing(&self) -> Draft {
        self.draft(vec![
            self.vote(0, VoteChoice::For),
            self.vote(1, VoteChoice::For),
        ])
    }

    fn verify(&self, ev: &AuthorizationEvidenceV0) -> Result<Verdict, AuthzRefusal> {
        verify_authorization(ev, &self.rule, NOW).map(|v| v.authorization().verdict)
    }

    fn admit(
        &self,
        ev: &AuthorizationEvidenceV0,
        now: u64,
        latest_epoch: u64,
        consumed: &mut InMemoryConsumedSet,
    ) -> Result<(), AdmissionRefusal> {
        admit(ev, &self.rule, self.target, now, latest_epoch, consumed).map(|_| ())
    }
}

impl Draft {
    fn seal(self) -> AuthorizationEvidenceV0 {
        let ctx = self.designation.body.context;
        let mut ev = AuthorizationEvidenceV0 {
            inception: self.inception,
            standing: self
                .standing
                .into_iter()
                .map(|s| Signed::sign(s, &self.writer))
                .collect(),
            rule_adoption: Signed::sign(
                RuleAdoptionV0 {
                    context: ctx,
                    epoch: self.epoch,
                    rule_hash: self.adopted_hash,
                },
                &self.writer,
            ),
            rule_artifact: self.rule_artifact,
            vote_bundles: self.bundles,
            authorization: Signed::sign(
                AuthorizationV0 {
                    context: ctx,
                    epoch: self.epoch,
                    subject_matter: self.subject_matter,
                    rule_hash: self.auth_rule_hash.unwrap_or(self.adopted_hash),
                    verdict: self.verdict,
                    inputs: self.inputs,
                    effect: self.effect,
                    operation_id: self.operation_id,
                    not_after: self.not_after,
                },
                &self.writer,
            ),
            designation: self.designation,
        };
        ev.canonicalize();
        ev
    }
}

// ---------------------------------------------------------------------------------------------
// Positive
// ---------------------------------------------------------------------------------------------

/// P1: three members, approval 2/3, two For votes → Pass, admitted exactly once.
#[test]
fn p1_two_of_three_for_passes_and_is_admitted_once() {
    let w = world();
    let ev = w.passing().seal();
    assert_eq!(w.verify(&ev), Ok(Verdict::Pass));
    let mut consumed = InMemoryConsumedSet::default();
    assert_eq!(w.admit(&ev, NOW, 1, &mut consumed), Ok(()));
    assert_eq!(
        w.admit(&ev, NOW, 1, &mut consumed),
        Err(AdmissionRefusal::Replayed)
    );
}

/// P2: the verifier needs only bytes and the rule artifact carried in them — no node, store,
/// network or secret. The rule implementation is rebuilt from the artifact.
#[test]
fn p2_verifies_from_bytes_alone() {
    let bytes = world().passing().seal().encode();
    let decoded = AuthorizationEvidenceV0::decode(&bytes).unwrap();
    assert_eq!(decoded.encode(), bytes, "evidence encoding is canonical");
    let rule = ThresholdRuleV0::from_artifact(&decoded.rule_artifact).unwrap();
    let verified = verify_authorization_bytes(&bytes, &rule, NOW).unwrap();
    assert_eq!(verified.authorization().verdict, Verdict::Pass);
}

/// P3: Abstain counts toward quorum only.
#[test]
fn p3_abstain_counts_toward_quorum_only() {
    let w = world();
    // 1 For + 1 Abstain: participation 2/3 meets quorum; approval 1/1.
    let ev = w
        .draft(vec![
            w.vote(0, VoteChoice::For),
            w.vote(1, VoteChoice::Abstain),
        ])
        .seal();
    assert_eq!(w.verify(&ev), Ok(Verdict::Pass));

    // 1 For alone: approval 1/1 but participation 1/3 misses quorum → Fail, and a writer that
    // signs Fail is verified as telling the truth.
    let mut d = w.draft(vec![w.vote(0, VoteChoice::For)]);
    d.verdict = Verdict::Fail;
    assert_eq!(w.verify(&d.seal()), Ok(Verdict::Fail));

    // 1 For + 1 Against + 1 Abstain: quorum met, approval 1/2 < 2/3 → Fail. Abstain did not
    // become a For.
    let mut d = w.draft(vec![
        w.vote(0, VoteChoice::For),
        w.vote(1, VoteChoice::Against),
        w.vote(2, VoteChoice::Abstain),
    ]);
    d.verdict = Verdict::Fail;
    let ev = d.seal();
    assert_eq!(w.verify(&ev), Ok(Verdict::Fail));
    // A verified Fail is never admitted.
    assert_eq!(
        w.admit(&ev, NOW, 1, &mut InMemoryConsumedSet::default()),
        Err(AdmissionRefusal::VerdictNotPass)
    );
}

/// P4: the custodian's outcome receipt verifies against the authorization, by a third party.
#[test]
fn p4_outcome_receipt_verifies_against_authorization() {
    let w = world();
    let ev = w.passing().seal();
    let admission = admit(
        &ev,
        &w.rule,
        w.target,
        NOW,
        1,
        &mut InMemoryConsumedSet::default(),
    )
    .unwrap();
    let receipt = issue_outcome_receipt(&admission, &w.custodian, [0x4e; 32]).unwrap();

    // Third party: bytes only.
    let receipt = Signed::<OutcomeReceiptV0>::decode(&receipt.encode()).unwrap();
    let verified = verify_authorization_bytes(&ev.encode(), &w.rule, NOW).unwrap();
    assert_eq!(verify_outcome_receipt(&receipt, &verified), Ok(()));

    // A receipt from another key is not the target's.
    let impostor = issue_outcome_receipt(&admission, &key(0xC2), [0x4e; 32]).unwrap();
    assert_eq!(
        verify_outcome_receipt(&impostor, &verified),
        Err(ReceiptRefusal::WrongCustodian)
    );
    // A receipt whose result was altered after signing.
    let mut altered = receipt.clone();
    altered.body.result_hash = [0x4f; 32];
    assert_eq!(
        verify_outcome_receipt(&altered, &verified),
        Err(ReceiptRefusal::BadSignature)
    );
    // A receipt for another authorization.
    let other = w.passing();
    let mut other = other;
    other.operation_id = [0x0a; 32];
    let other = verify_authorization(&other.seal(), &w.rule, NOW).unwrap();
    assert_eq!(
        verify_outcome_receipt(&receipt, &other),
        Err(ReceiptRefusal::OperationMismatch)
    );
}

/// P5 (governance side): golden vectors for the act payload and the rule artifact, and
/// decode∘encode as the identity.
#[test]
fn p5_governance_encodings() {
    let act = GovernanceActV0 {
        context: [0x2c; 32],
        proposal: PROPOSAL,
        choice: VoteChoice::Against,
        act_nonce: [0x07; 32],
    };
    let bytes = act.canonical_bytes();
    assert_eq!(GovernanceActV0::decode(&bytes).unwrap(), act);
    let artifact = rule().artifact();
    assert_eq!(ThresholdRuleV0::from_artifact(&artifact).unwrap(), rule());
    let hex = |b: &[u8]| -> String {
        Sha256::digest(b)
            .iter()
            .map(|x| format!("{x:02x}"))
            .collect()
    };
    eprintln!("governance_act: {}", hex(&bytes));
    eprintln!("threshold_rule: {}", hex(&artifact));
    assert_eq!(hex(&bytes), GOLDEN_GOVERNANCE_ACT);
    assert_eq!(hex(&artifact), GOLDEN_THRESHOLD_RULE);

    // Strictness: trailing byte, unknown choice, malformed ratios.
    let mut long = bytes.clone();
    long.push(0);
    assert!(GovernanceActV0::decode(&long).is_err());
    let mut bad_choice = bytes.clone();
    bad_choice[bytes.len() - 33] = 9;
    assert!(GovernanceActV0::decode(&bad_choice).is_err());
    let mut zero_den = rule();
    zero_den.quorum = Ratio { num: 0, den: 0 };
    assert_eq!(
        ThresholdRuleV0::from_artifact(&zero_den.artifact()),
        Err(RuleError::MalformedArtifact("quorum"))
    );
}

const GOLDEN_GOVERNANCE_ACT: &str =
    "4bc4a49af98dd62c6b0aeae230dc8467e157612463490dde282f235ce915b039";
const GOLDEN_THRESHOLD_RULE: &str =
    "38106f3fff4720f4f2a3929c5d091658f51e1c7eb91d2c2f357467ffdb81d131";

/// Epoch progression, positive side: an epoch-2 writer designated by the founder threshold is
/// admitted by a custodian that knows epoch 2.
#[test]
fn epoch_two_writer_is_admitted() {
    let w = world();
    let writer2 = key(0xA2);
    let mut d = w.passing();
    d.designation = designate(&w.founders[1..], w.ctx, 2, &writer2);
    d.writer = writer2;
    d.epoch = 2;
    for s in &mut d.standing {
        s.epoch = 2;
    }
    let ev = d.seal();
    assert_eq!(
        w.admit(&ev, NOW, 2, &mut InMemoryConsumedSet::default()),
        Ok(())
    );
}

// ---------------------------------------------------------------------------------------------
// Adversarial
// ---------------------------------------------------------------------------------------------

/// A1: any byte flipped in a vote bundle, standing statement, designation or authorization is
/// refused. Exhaustive over every byte of each signed component.
#[test]
fn a1_any_flipped_byte_is_refused() {
    let w = world();
    let ev = w.passing().seal();
    let mut flips = 0usize;

    let mut check =
        |name: &str, len: usize, rebuild: &dyn Fn(usize) -> Option<AuthorizationEvidenceV0>| {
            for i in 0..len {
                if let Some(tampered) = rebuild(i) {
                    assert!(
                        verify_authorization(&tampered, &w.rule, NOW).is_err(),
                        "{name}: flip at byte {i} still verified"
                    );
                }
                flips += 1;
            }
        };

    let des = ev.designation.encode();
    check("designation", des.len(), &|i| {
        let mut b = des.clone();
        b[i] ^= 0x01;
        let mut t = ev.clone();
        t.designation = SignedDesignation::decode(&b).ok()?;
        Some(t)
    });
    let st = ev.standing[0].encode();
    check("standing", st.len(), &|i| {
        let mut b = st.clone();
        b[i] ^= 0x01;
        let mut t = ev.clone();
        t.standing[0] = Signed::decode(&b).ok()?;
        Some(t)
    });
    let auth = ev.authorization.encode();
    check("authorization", auth.len(), &|i| {
        let mut b = auth.clone();
        b[i] ^= 0x01;
        let mut t = ev.clone();
        t.authorization = Signed::decode(&b).ok()?;
        Some(t)
    });
    let bundle = ev.vote_bundles[0].canonical_bytes();
    check("vote bundle", bundle.len(), &|i| {
        let mut b = bundle.clone();
        b[i] ^= 0x01;
        let mut t = ev.clone();
        t.vote_bundles[0] = EvidenceBundle::decode(&b).ok()?;
        Some(t)
    });
    assert!(flips > 1_000, "the sweep covered {flips} bytes");

    // Specific variants for a flip inside each signature.
    let last = |mut b: Vec<u8>| {
        let n = b.len();
        b[n - 1] ^= 0x01;
        b
    };
    let mut t = ev.clone();
    t.authorization = Signed::decode(&last(ev.authorization.encode())).unwrap();
    assert_eq!(
        w.verify(&t),
        Err(AuthzRefusal::BadWriterSignature(
            StatementKind::Authorization
        ))
    );
    let mut t = ev.clone();
    t.standing[0] = Signed::decode(&last(ev.standing[0].encode())).unwrap();
    assert_eq!(
        w.verify(&t),
        Err(AuthzRefusal::BadWriterSignature(StatementKind::Standing))
    );
    let mut t = ev.clone();
    t.designation = SignedDesignation::decode(&last(ev.designation.encode())).unwrap();
    assert_eq!(w.verify(&t), Err(AuthzRefusal::DesignationBadSignature));
}

/// A2: a vote from a device without `Sign`, or whose grant was revoked before the evaluation
/// position, is refused by the reused N4 verifier.
#[test]
fn a2_device_without_sign_or_revoked_is_refused() {
    let w = world();

    // Granted only Present, signs as Sign → N4 CapabilityNotGranted.
    let present_only = Member::new(0x01, CapabilitySet::new([DeviceCapability::Present]));
    let bundle = present_only.vote(
        w.ctx,
        PROPOSAL,
        VoteChoice::For,
        0,
        1,
        DeviceCapability::Sign,
    );
    let ev = w.draft(vec![bundle, w.vote(1, VoteChoice::For)]).seal();
    assert!(matches!(
        w.verify(&ev),
        Err(AuthzRefusal::VoteRefused {
            error: EvidenceVerifyError::Act(DeviceActVerifyError::Refused(
                DeviceAuthorityRefusal::CapabilityNotGranted(DeviceCapability::Sign)
            )),
            ..
        })
    ));

    // Granted Present, signs as Present → N4 accepts, the spine refuses: votes need Sign.
    let bundle = present_only.vote(
        w.ctx,
        PROPOSAL,
        VoteChoice::For,
        0,
        1,
        DeviceCapability::Present,
    );
    let id = bundle.act().act.act_id();
    let ev = w.draft(vec![bundle, w.vote(1, VoteChoice::For)]).seal();
    assert_eq!(w.verify(&ev), Err(AuthzRefusal::WrongCapability(id)));

    // Revoked at position 2, act evaluated at 2 → N4 refuses (grant no longer in force).
    let mut revoked = Member::standard(0x01);
    revoked.revoke_device();
    let bundle = revoked.vote(
        w.ctx,
        PROPOSAL,
        VoteChoice::For,
        0,
        2,
        DeviceCapability::Sign,
    );
    let ev = w.draft(vec![bundle, w.vote(1, VoteChoice::For)]).seal();
    assert!(matches!(
        w.verify(&ev),
        Err(AuthzRefusal::VoteRefused {
            error: EvidenceVerifyError::Act(DeviceActVerifyError::Refused(_)),
            ..
        })
    ));
}

/// A3: a voter without standing, with expired standing, or with standing for another role.
#[test]
fn a3_voter_standing_is_enforced() {
    let w = world();

    let mut d = w.passing();
    let s0 = w.members[0].subject;
    d.standing.retain(|s| s.subject != s0);
    assert_eq!(w.verify(&d.seal()), Err(AuthzRefusal::NoStanding(s0)));

    let mut d = w.passing();
    for s in &mut d.standing {
        if s.subject == s0 {
            s.not_after = NOW - 1;
        }
    }
    assert_eq!(
        w.verify(&d.seal()),
        Err(AuthzRefusal::StandingExpired {
            subject: s0,
            not_after: NOW - 1,
            now: NOW
        })
    );

    let mut d = w.passing();
    for s in &mut d.standing {
        if s.subject == s0 {
            s.role = "observer".into();
        }
    }
    assert_eq!(
        w.verify(&d.seal()),
        Err(AuthzRefusal::StandingRoleMismatch(s0))
    );
}

/// A4: the same Subject votes twice (two distinct signed acts): the rule refuses, rather than the
/// writer choosing which one counts.
#[test]
fn a4_double_vote_is_refused() {
    let w = world();
    let m = &w.members[0];
    let first = m.vote(
        w.ctx,
        PROPOSAL,
        VoteChoice::For,
        1,
        1,
        DeviceCapability::Sign,
    );
    let second = m.vote(
        w.ctx,
        PROPOSAL,
        VoteChoice::For,
        2,
        1,
        DeviceCapability::Sign,
    );
    let ev = w.draft(vec![first, second]).seal();
    assert_eq!(
        w.verify(&ev),
        Err(AuthzRefusal::RuleRefused(RuleError::DuplicateSubject(
            m.subject
        )))
    );
}

/// A5: a valid vote replayed into another proposal, or another context, is not bound.
#[test]
fn a5_cross_proposal_and_cross_context_replay_is_refused() {
    let w = world();
    let m = &w.members[0];

    let foreign = m.vote(
        w.ctx,
        OTHER_PROPOSAL,
        VoteChoice::For,
        0,
        1,
        DeviceCapability::Sign,
    );
    let id = foreign.act().act.act_id();
    let ev = w.draft(vec![foreign, w.vote(1, VoteChoice::For)]).seal();
    assert_eq!(
        w.verify(&ev),
        Err(AuthzRefusal::RuleRefused(RuleError::ActNotBound {
            act_id: id
        }))
    );

    let other_ctx = ContextRef([0x77; 32]);
    let foreign = m.vote(
        other_ctx,
        PROPOSAL,
        VoteChoice::For,
        0,
        1,
        DeviceCapability::Sign,
    );
    let id = foreign.act().act.act_id();
    let ev = w.draft(vec![foreign, w.vote(1, VoteChoice::For)]).seal();
    assert_eq!(
        w.verify(&ev),
        Err(AuthzRefusal::RuleRefused(RuleError::ActNotBound {
            act_id: id
        }))
    );
}

/// A6: the writer signs Pass over inputs that do not meet the threshold.
#[test]
fn a6_verdict_mismatch_is_refused() {
    let w = world();
    let ev = w
        .draft(vec![
            w.vote(0, VoteChoice::For),
            w.vote(1, VoteChoice::Against),
            w.vote(2, VoteChoice::Against),
        ])
        .seal();
    assert_eq!(
        w.verify(&ev),
        Err(AuthzRefusal::VerdictMismatch {
            signed: Verdict::Pass,
            evaluated: Verdict::Fail
        })
    );
}

/// A7: the writer omits a valid Against vote. Supplying that vote alongside the authorization is
/// censorship evidence; stripping a counted vote is also refused.
#[test]
fn a7_omitted_or_stripped_votes_are_refused() {
    let w = world();
    let against = w.vote(2, VoteChoice::Against);
    let against_id = against.act().act.act_id();
    // The writer counted only the two For votes (and would sign Pass).
    let mut d = w.passing();
    // The omitted voter attaches its own bundle.
    d.bundles.push(against);
    assert_eq!(
        w.verify(&d.seal()),
        Err(AuthzRefusal::UnlistedAct(against_id))
    );

    // A counted vote stripped in transit.
    let mut ev = w.passing().seal();
    let stripped = ev.vote_bundles.remove(0).act().act.act_id();
    assert_eq!(w.verify(&ev), Err(AuthzRefusal::InputActMissing(stripped)));
}

/// A8: the authorization's rule is not the adopted one; the artifact does not hash to the adopted
/// hash; the verifier's implementation is not the adopted rule.
#[test]
fn a8_rule_hash_chain_is_enforced() {
    let w = world();

    let mut d = w.passing();
    d.auth_rule_hash = Some([0x99; 32]);
    assert_eq!(w.verify(&d.seal()), Err(AuthzRefusal::RuleNotAdopted));

    let mut d = w.passing();
    let mut lax = rule();
    lax.approval = Ratio { num: 1, den: 3 };
    d.rule_artifact = lax.artifact();
    assert_eq!(w.verify(&d.seal()), Err(AuthzRefusal::RuleArtifactMismatch));

    // A custodian that pins its own rule refuses a consistent chain for a different rule.
    let ev = w.passing().seal();
    assert_eq!(
        verify_authorization(&ev, &lax, NOW).map(|_| ()),
        Err(AuthzRefusal::RuleImplementationMismatch)
    );
}

/// A9: a stale writer. An epoch-1 authorization after the custodian has seen epoch 2.
#[test]
fn a9_stale_epoch_is_refused() {
    let w = world();
    let ev = w.passing().seal();
    assert_eq!(
        w.admit(&ev, NOW, 2, &mut InMemoryConsumedSet::default()),
        Err(AdmissionRefusal::StaleEpoch {
            epoch: 1,
            latest_known: 2
        })
    );
    // An epoch-1 writer cannot claim epoch 2 under the epoch-1 designation.
    let mut d = w.passing();
    d.epoch = 2;
    assert_eq!(
        w.verify(&d.seal()),
        Err(AuthzRefusal::EpochMismatch(StatementKind::Authorization))
    );
    // Nor can a stranger sign under the epoch-1 designation.
    let mut d = w.passing();
    d.writer = key(0xA9);
    assert_eq!(
        w.verify(&d.seal()),
        Err(AuthzRefusal::BadWriterSignature(
            StatementKind::Authorization
        ))
    );
}

/// A10: an authorization replayed after its operation id is consumed. A refused admission does
/// not consume.
#[test]
fn a10_replay_is_refused_and_refusals_do_not_consume() {
    let w = world();
    let ev = w.passing().seal();
    let mut consumed = InMemoryConsumedSet::default();
    // Refused first (expired): nothing consumed.
    assert!(matches!(
        w.admit(&ev, AUTH_UNTIL + 1, 1, &mut consumed),
        Err(AdmissionRefusal::Expired { .. })
    ));
    assert_eq!(w.admit(&ev, NOW, 1, &mut consumed), Ok(()));
    assert_eq!(
        w.admit(&ev, NOW, 1, &mut consumed),
        Err(AdmissionRefusal::Replayed)
    );
}

/// A11: expired.
#[test]
fn a11_expired_is_refused() {
    let w = world();
    let ev = w.passing().seal();
    assert_eq!(
        w.admit(&ev, AUTH_UNTIL + 1, 1, &mut InMemoryConsumedSet::default()),
        Err(AdmissionRefusal::Expired {
            not_after: AUTH_UNTIL,
            now: AUTH_UNTIL + 1
        })
    );
}

/// A12: an authorization for another target.
#[test]
fn a12_wrong_target_is_refused() {
    let w = world();
    let ev = w.passing().seal();
    let other_target = target_id(&principal(0xC2));
    assert_eq!(
        admit(
            &ev,
            &w.rule,
            other_target,
            NOW,
            1,
            &mut InMemoryConsumedSet::default()
        )
        .map(|_| ()),
        Err(AdmissionRefusal::WrongTarget)
    );
}

/// A13: two authorizations with one operation id and different effects. The second is refused with
/// a third-party-verifiable proof; afterwards neither is admissible at this custodian.
#[test]
fn a13_equivocation_yields_a_proof() {
    let w = world();
    let first = w.passing().seal();
    let mut d = w.passing();
    d.effect.params_hash = [0xef; 32];
    let second = d.seal();
    let mut consumed = InMemoryConsumedSet::default();
    assert_eq!(w.admit(&first, NOW, 1, &mut consumed), Ok(()));
    let proof = match w.admit(&second, NOW, 1, &mut consumed) {
        Err(AdmissionRefusal::Equivocation(proof)) => proof,
        other => panic!("expected equivocation, got {other:?}"),
    };
    assert!(proof.is_valid());
    assert_eq!(
        proof.first.authorization.body.operation_id,
        proof.second.authorization.body.operation_id
    );
    // A forged proof (bodies swapped under the wrong signature) is not valid.
    let mut forged = (*proof).clone();
    forged.second.authorization.body.effect.params_hash = [0xee; 32];
    assert!(!forged.is_valid());
    // Neither is admissible again here.
    assert_eq!(
        w.admit(&first, NOW, 1, &mut consumed),
        Err(AdmissionRefusal::Replayed)
    );
    assert!(matches!(
        w.admit(&second, NOW, 1, &mut consumed),
        Err(AdmissionRefusal::Equivocation(_))
    ));
}

/// A14: a designation signed below the founder threshold, or by a non-founder.
#[test]
fn a14_designation_threshold_is_enforced() {
    let w = world();
    let mut d = w.passing();
    d.designation = designate(&w.founders[..1], w.ctx, 1, &w.writer);
    assert_eq!(
        w.verify(&d.seal()),
        Err(AuthzRefusal::DesignationBelowThreshold {
            valid: 1,
            threshold: 2
        })
    );

    let mut d = w.passing();
    d.designation = designate(&[w.founders[0].clone(), key(0xEE)], w.ctx, 1, &w.writer);
    assert_eq!(
        w.verify(&d.seal()),
        Err(AuthzRefusal::DesignationSignerNotFounder)
    );
}

/// A15: the writer-signed evaluation position differs from the act's own; the reused N4 verifier
/// surfaces `EvaluationPositionMismatch`. A wrong Subject surfaces `ActSubjectMismatch`.
#[test]
fn a15_evaluation_position_mismatch_is_surfaced() {
    let w = world();
    let mut d = w.passing();
    d.inputs[0].position = 2;
    assert!(matches!(
        w.verify(&d.seal()),
        Err(AuthzRefusal::VoteRefused {
            error: EvidenceVerifyError::Act(DeviceActVerifyError::EvaluationPositionMismatch {
                act: 1,
                relying: 2
            }),
            ..
        })
    ));

    let mut d = w.passing();
    d.inputs[0].subject = w.members[2].subject;
    assert!(matches!(
        w.verify(&d.seal()),
        Err(AuthzRefusal::VoteRefused {
            error: EvidenceVerifyError::Act(DeviceActVerifyError::ActSubjectMismatch),
            ..
        })
    ));
}

/// The rule is generic over the kernel boundary: the trait object the verifier receives is the
/// governance rule, and its hash is the artifact's hash.
#[test]
fn rule_hash_is_the_artifact_hash() {
    let r = rule();
    let expected: [u8; 32] = Sha256::digest(r.artifact()).into();
    assert_eq!(AdoptedRule::rule_hash(&r), expected);
    assert_eq!(world().inception.threshold, 2);
}
