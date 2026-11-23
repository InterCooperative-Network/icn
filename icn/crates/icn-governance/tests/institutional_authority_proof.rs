//! Institutional authority lifecycle — cross-crate composition proof (EXPERIMENTAL, REHEARSAL).
//!
//! This is an integration harness, not a protocol. It composes components that already exist and
//! asks whether they fit together end to end:
//!
//! ```text
//! GEN-A Subject genesis (icn-identity::subject_context)
//!   -> verified N1/N4 device authority (authority_log, device_authority, evidence_bundle)
//!   -> GEN-B rehearsal recognition (icn-governance::subject_recognition, BootstrapRehearsalV1)
//!   -> standing in a Context Authority Log (icn-authz::effect_v0::log, #2885)
//!   -> derived electorate + adopted ThresholdRuleV0 (icn-governance::authz_threshold, #2884)
//!   -> Subject-signed GovernanceActV0 carried as N4 DeviceActV1 evidence
//!   -> checkpointed authorization (#2886) re-verified by an independent verifier from bytes only
//!   -> target-side atomic local reservation (icn-authz-store, #2887/#2888)
//!   -> restart, reconciliation and replay refusal.
//! ```
//!
//! # What is synthetic here — read before citing any result
//!
//! - The institution is **synthetic**. The only recognition basis is GEN-B's
//!   `BootstrapRehearsalV1`, which is explicitly non-production: no institution decided anything.
//! - Founders are test keys. Nothing here shows they hold legitimate institutional authority.
//! - The binding between the authz `ContextRef` and the GEN-A/GEN-B `GovernanceDomainId` is a
//!   **harness-local convention** ([`harness_domain_binding`] placed in `label_hash`, which the
//!   authz layer binds but never interprets). No production type carries that binding, and no
//!   log transition carries recognition evidence. The tests below demonstrate that gap
//!   (`the_log_alone_accepts_standing_for_a_foreign_or_unrecognized_subject`); the recognition
//!   gate in [`independent_verify`] is test code, not a proposed format.
//! - "Reservation" is a durable local record that capacity was decremented. No Linux resource is
//!   enforced, nothing is executed, nothing is settled, and there is no exactly-once claim beyond
//!   one custodian's local store.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use ed25519_dalek::SigningKey;
use icn_authz::effect_v0::{
    find_log_fork, replay_context_log, sign_checkpointed_authorization, transition_subject_matter,
    AdmissionRefusal, AdoptedRule, CanonicalV0, CheckpointedAuthorizationV0,
    CheckpointedEvidenceV0, ContextInceptionV0, ContextRef, EffectV0, InputActV0, LogCheckpointV0,
    LogEntryV0, LogRefusal, RelyingFrontierV0, RuleResolver, Signed, SignedLogEntryV0,
    TransitionV0, Verdict,
};
use icn_authz_store::{
    DurabilityBarrier, LocalReservationPolicyV0, LocalResourceReservations, ReservationError,
    ReservationReconciliation, ReservationRequestV0, ResourcePoolV0,
};
use icn_governance::authz_threshold::{GovernanceActV0, Ratio, ThresholdRuleV0};
use icn_governance::domain::GovernanceDomainId;
use icn_governance::subject_recognition::{RecognitionBasisKind, SubjectRecognitionV1};
use icn_governance::vote::VoteChoice;
use icn_identity::authority_log::{
    AuthorityBody, AuthorityStore, ContinuityRoot, DeviceCapability, EstablishmentKind,
    PrincipalKey, SignedAuthorityEvent, SubjectId, WitnessSignature,
};
use icn_identity::device_authority::{sign_device_act, DeviceActV1};
use icn_identity::evidence_bundle::EvidenceBundle;
use icn_identity::subject_context::{
    incept_subject_context_v1, verify_subject_context_genesis_v1, ContextSalt,
    SubjectContextDescriptor, SubjectContextGenesisV1, SubjectContextKind,
    SUBJECT_CONTEXT_GENESIS_VERSION,
};
use icn_kernel_api::canonical::{CanonicalHasher, DomainTag};
use icn_store::SledStore;

/// The synthetic rehearsal institution's governance-domain id.
const DOMAIN: &str = "rehearsal.synthetic-coop.example";
/// Another domain, for foreign-Subject and wrong-binding cases.
const FOREIGN_DOMAIN: &str = "rehearsal.other-coop.example";
const ROLE: &str = "member";
const NOW: u64 = 1_000;
const NOT_AFTER: u64 = 1_500;
const OP: [u8; 32] = [0x0a; 32];
const POOL: [u8; 32] = [0x71; 32];
const POLICY: [u8; 32] = [0x72; 32];

/// HARNESS-LOCAL convention, not a format: the authz context's `label_hash` commits to the
/// governance domain whose GEN-B recognitions populate its electorate.
fn harness_domain_binding(domain: &str) -> [u8; 32] {
    let mut h = CanonicalHasher::new(DomainTag::new(
        b"icn:test:harness:authz-context-domain-binding:v0",
    ));
    h.str(domain);
    h.finish()
}

fn key(seed: u8) -> SigningKey {
    SigningKey::from_bytes(&[seed; 32])
}

fn pk(k: &SigningKey) -> PrincipalKey {
    PrincipalKey::try_from_verifying_key(k.verifying_key()).unwrap()
}

// ---------------------------------------------------------------------------------------------
// GEN-A Subject with its own device
// ---------------------------------------------------------------------------------------------

/// A fresh GEN-A context-scoped Subject. The device generated its own key; the Subject's
/// generation-0 authority signed the authorize event. No legacy `Did` appears anywhere.
struct GenASubject {
    genesis: SubjectContextGenesisV1,
    subject: SubjectId,
    device_key: SigningKey,
    events: Vec<SignedAuthorityEvent>,
}

impl GenASubject {
    fn incept(domain: &str, seed: u8) -> Self {
        let descriptor = SubjectContextDescriptor::governance_domain_v1(
            domain,
            ContextSalt::from_bytes([seed ^ 0x5a; 32]),
        )
        .unwrap();
        let root = ContinuityRoot::with_plan(
            [seed; 32],
            descriptor.context_nonce(),
            vec![EstablishmentKind::Rotate; 2],
        );
        let device_key = key(seed.wrapping_add(0x40));
        let genesis = incept_subject_context_v1(&descriptor, &root, pk(&device_key)).unwrap();
        // GEN-A verification derives the Subject and device; nothing is asserted by the bundle.
        let verified = verify_subject_context_genesis_v1(
            &genesis,
            SubjectContextKind::GovernanceDomainV1,
            domain,
        )
        .unwrap();
        assert_eq!(verified.device, pk(&device_key));
        // The two N1 events the genesis carries, rebuilt from their canonical bytes.
        let events = vec![
            SignedAuthorityEvent::new(
                AuthorityBody::decode(&genesis.inception_body_bytes).unwrap(),
                genesis.inception_witness,
            ),
            SignedAuthorityEvent::new(
                AuthorityBody::decode(&genesis.authorize_body_bytes).unwrap(),
                genesis.authorize_witness,
            ),
        ];
        GenASubject {
            subject: verified.subject,
            genesis,
            device_key,
            events,
        }
    }

    /// A Subject-signed governance act (N4 device act over a `GovernanceActV0`), as an evidence
    /// bundle assembled from the Subject's own N1 history.
    fn act(&self, ctx: ContextRef, matter: [u8; 32], choice: VoteChoice) -> EvidenceBundle {
        let payload = GovernanceActV0 {
            context: ctx.0,
            proposal: matter,
            choice,
            act_nonce: [0x77; 32],
        }
        .canonical_bytes();
        let act = DeviceActV1::new(
            self.subject,
            pk(&self.device_key),
            DeviceCapability::Sign,
            1,
            payload,
        )
        .unwrap();
        let signed = sign_device_act(&act, &self.device_key).unwrap();
        let mut store = AuthorityStore::new();
        store.ingest_all(self.events.iter());
        EvidenceBundle::assemble(&store, &signed).unwrap()
    }

    fn parts(&self) -> GenesisParts {
        GenesisParts::of(&self.genesis)
    }
}

// ---------------------------------------------------------------------------------------------
// The synthetic institution's authority context
// ---------------------------------------------------------------------------------------------

struct Rules(Vec<ThresholdRuleV0>);

impl RuleResolver for Rules {
    fn resolve(&self, rule_hash: &[u8; 32]) -> Option<&dyn AdoptedRule> {
        self.0
            .iter()
            .find(|r| &r.rule_hash() == rule_hash)
            .map(|r| r as &dyn AdoptedRule)
    }
}

/// The adopted rule for both slots: 2/3 quorum, 2/3 approval over Subjects holding `ROLE`.
fn strict() -> ThresholdRuleV0 {
    ThresholdRuleV0 {
        role: ROLE.into(),
        quorum: Ratio { num: 2, den: 3 },
        approval: Ratio { num: 2, den: 3 },
    }
}

fn rules() -> Rules {
    Rules(vec![strict()])
}

struct Institution {
    founders: [SigningKey; 3],
    inception: ContextInceptionV0,
    ctx: ContextRef,
    writer: SigningKey,
    members: Vec<GenASubject>,
    custodian: [u8; 32],
}

impl Institution {
    /// Three founder stewards (2-of-3), a writer, three GEN-A members of `DOMAIN`, and a context
    /// whose label carries the harness binding to `bound_domain`.
    fn new(bound_domain: &str) -> Self {
        let founders = [key(0xF1), key(0xF2), key(0xF3)];
        let inception = ContextInceptionV0::new(
            founders.iter().map(pk).collect(),
            2,
            harness_domain_binding(bound_domain),
        )
        .unwrap();
        Institution {
            ctx: inception.context_ref(),
            inception,
            founders,
            writer: key(0xA1),
            members: vec![
                GenASubject::incept(DOMAIN, 0x01),
                GenASubject::incept(DOMAIN, 0x02),
                GenASubject::incept(DOMAIN, 0x03),
            ],
            custodian: icn_authz::effect_v0::target_id(&pk(&key(0xC1))),
        }
    }

    fn m(&self, i: usize) -> &GenASubject {
        &self.members[i]
    }

    fn genesis_entry(&self) -> SignedLogEntryV0 {
        let mut standing: Vec<(SubjectId, String)> = self
            .members
            .iter()
            .map(|m| (m.subject, ROLE.to_string()))
            .collect();
        standing.sort();
        let body = LogEntryV0 {
            context: self.ctx,
            seq: 1,
            prev: self.ctx.0,
            transition: TransitionV0::Genesis {
                writer: pk(&self.writer),
                transition_rule: strict().rule_hash(),
                effect_rule: strict().rule_hash(),
                standing,
            },
        };
        SignedLogEntryV0::genesis(body, &[&self.founders[0], &self.founders[1]]).unwrap()
    }

    fn tip(&self, log: &[SignedLogEntryV0]) -> LogCheckpointV0 {
        replay_context_log(&self.inception, log, &rules())
            .unwrap()
            .tip()
            .checkpoint
    }

    /// The next entry, decided by For acts from `voters` and sequenced by the writer.
    fn decided(
        &self,
        log: &[SignedLogEntryV0],
        transition: TransitionV0,
        voters: &[&GenASubject],
    ) -> SignedLogEntryV0 {
        let tip = self.tip(log);
        let body = LogEntryV0 {
            context: self.ctx,
            seq: tip.seq + 1,
            prev: tip.head,
            transition,
        };
        let matter = transition_subject_matter(&body);
        let votes = voters
            .iter()
            .map(|m| m.act(self.ctx, matter, VoteChoice::For))
            .collect();
        SignedLogEntryV0::decided(body, &self.writer, votes).unwrap()
    }

    fn request(&self, op: [u8; 32], quantity: u64) -> ReservationRequestV0 {
        ReservationRequestV0 {
            context: self.ctx,
            operation_id: op,
            pool_id: POOL,
            expected_pool_revision: 0,
            quantity,
            unit: "cpu-hours".into(),
            request_params_hash: [0x73; 32],
            policy_hash: POLICY,
        }
    }

    fn pool(&self, capacity: u64) -> ResourcePoolV0 {
        ResourcePoolV0 {
            context: self.ctx,
            pool_id: POOL,
            custodian: self.custodian,
            unit: "cpu-hours".into(),
            total_capacity: capacity,
            available_capacity: capacity,
            revision: 0,
            policy: LocalReservationPolicyV0 {
                enabled: true,
                max_quantity_per_reservation: capacity,
                policy_hash: POLICY,
            },
        }
    }

    /// A checkpointed authorization of exactly `effect` under `op`, at `checkpoint`, with For acts
    /// from `voters` and a Pass verdict the writer signs (and every verifier re-derives).
    fn authorize(
        &self,
        log: &[SignedLogEntryV0],
        checkpoint: LogCheckpointV0,
        voters: &[&GenASubject],
        effect: EffectV0,
        op: [u8; 32],
    ) -> CheckpointedEvidenceV0 {
        let matter = effect.commitment(self.ctx, op);
        let mut vote_bundles: Vec<EvidenceBundle> = voters
            .iter()
            .map(|m| m.act(self.ctx, matter, VoteChoice::For))
            .collect();
        vote_bundles.sort_by_key(|b| b.act().act.act_id());
        let inputs = vote_bundles
            .iter()
            .map(|b| InputActV0 {
                act_id: b.act().act.act_id(),
                subject: b.subject(),
                position: b.evaluation_position(),
            })
            .collect();
        let body = CheckpointedAuthorizationV0 {
            context: self.ctx,
            checkpoint,
            subject_matter: matter,
            rule_hash: strict().rule_hash(),
            verdict: Verdict::Pass,
            inputs,
            effect,
            operation_id: op,
            not_after: NOT_AFTER,
        };
        CheckpointedEvidenceV0 {
            inception: self.inception.clone(),
            log: log.to_vec(),
            authorization: sign_checkpointed_authorization(body, &self.writer),
            vote_bundles,
        }
    }

    fn reservation_evidence(
        &self,
        log: &[SignedLogEntryV0],
        voters: &[&GenASubject],
        request: &ReservationRequestV0,
    ) -> CheckpointedEvidenceV0 {
        self.authorize(
            log,
            self.tip(log),
            voters,
            request.effect(self.custodian).unwrap(),
            request.operation_id,
        )
    }

    fn frontier(&self, min_seq: u64, pinned: Option<LogCheckpointV0>) -> RelyingFrontierV0 {
        RelyingFrontierV0 {
            context: self.ctx,
            min_seq,
            pinned,
        }
    }

    /// The relying party's out-of-band trust for this institution.
    fn trust(&self, min_seq: u64, pinned: Option<LogCheckpointV0>) -> VerifierTrust {
        VerifierTrust {
            domain: GovernanceDomainId(DOMAIN.into()),
            context: self.ctx,
            frontier: self.frontier(min_seq, pinned),
        }
    }

    fn parts(&self) -> Vec<GenesisParts> {
        self.members.iter().map(GenASubject::parts).collect()
    }
}

/// The honest lifecycle up to a granted fourth member: genesis (3 founders' members), then a
/// fourth GEN-A Subject granted standing by a 2-of-3 Subject decision.
fn honest_history(inst: &Institution) -> (Vec<SignedLogEntryV0>, GenASubject) {
    let fourth = GenASubject::incept(DOMAIN, 0x04);
    let mut log = vec![inst.genesis_entry()];
    log.push(inst.decided(
        &log,
        TransitionV0::GrantStanding {
            subject: fourth.subject,
            role: ROLE.into(),
        },
        &[inst.m(0), inst.m(1)],
    ));
    (log, fourth)
}

// ---------------------------------------------------------------------------------------------
// Independent verifier: holds bytes and out-of-band trust, never the producer's verdict or keys
// ---------------------------------------------------------------------------------------------

/// HARNESS transport of a GEN-A genesis bundle's public fields. GEN-A v1 has no wire codec, so a
/// cross-process verifier cannot receive recognition evidence in a canonical form today.
#[derive(Clone)]
struct GenesisParts {
    context_id: String,
    salt: [u8; 32],
    inception_body: Vec<u8>,
    inception_witness: [u8; 64],
    authorize_body: Vec<u8>,
    authorize_witness: [u8; 64],
}

impl GenesisParts {
    fn of(g: &SubjectContextGenesisV1) -> Self {
        GenesisParts {
            context_id: g.context_id.clone(),
            salt: *g.context_salt.as_bytes(),
            inception_body: g.inception_body_bytes.clone(),
            inception_witness: *g.inception_witness.as_bytes(),
            authorize_body: g.authorize_body_bytes.clone(),
            authorize_witness: *g.authorize_witness.as_bytes(),
        }
    }

    fn genesis(&self) -> SubjectContextGenesisV1 {
        SubjectContextGenesisV1 {
            version: SUBJECT_CONTEXT_GENESIS_VERSION,
            context_kind: SubjectContextKind::GovernanceDomainV1,
            context_id: self.context_id.clone(),
            context_salt: ContextSalt::from_bytes(self.salt),
            inception_body_bytes: self.inception_body.clone(),
            inception_witness: WitnessSignature::from_bytes(self.inception_witness),
            authorize_body_bytes: self.authorize_body.clone(),
            authorize_witness: WitnessSignature::from_bytes(self.authorize_witness),
        }
    }
}

/// Everything the verifier receives, as bytes.
#[derive(Clone)]
struct EvidencePackage {
    inception: Vec<u8>,
    log: Vec<Vec<u8>>,
    authorization: Vec<u8>,
    vote_bundles: Vec<Vec<u8>>,
    recognition_geneses: Vec<GenesisParts>,
}

impl EvidencePackage {
    fn of(ev: &CheckpointedEvidenceV0, geneses: Vec<GenesisParts>) -> Self {
        EvidencePackage {
            inception: ev.inception.canonical_bytes(),
            log: ev.log.iter().map(SignedLogEntryV0::encode).collect(),
            authorization: ev.authorization.encode(),
            vote_bundles: ev
                .vote_bundles
                .iter()
                .map(EvidenceBundle::canonical_bytes)
                .collect(),
            recognition_geneses: geneses,
        }
    }
}

/// What the relying party trusts out of band: which domain it serves, which authz context it
/// accepts for that domain, and its own frontier (never the presenter's claim of currency).
struct VerifierTrust {
    domain: GovernanceDomainId,
    context: ContextRef,
    frontier: RelyingFrontierV0,
}

#[derive(Debug, PartialEq, Eq)]
enum IndependentRefusal {
    Decode(&'static str),
    UntrustedContext,
    DomainBindingMismatch,
    Log(LogRefusal),
    /// A Subject in the electorate at the checkpoint has no rehearsal recognition by the domain.
    UnrecognizedStanding(SubjectId),
}

/// What the independent verifier concludes. `rehearsal_only` is always true: the only recognition
/// basis that exists is non-production.
#[derive(Debug, PartialEq, Eq)]
struct IndependentVerdict {
    context: ContextRef,
    operation_id: [u8; 32],
    effect: EffectV0,
    checkpoint: LogCheckpointV0,
    electorate: usize,
    rehearsal_only: bool,
}

fn independent_verify(
    pkg: &EvidencePackage,
    trust: &VerifierTrust,
) -> Result<IndependentVerdict, IndependentRefusal> {
    let inception = ContextInceptionV0::decode(&pkg.inception)
        .map_err(|_| IndependentRefusal::Decode("inception"))?;
    if inception.context_ref() != trust.context {
        return Err(IndependentRefusal::UntrustedContext);
    }
    if inception.label_hash != harness_domain_binding(&trust.domain.0) {
        return Err(IndependentRefusal::DomainBindingMismatch);
    }
    let log = pkg
        .log
        .iter()
        .map(|b| SignedLogEntryV0::decode(b))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| IndependentRefusal::Decode("log"))?;
    let authorization = Signed::<CheckpointedAuthorizationV0>::decode(&pkg.authorization)
        .map_err(|_| IndependentRefusal::Decode("authorization"))?;
    let vote_bundles = pkg
        .vote_bundles
        .iter()
        .map(|b| EvidenceBundle::decode(b))
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| IndependentRefusal::Decode("vote bundle"))?;
    let evidence = CheckpointedEvidenceV0 {
        inception: inception.clone(),
        log: log.clone(),
        authorization,
        vote_bundles,
    };
    // Re-derive the verdict from the log, the rule and the N4 acts. Nothing the writer asserts
    // about the electorate, rule or verdict is taken on its word.
    let verified = icn_authz::effect_v0::verify_checkpointed_authorization(
        &evidence,
        &trust.frontier,
        &rules(),
    )
    .map_err(IndependentRefusal::Log)?;
    let auth = verified.authorization().clone();

    // Recognition gate (HARNESS): every Subject in the electorate at the checkpoint must have a
    // GEN-B rehearsal recognition by the trusted domain, re-derived from its GEN-A genesis.
    let replayed =
        replay_context_log(&inception, &log, &rules()).map_err(IndependentRefusal::Log)?;
    let state = replayed
        .state_at(auth.checkpoint.seq)
        .ok_or(IndependentRefusal::Decode("checkpoint state"))?;
    let electorate = state.eligible(ROLE);
    for subject in &electorate {
        let recognized = pkg.recognition_geneses.iter().any(|parts| {
            SubjectRecognitionV1::recognize_bootstrap_rehearsal(&trust.domain, &parts.genesis())
                .map(|r| {
                    r.subject() == subject
                        && r.basis_kind() == RecognitionBasisKind::BootstrapRehearsalV1
                })
                .unwrap_or(false)
        });
        if !recognized {
            return Err(IndependentRefusal::UnrecognizedStanding(*subject));
        }
    }
    Ok(IndependentVerdict {
        context: auth.context,
        operation_id: auth.operation_id,
        effect: auth.effect.clone(),
        checkpoint: auth.checkpoint,
        electorate: electorate.len(),
        rehearsal_only: true,
    })
}

struct FailingBarrier;

impl DurabilityBarrier for FailingBarrier {
    fn sync(&self, _: &SledStore) -> Result<(), String> {
        Err("injected barrier failure".into())
    }
}

fn available(r: &LocalResourceReservations<impl DurabilityBarrier>, ctx: ContextRef) -> u64 {
    r.inspect_pool(ctx, POOL)
        .unwrap()
        .unwrap()
        .available_capacity
}

// ---------------------------------------------------------------------------------------------
// The lifecycle
// ---------------------------------------------------------------------------------------------

/// The whole chain once, with an independent verifier and a restart.
#[test]
fn lifecycle_gen_a_to_durable_reservation_survives_restart() {
    let inst = Institution::new(DOMAIN);

    // GEN-B: each member's rehearsal recognition is derivable from its GEN-A genesis, and only
    // for the domain that genesis was incepted for.
    for m in &inst.members {
        let r = SubjectRecognitionV1::recognize_bootstrap_rehearsal(
            &GovernanceDomainId(DOMAIN.into()),
            &m.genesis,
        )
        .unwrap();
        assert_eq!(r.subject(), &m.subject);
        assert_eq!(r.basis_kind(), RecognitionBasisKind::BootstrapRehearsalV1);
        assert!(SubjectRecognitionV1::recognize_bootstrap_rehearsal(
            &GovernanceDomainId(FOREIGN_DOMAIN.into()),
            &m.genesis,
        )
        .is_err());
    }

    // Separately authorized standing: the fourth member is granted by a Subject decision.
    let (log, fourth) = honest_history(&inst);
    let state = replay_context_log(&inst.inception, &log, &rules()).unwrap();
    assert_eq!(state.tip().eligible(ROLE).len(), 4);
    assert!(state.tip().eligible(ROLE).contains(&fourth.subject));

    // Four in the electorate: three For acts meet 2/3 quorum and approval.
    let request = inst.request(OP, 4);
    let evidence = inst.reservation_evidence(&log, &[inst.m(0), inst.m(1), &fourth], &request);
    let mut geneses = inst.parts();
    geneses.push(fourth.parts());
    let package = EvidencePackage::of(&evidence, geneses);

    // The independent verifier sees bytes and its own trust; it re-derives everything.
    let verdict = independent_verify(&package, &inst.trust(2, None)).unwrap();
    assert_eq!(verdict.context, inst.ctx);
    assert_eq!(verdict.operation_id, OP);
    assert_eq!(verdict.effect, request.effect(inst.custodian).unwrap());
    assert_eq!(verdict.checkpoint.seq, 2);
    assert_eq!(verdict.electorate, 4);
    assert!(verdict.rehearsal_only);

    // Target-side admission + atomic local reservation, independently re-verified by the target.
    let dir = tempfile::tempdir().unwrap();
    {
        let target = LocalResourceReservations::open(dir.path(), inst.custodian).unwrap();
        target.initialize_pool(inst.pool(10)).unwrap();
        let record = target
            .reserve_checkpointed(&evidence, &inst.frontier(2, None), &rules(), NOW, &request)
            .unwrap();
        assert_eq!((record.available_before, record.available_after), (10, 6));
    }

    // Restart: a new process opens the same store.
    let target = LocalResourceReservations::open(dir.path(), inst.custodian).unwrap();
    assert!(matches!(
        target.reconcile(inst.ctx, OP).unwrap(),
        ReservationReconciliation::Committed { .. }
    ));
    assert_eq!(available(&target, inst.ctx), 6);
    // Replay after restart is refused; capacity is untouched.
    assert_eq!(
        target.reserve_checkpointed(&evidence, &inst.frontier(2, None), &rules(), NOW, &request),
        Err(ReservationError::Authorization(AdmissionRefusal::Replayed))
    );
    assert_eq!(available(&target, inst.ctx), 6);
    // The evidence is still historically valid to the independent verifier: verification and
    // consumption are different facts, owned by different parties.
    assert!(independent_verify(&package, &inst.trust(2, None)).is_ok());
}

// ---------------------------------------------------------------------------------------------
// Unauthorized standing
// ---------------------------------------------------------------------------------------------

/// The seam this harness exists to find: the log admits a standing grant for any `SubjectId`
/// that a Subject decision names. It does not know about GEN-A context or GEN-B recognition. Only
/// the (harness) recognition gate refuses it.
#[test]
fn the_log_alone_accepts_standing_for_a_foreign_or_unrecognized_subject() {
    let inst = Institution::new(DOMAIN);
    let foreign = GenASubject::incept(FOREIGN_DOMAIN, 0x05);
    let mut log = vec![inst.genesis_entry()];
    log.push(inst.decided(
        &log,
        TransitionV0::GrantStanding {
            subject: foreign.subject,
            role: ROLE.into(),
        },
        &[inst.m(0), inst.m(1)],
    ));
    // The authz layer accepts the grant.
    let state = replay_context_log(&inst.inception, &log, &rules()).unwrap();
    assert!(state.tip().eligible(ROLE).contains(&foreign.subject));

    let request = inst.request(OP, 1);
    let evidence = inst.reservation_evidence(&log, &[inst.m(0), inst.m(1), inst.m(2)], &request);

    // Foreign genesis supplied: recognition for DOMAIN is not derivable from it.
    let mut with_foreign = inst.parts();
    with_foreign.push(foreign.parts());
    assert_eq!(
        independent_verify(
            &EvidencePackage::of(&evidence, with_foreign),
            &inst.trust(2, None)
        ),
        Err(IndependentRefusal::UnrecognizedStanding(foreign.subject))
    );
    // No genesis supplied at all.
    assert_eq!(
        independent_verify(
            &EvidencePackage::of(&evidence, inst.parts()),
            &inst.trust(2, None)
        ),
        Err(IndependentRefusal::UnrecognizedStanding(foreign.subject))
    );

    // The target, which has no recognition gate, admits it: the gap is real at the target.
    let dir = tempfile::tempdir().unwrap();
    let target = LocalResourceReservations::open(dir.path(), inst.custodian).unwrap();
    target.initialize_pool(inst.pool(10)).unwrap();
    assert!(target
        .reserve_checkpointed(&evidence, &inst.frontier(2, None), &rules(), NOW, &request)
        .is_ok());
}

/// A writer cannot grant standing on its own word: a decision with no Subject acts is refused by
/// replay, so nothing downstream ever sees the grant.
#[test]
fn writer_alone_cannot_grant_standing() {
    let inst = Institution::new(DOMAIN);
    let intruder = GenASubject::incept(DOMAIN, 0x06);
    let log = vec![inst.genesis_entry()];
    let forged = inst.decided(
        &log,
        TransitionV0::GrantStanding {
            subject: intruder.subject,
            role: ROLE.into(),
        },
        &[],
    );
    let mut bad = log.clone();
    bad.push(forged);
    assert!(replay_context_log(&inst.inception, &bad, &rules()).is_err());
}

// ---------------------------------------------------------------------------------------------
// Wrong context, stale checkpoint, conflicting histories
// ---------------------------------------------------------------------------------------------

#[test]
fn wrong_context_or_wrong_domain_binding_is_refused() {
    let inst = Institution::new(DOMAIN);
    let (log, fourth) = honest_history(&inst);
    let request = inst.request(OP, 4);
    let evidence = inst.reservation_evidence(&log, &[inst.m(0), inst.m(1), &fourth], &request);
    let package = EvidencePackage::of(&evidence, inst.parts());

    // A verifier serving a different authz context.
    let other = Institution::new(FOREIGN_DOMAIN);
    let mut trust = inst.trust(2, None);
    trust.context = other.ctx;
    assert_eq!(
        independent_verify(&package, &trust),
        Err(IndependentRefusal::UntrustedContext)
    );

    // A context whose label binds another domain, trusted as if it served DOMAIN.
    let (olog, ofourth) = honest_history(&other);
    let oreq = other.request(OP, 4);
    let oev = other.reservation_evidence(&olog, &[other.m(0), other.m(1), &ofourth], &oreq);
    assert_eq!(
        independent_verify(
            &EvidencePackage::of(&oev, other.parts()),
            &VerifierTrust {
                domain: GovernanceDomainId(DOMAIN.into()),
                context: other.ctx,
                frontier: other.frontier(2, None),
            }
        ),
        Err(IndependentRefusal::DomainBindingMismatch)
    );

    // The target's pool lives in `inst.ctx`; the other context's evidence names `other.ctx`.
    let dir = tempfile::tempdir().unwrap();
    let target = LocalResourceReservations::open(dir.path(), inst.custodian).unwrap();
    target.initialize_pool(inst.pool(10)).unwrap();
    assert_eq!(
        target.reserve_checkpointed(&oev, &inst.frontier(2, None), &rules(), NOW, &request),
        Err(ReservationError::Authorization(
            AdmissionRefusal::UntrustedContext
        ))
    );
    assert_eq!(target.inspect_consumed(inst.ctx, OP).unwrap(), None);
    assert_eq!(available(&target, inst.ctx), 10);
}

/// An authorization taken at genesis is stale for a party whose own frontier is at seq 2 (it
/// knows the electorate changed). Neither the verifier nor the target consumes anything.
#[test]
fn stale_checkpoint_is_refused_by_verifier_and_target() {
    let inst = Institution::new(DOMAIN);
    let (log, _fourth) = honest_history(&inst);
    let genesis_cp = inst.tip(&log[..1]);
    let request = inst.request(OP, 4);
    let stale = inst.authorize(
        &log,
        genesis_cp,
        &[inst.m(0), inst.m(1)],
        request.effect(inst.custodian).unwrap(),
        OP,
    );
    let refusal = LogRefusal::CheckpointBelowFrontier {
        checkpoint: 1,
        frontier: 2,
    };
    assert_eq!(
        independent_verify(
            &EvidencePackage::of(&stale, inst.parts()),
            &inst.trust(2, None)
        ),
        Err(IndependentRefusal::Log(refusal.clone()))
    );
    let dir = tempfile::tempdir().unwrap();
    let target = LocalResourceReservations::open(dir.path(), inst.custodian).unwrap();
    target.initialize_pool(inst.pool(10)).unwrap();
    assert_eq!(
        target.reserve_checkpointed(&stale, &inst.frontier(2, None), &rules(), NOW, &request),
        Err(ReservationError::Authorization(AdmissionRefusal::Log(
            refusal
        )))
    );
    assert_eq!(target.inspect_consumed(inst.ctx, OP).unwrap(), None);
    assert_eq!(available(&target, inst.ctx), 10);
}

/// Two histories that diverge at seq 2 yield an attributable fork proof; a party pinned to one
/// branch refuses evidence from the other. An unpinned party at the lower frontier cannot tell
/// (pinned by `known_limit_unpinned_party_accepts_either_branch` in `context_log.rs`).
#[test]
fn conflicting_histories_yield_a_fork_proof_and_the_pin_decides() {
    let inst = Institution::new(DOMAIN);
    let (a, fourth) = honest_history(&inst);
    let mut b = vec![inst.genesis_entry()];
    b.push(inst.decided(
        &b,
        TransitionV0::RevokeStanding {
            subject: inst.m(2).subject,
            role: ROLE.into(),
        },
        &[inst.m(0), inst.m(1)],
    ));
    assert!(find_log_fork(&inst.inception, &a, &b).is_some());

    let request = inst.request(OP, 4);
    let on_b = inst.reservation_evidence(&b, &[inst.m(0), inst.m(1)], &request);
    let pin_a = inst.tip(&a);
    assert_eq!(
        independent_verify(
            &EvidencePackage::of(&on_b, inst.parts()),
            &inst.trust(1, Some(pin_a))
        ),
        Err(IndependentRefusal::Log(
            LogRefusal::PinnedCheckpointMismatch { seq: 2 }
        ))
    );
    let dir = tempfile::tempdir().unwrap();
    let target = LocalResourceReservations::open(dir.path(), inst.custodian).unwrap();
    target.initialize_pool(inst.pool(10)).unwrap();
    assert!(matches!(
        target.reserve_checkpointed(
            &on_b,
            &inst.frontier(1, Some(pin_a)),
            &rules(),
            NOW,
            &request
        ),
        Err(ReservationError::Authorization(AdmissionRefusal::Log(
            LogRefusal::PinnedCheckpointMismatch { seq: 2 }
        )))
    ));
    assert_eq!(available(&target, inst.ctx), 10);
    // The branch the target is pinned to still works.
    let on_a = inst.reservation_evidence(&a, &[inst.m(0), inst.m(1), &fourth], &request);
    assert!(target
        .reserve_checkpointed(
            &on_a,
            &inst.frontier(1, Some(pin_a)),
            &rules(),
            NOW,
            &request
        )
        .is_ok());
}

// ---------------------------------------------------------------------------------------------
// Repeated operation ids, storage failure, restart
// ---------------------------------------------------------------------------------------------

/// A second, different authorization for the same operation id is refused after a restart as
/// attributable equivocation; capacity is decremented once.
#[test]
fn repeated_operation_id_allocates_once_across_restart() {
    let inst = Institution::new(DOMAIN);
    let (log, fourth) = honest_history(&inst);
    let voters = [inst.m(0), inst.m(1), &fourth];
    let first_req = inst.request(OP, 4);
    let first = inst.reservation_evidence(&log, &voters, &first_req);
    let dir = tempfile::tempdir().unwrap();
    {
        let target = LocalResourceReservations::open(dir.path(), inst.custodian).unwrap();
        target.initialize_pool(inst.pool(10)).unwrap();
        target
            .reserve_checkpointed(&first, &inst.frontier(2, None), &rules(), NOW, &first_req)
            .unwrap();
    }
    let target = LocalResourceReservations::open(dir.path(), inst.custodian).unwrap();
    // Same op id, different quantity (a different effect), expecting the post-commit revision.
    let mut second_req = inst.request(OP, 5);
    second_req.expected_pool_revision = 1;
    let second = inst.reservation_evidence(&log, &voters, &second_req);
    assert!(matches!(
        target.reserve_checkpointed(&second, &inst.frontier(2, None), &rules(), NOW, &second_req),
        Err(ReservationError::Authorization(
            AdmissionRefusal::Equivocation(_)
        ))
    ));
    assert_eq!(available(&target, inst.ctx), 6);
}

/// The commit's durability barrier fails: the target reports uncertainty, not success or
/// failure. After a restart, reconciliation finds the committed reservation and a retry is a
/// replay — no second allocation.
#[test]
fn storage_failure_is_uncertain_and_reconciles_once_after_restart() {
    let inst = Institution::new(DOMAIN);
    let (log, fourth) = honest_history(&inst);
    let request = inst.request(OP, 4);
    let evidence = inst.reservation_evidence(&log, &[inst.m(0), inst.m(1), &fourth], &request);
    let dir = tempfile::tempdir().unwrap();
    LocalResourceReservations::open(dir.path(), inst.custodian)
        .unwrap()
        .initialize_pool(inst.pool(10))
        .unwrap();
    {
        let failing = LocalResourceReservations::with_barrier(
            SledStore::open(dir.path()).unwrap(),
            inst.custodian,
            FailingBarrier,
        );
        assert_eq!(
            failing.reserve_checkpointed(
                &evidence,
                &inst.frontier(2, None),
                &rules(),
                NOW,
                &request
            ),
            Err(ReservationError::CommitUncertain(
                "injected barrier failure".into()
            ))
        );
    }
    let target = LocalResourceReservations::open(dir.path(), inst.custodian).unwrap();
    assert!(matches!(
        target.reconcile(inst.ctx, OP).unwrap(),
        ReservationReconciliation::Committed { .. }
    ));
    assert_eq!(
        target.reserve_checkpointed(&evidence, &inst.frontier(2, None), &rules(), NOW, &request),
        Err(ReservationError::Authorization(AdmissionRefusal::Replayed))
    );
    assert_eq!(available(&target, inst.ctx), 6);
}
