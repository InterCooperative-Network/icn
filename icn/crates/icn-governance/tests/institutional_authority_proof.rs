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
//! - The binding between the authz `ContextRef` and the GEN-A/GEN-B `GovernanceDomainId` is the
//!   relying party's own [`InstitutionalTrustAnchorV0`] (`icn_governance::institutional_admission`),
//!   built here from test constants. It is trusted configuration, not a format: nothing reads the
//!   inception's `label_hash` (the harness still sets one so the two synthetic contexts differ).
//!   No log transition carries recognition evidence; the log-only gap is pinned by
//!   `the_log_alone_accepts_standing_for_a_foreign_or_unrecognized_subject`, and the
//!   institutional gate that closes it is exercised in the "Institutional admission" section.
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
    AdmissionGradeV0, AdmissionPolicyRefV0, AdmissionRegistryV0, AdmissionRequirementV1,
    CustodianClockV0, DurabilityBarrier, LocalReservationPolicyV0, LocalResourceReservations,
    ReservationError, ReservationReconciliation, ReservationRequestV0, ResourcePoolRecord,
    ResourcePoolV0, ResourcePoolV1,
};
use icn_governance::authz_threshold::{GovernanceActV0, Ratio, ThresholdRuleV0};
use icn_governance::domain::GovernanceDomainId;
use icn_governance::institutional_admission::{
    admit_institutional, verify_institutional_authorization, InstitutionalAdmissionError,
    InstitutionalAdmissionVerifierV0, InstitutionalEvidenceV0, InstitutionalRefusal,
    InstitutionalSupplementV0, InstitutionalTrustAnchorV0, Legitimacy, RecognitionRequirement,
};
use icn_governance::subject_recognition::RecognitionError;
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

/// An uninterpreted label so contexts founded for different synthetic domains differ. Nothing
/// verifies it; the domain/context binding is the relying party's [`InstitutionalTrustAnchorV0`].
fn harness_context_label(domain: &str) -> [u8; 32] {
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
            harness_context_label(bound_domain),
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

    /// The relying party's out-of-band trust: `DOMAIN` is served by this context, at this
    /// frontier, under the strict rule, rehearsal recognition permitted.
    fn trust(&self, min_seq: u64, pinned: Option<LogCheckpointV0>) -> InstitutionalTrustAnchorV0 {
        anchor(
            DOMAIN,
            self.ctx,
            min_seq,
            pinned,
            RecognitionRequirement::RehearsalPermitted,
        )
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

/// The relying party's anchor, from its own configuration.
fn anchor(
    domain: &str,
    context: ContextRef,
    min_seq: u64,
    pinned: Option<LogCheckpointV0>,
    recognition: RecognitionRequirement,
) -> InstitutionalTrustAnchorV0 {
    InstitutionalTrustAnchorV0::new(
        GovernanceDomainId(domain.into()),
        context,
        min_seq,
        pinned,
        vec![strict()],
        recognition,
    )
    .unwrap()
}

#[derive(Debug, PartialEq, Eq)]
enum IndependentRefusal {
    Decode(&'static str),
    /// The library gate (`verify_institutional_authorization`) refused.
    Institutional(InstitutionalRefusal),
}

/// What the independent verifier concludes. The legitimacy is always `RehearsalOnly`: the only
/// recognition basis that exists is non-production.
#[derive(Debug, PartialEq, Eq)]
struct IndependentVerdict {
    context: ContextRef,
    operation_id: [u8; 32],
    effect: EffectV0,
    checkpoint: LogCheckpointV0,
    voters: usize,
    legitimacy: Legitimacy,
}

/// Decode bytes, then run the library institutional verifier under the relying party's anchor.
fn independent_verify(
    pkg: &EvidencePackage,
    trust: &InstitutionalTrustAnchorV0,
) -> Result<IndependentVerdict, IndependentRefusal> {
    let inception = ContextInceptionV0::decode(&pkg.inception)
        .map_err(|_| IndependentRefusal::Decode("inception"))?;
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
    let evidence = InstitutionalEvidenceV0 {
        authorization: CheckpointedEvidenceV0 {
            inception,
            log,
            authorization,
            vote_bundles,
        },
        recognitions: pkg
            .recognition_geneses
            .iter()
            .map(GenesisParts::genesis)
            .collect(),
    };
    // Re-derive everything: log, rule, N4 acts, and GEN-B recognition of the whole electorate for
    // the anchor's domain. Nothing the writer or presenter asserts is taken on its word.
    let verified = verify_institutional_authorization(trust, &evidence)
        .map_err(IndependentRefusal::Institutional)?;
    let auth = verified.verified().authorization();
    Ok(IndependentVerdict {
        context: auth.context,
        operation_id: auth.operation_id,
        effect: auth.effect.clone(),
        checkpoint: auth.checkpoint,
        voters: verified.voters().len(),
        legitimacy: verified.legitimacy(),
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
    assert_eq!(verdict.voters, 3);
    assert_eq!(verdict.legitimacy, Legitimacy::RehearsalOnly);

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
/// the institutional gate (`icn_governance::institutional_admission`) refuses it.
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

    // Foreign genesis supplied: it does not verify for DOMAIN, so the presentation is refused.
    let mut with_foreign = inst.parts();
    with_foreign.push(foreign.parts());
    assert!(matches!(
        independent_verify(
            &EvidencePackage::of(&evidence, with_foreign),
            &inst.trust(2, None)
        ),
        Err(IndependentRefusal::Institutional(
            InstitutionalRefusal::InvalidRecognition {
                index: 3,
                error: RecognitionError::Genesis(_)
            }
        ))
    ));
    // No recognition for it supplied at all: its standing in the electorate at seq 2 is refused.
    assert_eq!(
        independent_verify(
            &EvidencePackage::of(&evidence, inst.parts()),
            &inst.trust(2, None)
        ),
        Err(IndependentRefusal::Institutional(
            InstitutionalRefusal::UnrecognizedStanding {
                at: 2,
                subject: foreign.subject
            }
        ))
    );

    // A V0 (caller-resolved) pool called directly with a domain-blind resolver admits it: that is
    // the generic contract. A pool that declares institutional admission refuses the same call
    // (see `direct_ungated_call_is_refused_for_an_institutional_policy_pool`).
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
    let trust = anchor(
        DOMAIN,
        other.ctx,
        2,
        None,
        RecognitionRequirement::RehearsalPermitted,
    );
    assert_eq!(
        independent_verify(&package, &trust),
        Err(IndependentRefusal::Institutional(
            InstitutionalRefusal::UntrustedContext
        ))
    );

    // A relying party misconfigured to trust another institution's context for DOMAIN. The
    // binding itself is the relying party's, so nothing in the evidence can contradict it; what
    // fails is that the other context's electorate is not recognised by DOMAIN.
    let (olog, ofourth) = honest_history(&other);
    let oreq = other.request(OP, 4);
    let oev = other.reservation_evidence(&olog, &[other.m(0), other.m(1), &ofourth], &oreq);
    let misbound = anchor(
        DOMAIN,
        other.ctx,
        2,
        None,
        RecognitionRequirement::RehearsalPermitted,
    );
    // KNOWN LIMIT: `other`'s members are themselves recognised by DOMAIN (the harness incepts
    // every member for DOMAIN), so the misbound anchor ACCEPTS. Nothing in the evidence can
    // contradict the relying party's binding; its correctness rests on where the anchor came from
    // (the institutional root — an unowned seam). The session-2 harness caught this case only by
    // interpreting `label_hash`, a test convention deliberately not promoted to a format.
    let mut oparts = other.parts();
    oparts.push(ofourth.parts());
    assert!(independent_verify(&EvidencePackage::of(&oev, oparts), &misbound).is_ok());
    // A foreign-domain Subject cannot supply recognition under DOMAIN at all.
    let stranger = GenASubject::incept(FOREIGN_DOMAIN, 0x08);
    assert!(matches!(
        independent_verify(
            &EvidencePackage::of(&oev, vec![stranger.parts()]),
            &misbound
        ),
        Err(IndependentRefusal::Institutional(
            InstitutionalRefusal::InvalidRecognition { index: 0, .. }
        ))
    ));
    // Without recognition items the electorate is refused instead.
    assert!(matches!(
        independent_verify(&EvidencePackage::of(&oev, vec![]), &misbound),
        Err(IndependentRefusal::Institutional(
            InstitutionalRefusal::UnrecognizedStanding { .. }
        ))
    ));

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
        Err(IndependentRefusal::Institutional(
            InstitutionalRefusal::Log(refusal.clone())
        ))
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
        Err(IndependentRefusal::Institutional(
            InstitutionalRefusal::Log(LogRefusal::PinnedCheckpointMismatch { seq: 2 })
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

// ---------------------------------------------------------------------------------------------
// Institutional admission: the library gate in front of the real target
// ---------------------------------------------------------------------------------------------

fn geneses(subjects: &[&GenASubject]) -> Vec<SubjectContextGenesisV1> {
    subjects.iter().map(|s| s.genesis.clone()).collect()
}

fn members_and(inst: &Institution, extra: &[&GenASubject]) -> Vec<SubjectContextGenesisV1> {
    let mut all: Vec<&GenASubject> = inst.members.iter().collect();
    all.extend_from_slice(extra);
    geneses(&all)
}

type GateResult = Result<
    (
        icn_governance::institutional_admission::VerifiedInstitutionalAuthorization,
        icn_authz_store::ReservationRecordV0,
    ),
    InstitutionalAdmissionError<ReservationError>,
>;

/// Admission through the institutional gate. `reached` records whether the target was invoked.
fn reserve_via_gate(
    target: &LocalResourceReservations<impl DurabilityBarrier>,
    trust: &InstitutionalTrustAnchorV0,
    evidence: &InstitutionalEvidenceV0,
    request: &ReservationRequestV0,
    reached: &std::cell::Cell<bool>,
) -> GateResult {
    admit_institutional(trust, evidence, |ev, frontier, rules| {
        reached.set(true);
        // The target receives the anchor's frontier, whatever the presenter would have preferred.
        assert_eq!(*frontier, trust.frontier());
        target.reserve_checkpointed(ev, frontier, rules, NOW, request)
    })
}

fn fresh_target(inst: &Institution) -> (tempfile::TempDir, LocalResourceReservations) {
    let dir = tempfile::tempdir().unwrap();
    let target = LocalResourceReservations::open(dir.path(), inst.custodian).unwrap();
    target.initialize_pool(inst.pool(10)).unwrap();
    (dir, target)
}

/// Nothing consumed, no reservation, capacity unchanged.
fn untouched(target: &LocalResourceReservations, inst: &Institution, op: [u8; 32]) {
    assert_eq!(target.inspect_consumed(inst.ctx, op).unwrap(), None);
    assert!(matches!(
        target.reconcile(inst.ctx, op).unwrap(),
        ReservationReconciliation::Absent
    ));
    assert_eq!(available(target, inst.ctx), 10);
}

// ---------------------------------------------------------------------------------------------
// Mandatory admission at the target (experimental V1 pool + custodian-installed verifier)
// ---------------------------------------------------------------------------------------------

/// The custodian's admission clock in these tests: a fixed reading in unix seconds (Part 8 F1).
struct FixedClock(u64);

impl CustodianClockV0 for FixedClock {
    fn now(&self) -> u64 {
        self.0
    }
}

fn clock_at(now: u64) -> std::sync::Arc<dyn CustodianClockV0> {
    std::sync::Arc::new(FixedClock(now))
}

fn registry_for(anchors: &[&InstitutionalTrustAnchorV0]) -> AdmissionRegistryV0 {
    registry_at(anchors, Some(NOW))
}

/// The custodian's verifiers for `anchors`, with its clock fixed at `now` (or no clock).
fn registry_at(anchors: &[&InstitutionalTrustAnchorV0], now: Option<u64>) -> AdmissionRegistryV0 {
    let mut registry = AdmissionRegistryV0::new();
    for anchor in anchors {
        registry
            .install(std::sync::Arc::new(InstitutionalAdmissionVerifierV0::new(
                (*anchor).clone(),
            )))
            .unwrap();
    }
    if let Some(now) = now {
        registry.install_clock(clock_at(now)).unwrap();
    }
    registry
}

fn institutional_pool(
    inst: &Institution,
    anchor: &InstitutionalTrustAnchorV0,
    min_grade: AdmissionGradeV0,
) -> ResourcePoolV1 {
    ResourcePoolV1 {
        pool: inst.pool(10),
        admission: AdmissionRequirementV1::Policy {
            policy: InstitutionalAdmissionVerifierV0::policy_ref(anchor),
            min_grade,
        },
    }
}

/// The custodian's store: its own verifier installed, and a pool that requires it.
fn policy_target(
    inst: &Institution,
    anchor: &InstitutionalTrustAnchorV0,
    min_grade: AdmissionGradeV0,
) -> (tempfile::TempDir, LocalResourceReservations) {
    let dir = tempfile::tempdir().unwrap();
    let target = LocalResourceReservations::open(dir.path(), inst.custodian)
        .unwrap()
        .with_admission(registry_for(&[anchor]));
    target
        .initialize_pool_v1(institutional_pool(inst, anchor, min_grade))
        .unwrap();
    (dir, target)
}

fn reserve_admitted(
    target: &LocalResourceReservations,
    evidence: &InstitutionalEvidenceV0,
    request: &ReservationRequestV0,
) -> Result<icn_authz_store::ReservationRecordV0, ReservationError> {
    let supplement = InstitutionalSupplementV0 {
        recognitions: evidence.recognitions.clone(),
    };
    target.reserve_admitted(&evidence.authorization, &supplement, request)
}

fn valid_admission(inst: &Institution) -> (InstitutionalEvidenceV0, ReservationRequestV0) {
    let (log, fourth) = honest_history(inst);
    let request = inst.request(OP, 4);
    let evidence = InstitutionalEvidenceV0 {
        authorization: inst.reservation_evidence(&log, &[inst.m(0), inst.m(1), &fourth], &request),
        recognitions: members_and(inst, &[&fourth]),
    };
    (evidence, request)
}

/// A recognised, authorized electorate admits exactly one reservation through the gate; the
/// admission survives restart and its replay through the gate is refused by the target.
#[test]
fn institutional_admission_reserves_once_and_survives_restart() {
    let inst = Institution::new(DOMAIN);
    let (log, fourth) = honest_history(&inst);
    let request = inst.request(OP, 4);
    let evidence = InstitutionalEvidenceV0 {
        authorization: inst.reservation_evidence(&log, &[inst.m(0), inst.m(1), &fourth], &request),
        recognitions: members_and(&inst, &[&fourth]),
    };
    let trust = inst.trust(2, None);
    let dir = tempfile::tempdir().unwrap();
    {
        let target = LocalResourceReservations::open(dir.path(), inst.custodian).unwrap();
        target.initialize_pool(inst.pool(10)).unwrap();
        let reached = std::cell::Cell::new(false);
        let (verified, record) =
            reserve_via_gate(&target, &trust, &evidence, &request, &reached).unwrap();
        assert!(reached.get());
        assert_eq!((record.available_before, record.available_after), (10, 6));
        assert_eq!(verified.domain(), &GovernanceDomainId(DOMAIN.into()));
        assert_eq!(verified.legitimacy(), Legitimacy::RehearsalOnly);
        // Each counted voter carries the GEN-B recognition hash a relying party re-derives.
        assert_eq!(verified.voters().len(), 3);
        for (subject, hash) in verified.voters() {
            let who = [inst.m(0), inst.m(1), &fourth]
                .into_iter()
                .find(|m| &m.subject == subject)
                .unwrap();
            let expected = SubjectRecognitionV1::recognize_bootstrap_rehearsal(
                &GovernanceDomainId(DOMAIN.into()),
                &who.genesis,
            )
            .unwrap();
            assert_eq!(hash, expected.recognition_hash());
        }
    }
    let target = LocalResourceReservations::open(dir.path(), inst.custodian).unwrap();
    assert!(matches!(
        target.reconcile(inst.ctx, OP).unwrap(),
        ReservationReconciliation::Committed { .. }
    ));
    let reached = std::cell::Cell::new(false);
    assert!(matches!(
        reserve_via_gate(&target, &trust, &evidence, &request, &reached),
        Err(InstitutionalAdmissionError::Target(
            ReservationError::Authorization(AdmissionRefusal::Replayed)
        ))
    ));
    assert_eq!(available(&target, inst.ctx), 6);
}

/// A foreign-domain Subject granted standing by an otherwise valid Subject decision is refused by
/// the gate, before the target is invoked. Nothing is consumed or reserved.
#[test]
fn foreign_domain_standing_is_refused_before_any_effect() {
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
    let request = inst.request(OP, 1);
    let authorization =
        inst.reservation_evidence(&log, &[inst.m(0), inst.m(1), inst.m(2)], &request);
    let (_dir, target) = fresh_target(&inst);
    let trust = inst.trust(2, None);

    for (recognitions, expect_invalid) in [
        (members_and(&inst, &[]), false),
        (members_and(&inst, &[&foreign]), true),
    ] {
        let evidence = InstitutionalEvidenceV0 {
            authorization: authorization.clone(),
            recognitions,
        };
        let reached = std::cell::Cell::new(false);
        let result = reserve_via_gate(&target, &trust, &evidence, &request, &reached);
        if expect_invalid {
            assert!(matches!(
                result,
                Err(InstitutionalAdmissionError::Refused(
                    InstitutionalRefusal::InvalidRecognition { index: 3, .. }
                ))
            ));
        } else {
            assert_eq!(
                result.unwrap_err(),
                InstitutionalAdmissionError::Refused(InstitutionalRefusal::UnrecognizedStanding {
                    at: 2,
                    subject: foreign.subject
                })
            );
        }
        assert!(!reached.get(), "target must not be invoked on refusal");
        untouched(&target, &inst, OP);
    }
}

/// A Subject with valid GEN-B recognition by the domain but no decided standing grant cannot vote:
/// recognition is not membership.
#[test]
fn recognized_subject_without_standing_grant_is_refused() {
    let inst = Institution::new(DOMAIN);
    let (log, fourth) = honest_history(&inst);
    let outsider = GenASubject::incept(DOMAIN, 0x07);
    let request = inst.request(OP, 4);
    let evidence = InstitutionalEvidenceV0 {
        authorization: inst.reservation_evidence(
            &log,
            &[inst.m(0), inst.m(1), &outsider],
            &request,
        ),
        recognitions: members_and(&inst, &[&fourth, &outsider]),
    };
    let (_dir, target) = fresh_target(&inst);
    let reached = std::cell::Cell::new(false);
    assert_eq!(
        reserve_via_gate(&target, &inst.trust(2, None), &evidence, &request, &reached).unwrap_err(),
        InstitutionalAdmissionError::Refused(InstitutionalRefusal::Log(
            LogRefusal::IneligibleVoter {
                at: 2,
                subject: outsider.subject
            }
        ))
    );
    assert!(!reached.get());
    untouched(&target, &inst, OP);
}

/// A writer-manufactured standing grant (no Subject acts) is refused by the gate's log replay even
/// when the beneficiary presents valid recognition.
#[test]
fn writer_manufactured_standing_is_refused_by_the_gate() {
    let inst = Institution::new(DOMAIN);
    let intruder = GenASubject::incept(DOMAIN, 0x06);
    let log = vec![inst.genesis_entry()];
    let mut forged = log.clone();
    forged.push(inst.decided(
        &log,
        TransitionV0::GrantStanding {
            subject: intruder.subject,
            role: ROLE.into(),
        },
        &[],
    ));
    let request = inst.request(OP, 1);
    let evidence = InstitutionalEvidenceV0 {
        authorization: inst.authorize(
            &forged,
            inst.tip(&log),
            &[inst.m(0), inst.m(1), &intruder],
            request.effect(inst.custodian).unwrap(),
            OP,
        ),
        recognitions: members_and(&inst, &[&intruder]),
    };
    let (_dir, target) = fresh_target(&inst);
    let reached = std::cell::Cell::new(false);
    // The zero-act grant at seq 2 fails the transition rule: refused at that entry, not by some
    // later check.
    assert_eq!(
        reserve_via_gate(&target, &inst.trust(1, None), &evidence, &request, &reached).unwrap_err(),
        InstitutionalAdmissionError::Refused(InstitutionalRefusal::Log(
            LogRefusal::TransitionNotAuthorized { seq: 2 }
        ))
    );
    assert!(!reached.get());
    untouched(&target, &inst, OP);
}

/// Missing recognition, stale checkpoints, a conflicting pinned history and an empty log all fail
/// closed before the target is invoked.
#[test]
fn missing_or_stale_authority_evidence_fails_closed() {
    let inst = Institution::new(DOMAIN);
    let (log, fourth) = honest_history(&inst);
    let request = inst.request(OP, 4);
    let voters = [inst.m(0), inst.m(1), &fourth];
    let good = inst.reservation_evidence(&log, &voters, &request);
    let (_dir, target) = fresh_target(&inst);

    let stale = inst.authorize(
        &log,
        inst.tip(&log[..1]),
        &[inst.m(0), inst.m(1)],
        request.effect(inst.custodian).unwrap(),
        OP,
    );
    let mut b = vec![inst.genesis_entry()];
    b.push(inst.decided(
        &b,
        TransitionV0::RevokeStanding {
            subject: inst.m(2).subject,
            role: ROLE.into(),
        },
        &[inst.m(0), inst.m(1)],
    ));
    let on_b = inst.reservation_evidence(&b, &[inst.m(0), inst.m(1)], &request);
    let mut empty_log = good.clone();
    empty_log.log.clear();

    let all = members_and(&inst, &[&fourth]);
    let cases: Vec<(&str, InstitutionalEvidenceV0, InstitutionalTrustAnchorV0)> = vec![
        (
            "no recognition at all",
            InstitutionalEvidenceV0 {
                authorization: good.clone(),
                recognitions: vec![],
            },
            inst.trust(2, None),
        ),
        (
            "recognition for the granted member missing",
            InstitutionalEvidenceV0 {
                authorization: good.clone(),
                recognitions: members_and(&inst, &[]),
            },
            inst.trust(2, None),
        ),
        (
            "checkpoint below the anchor frontier",
            InstitutionalEvidenceV0 {
                authorization: stale,
                recognitions: all.clone(),
            },
            inst.trust(2, None),
        ),
        (
            "history conflicting with the anchor pin",
            InstitutionalEvidenceV0 {
                authorization: on_b,
                recognitions: all.clone(),
            },
            inst.trust(1, Some(inst.tip(&log))),
        ),
        (
            "empty log",
            InstitutionalEvidenceV0 {
                authorization: empty_log,
                recognitions: all.clone(),
            },
            inst.trust(2, None),
        ),
    ];
    for (name, evidence, trust) in cases {
        let reached = std::cell::Cell::new(false);
        let result = reserve_via_gate(&target, &trust, &evidence, &request, &reached);
        let refusal = match result {
            Err(InstitutionalAdmissionError::Refused(r)) => r,
            other => panic!("{name}: expected a gate refusal, got {other:?}"),
        };
        match name {
            // The first decision in the log (the grant at seq 2, evaluated over the seq-1
            // electorate) is already gated: every decision is, not only the effect.
            "no recognition at all" => assert!(
                matches!(
                    refusal,
                    InstitutionalRefusal::UnrecognizedStanding { at: 1, .. }
                ),
                "{name}: {refusal:?}"
            ),
            "recognition for the granted member missing" => assert_eq!(
                refusal,
                InstitutionalRefusal::UnrecognizedStanding {
                    at: 2,
                    subject: fourth.subject
                },
                "{name}"
            ),
            "checkpoint below the anchor frontier" => assert_eq!(
                refusal,
                InstitutionalRefusal::Log(LogRefusal::CheckpointBelowFrontier {
                    checkpoint: 1,
                    frontier: 2
                }),
                "{name}"
            ),
            "history conflicting with the anchor pin" => assert_eq!(
                refusal,
                InstitutionalRefusal::Log(LogRefusal::PinnedCheckpointMismatch { seq: 2 }),
                "{name}"
            ),
            "empty log" => assert_eq!(refusal, InstitutionalRefusal::Log(LogRefusal::Empty)),
            _ => unreachable!(),
        }
        assert!(!reached.get(), "{name}: target invoked");
        untouched(&target, &inst, OP);
    }
}

/// Rehearsal recognition is never production legitimacy: an anchor requiring production-grade
/// recognition refuses every v1 presentation.
#[test]
fn production_requirement_refuses_rehearsal_recognition() {
    let inst = Institution::new(DOMAIN);
    let (log, fourth) = honest_history(&inst);
    let request = inst.request(OP, 4);
    let evidence = InstitutionalEvidenceV0 {
        authorization: inst.reservation_evidence(&log, &[inst.m(0), inst.m(1), &fourth], &request),
        recognitions: members_and(&inst, &[&fourth]),
    };
    let production = anchor(
        DOMAIN,
        inst.ctx,
        2,
        None,
        RecognitionRequirement::ProductionGrade,
    );
    let (_dir, target) = fresh_target(&inst);
    let reached = std::cell::Cell::new(false);
    assert_eq!(
        reserve_via_gate(&target, &production, &evidence, &request, &reached).unwrap_err(),
        InstitutionalAdmissionError::Refused(InstitutionalRefusal::RehearsalNotProduction {
            index: 0
        })
    );
    assert!(!reached.get());
    untouched(&target, &inst, OP);
}

/// The requester supplies evidence only. A self-consistent authorization from another context is
/// refused, and the anchor cannot be built without the relying party's own trust inputs.
#[test]
fn requester_cannot_substitute_context_domain_or_frontier() {
    let inst = Institution::new(DOMAIN);
    // Another context whose members are recognised by DOMAIN and whose evidence is internally
    // valid: the presenter would like it to count for `inst`.
    let other = Institution::new(FOREIGN_DOMAIN);
    let (olog, ofourth) = honest_history(&other);
    let request = inst.request(OP, 4);
    let evidence = InstitutionalEvidenceV0 {
        authorization: other.reservation_evidence(
            &olog,
            &[other.m(0), other.m(1), &ofourth],
            &request,
        ),
        recognitions: members_and(&other, &[&ofourth]),
    };
    // Valid under its own anchor...
    assert!(verify_institutional_authorization(&other.trust(2, None), &evidence).is_ok());
    // ...refused under the relying party's.
    let (_dir, target) = fresh_target(&inst);
    let reached = std::cell::Cell::new(false);
    assert_eq!(
        reserve_via_gate(&target, &inst.trust(2, None), &evidence, &request, &reached).unwrap_err(),
        InstitutionalAdmissionError::Refused(InstitutionalRefusal::UntrustedContext)
    );
    assert!(!reached.get());
    untouched(&target, &inst, OP);

    // An anchor is never implicit: the relying party must name a domain and a governing rule.
    use icn_governance::institutional_admission::TrustAnchorError;
    let build = |domain: &str, rules: Vec<ThresholdRuleV0>, pinned| {
        InstitutionalTrustAnchorV0::new(
            GovernanceDomainId(domain.into()),
            inst.ctx,
            2,
            pinned,
            rules,
            RecognitionRequirement::RehearsalPermitted,
        )
    };
    assert_eq!(
        build("", vec![strict()], None),
        Err(TrustAnchorError::EmptyDomain)
    );
    assert_eq!(build(DOMAIN, vec![], None), Err(TrustAnchorError::NoRules));
    assert_eq!(
        build(DOMAIN, vec![strict(), strict()], None),
        Err(TrustAnchorError::DuplicateRule)
    );
    let mut malformed = strict();
    malformed.approval = Ratio { num: 0, den: 3 };
    assert!(matches!(
        build(DOMAIN, vec![malformed], None),
        Err(TrustAnchorError::MalformedRule(_))
    ));
    assert_eq!(
        build(
            DOMAIN,
            vec![strict()],
            Some(LogCheckpointV0 {
                seq: 0,
                head: [0; 32]
            })
        ),
        Err(TrustAnchorError::PinnedAtZero)
    );
}

/// KNOWN LIMIT (pinned so it cannot be forgotten): the target does not know which resolver its
/// pool requires. A custodian that calls `reserve_checkpointed` directly with a domain-blind
/// resolver admits the foreign-standing authorization the gate refuses. Closing this needs the
/// target to bind a pool to a required admission policy (an `icn-authz-store` change).
/// The session-3 bypass, closed. With an institutional-policy pool the direct, ungated call (with
/// the pool's correct `policy_hash`) is refused at the commit boundary, and so is the closure gate
/// (the target cannot tell its resolver apart). The V0 pool keeps its caller-resolved contract.
#[test]
fn direct_ungated_call_is_refused_for_an_institutional_policy_pool() {
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
    let request = inst.request(OP, 1);
    let evidence = InstitutionalEvidenceV0 {
        authorization: inst.reservation_evidence(
            &log,
            &[inst.m(0), inst.m(1), inst.m(2)],
            &request,
        ),
        recognitions: members_and(&inst, &[]),
    };
    let trust = inst.trust(2, None);
    let (_dir, target) = policy_target(&inst, &trust, AdmissionGradeV0::Rehearsal);
    assert_eq!(request.policy_hash, inst.pool(10).policy.policy_hash);

    // Direct, ungated, correct policy hash: refused before anything is committed.
    let direct = target.reserve_checkpointed(
        &evidence.authorization,
        &inst.frontier(2, None),
        &rules(),
        NOW,
        &request,
    );
    assert!(
        matches!(direct, Err(ReservationError::AdmissionDenied(_))),
        "{direct:?}"
    );
    untouched(&target, &inst, OP);

    // The closure gate refuses first here (foreign standing); a valid authorization through it is
    // refused by the pool too (`closure_gate_cannot_satisfy_an_institutional_policy_pool`).
    let reached = std::cell::Cell::new(false);
    assert!(reserve_via_gate(&target, &trust, &evidence, &request, &reached).is_err());
    // The installed verifier refuses the foreign standing.
    let admitted = reserve_admitted(&target, &evidence, &request);
    assert!(
        matches!(admitted, Err(ReservationError::AdmissionRefused(_))),
        "{admitted:?}"
    );
    untouched(&target, &inst, OP);

    // Compatibility: a V0 pool keeps the pre-existing caller-resolved contract.
    let (_dir0, generic) = fresh_target(&inst);
    assert!(generic
        .reserve_checkpointed(
            &evidence.authorization,
            &inst.frontier(2, None),
            &rules(),
            NOW,
            &request
        )
        .is_ok());
    assert_eq!(available(&generic, inst.ctx), 9);
}

/// The resolver the gate hands the target is itself gated: inside a successful admission, the
/// target-side preflight with that resolver refuses a foreign-standing authorization for the same
/// context. Guards against the gate ever passing the target a domain-blind resolver.
#[test]
fn the_resolver_handed_to_the_target_enforces_recognition() {
    let inst = Institution::new(DOMAIN);
    let (log, fourth) = honest_history(&inst);
    let request = inst.request(OP, 4);
    let good = InstitutionalEvidenceV0 {
        authorization: inst.reservation_evidence(&log, &[inst.m(0), inst.m(1), &fourth], &request),
        recognitions: members_and(&inst, &[&fourth]),
    };
    // Same context, an electorate including a foreign-domain Subject.
    let foreign = GenASubject::incept(FOREIGN_DOMAIN, 0x05);
    let mut flog = vec![inst.genesis_entry()];
    flog.push(inst.decided(
        &flog,
        TransitionV0::GrantStanding {
            subject: foreign.subject,
            role: ROLE.into(),
        },
        &[inst.m(0), inst.m(1)],
    ));
    let other_op = [0x0b; 32];
    let freq = inst.request(other_op, 1);
    let fev = inst.reservation_evidence(&flog, &[inst.m(0), inst.m(1), inst.m(2)], &freq);
    // Domain-blind preflight admits it (control).
    assert!(icn_authz::effect_v0::preflight_checkpointed(
        &fev,
        &inst.frontier(2, None),
        &rules(),
        inst.custodian,
        NOW
    )
    .is_ok());
    let (_dir, target) = fresh_target(&inst);
    let (_, record) = admit_institutional(&inst.trust(2, None), &good, |ev, frontier, gated| {
        assert!(matches!(
            icn_authz::effect_v0::preflight_checkpointed(
                &fev,
                frontier,
                gated,
                inst.custodian,
                NOW
            ),
            Err(AdmissionRefusal::Log(LogRefusal::RuleRefused {
                at: 2,
                error: icn_authz::effect_v0::RuleError::IneligibleSubject(s)
            })) if s == foreign.subject
        ));
        target.reserve_checkpointed(ev, frontier, gated, NOW, &request)
    })
    .unwrap();
    assert_eq!(record.available_after, 6);
    assert_eq!(target.inspect_consumed(inst.ctx, other_op).unwrap(), None);
}

/// Two recognition items for one Subject are refused: the presenter must not choose which counts.
#[test]
fn duplicate_recognition_for_one_subject_is_refused() {
    let inst = Institution::new(DOMAIN);
    let (log, fourth) = honest_history(&inst);
    let request = inst.request(OP, 4);
    let evidence = InstitutionalEvidenceV0 {
        authorization: inst.reservation_evidence(&log, &[inst.m(0), inst.m(1), &fourth], &request),
        recognitions: members_and(&inst, &[&fourth, &fourth]),
    };
    assert_eq!(
        verify_institutional_authorization(&inst.trust(2, None), &evidence),
        Err(InstitutionalRefusal::DuplicateRecognition { index: 4 })
    );
}

// ---------------------------------------------------------------------------------------------
// Mandatory admission: adversarial matrix
// ---------------------------------------------------------------------------------------------

#[test]
fn mandatory_admission_reserves_once_and_the_requirement_survives_restart() {
    let inst = Institution::new(DOMAIN);
    let (evidence, request) = valid_admission(&inst);
    let trust = inst.trust(2, None);
    let (dir, target) = policy_target(&inst, &trust, AdmissionGradeV0::Rehearsal);
    let record = reserve_admitted(&target, &evidence, &request).unwrap();
    assert_eq!((record.available_before, record.available_after), (10, 6));
    drop(target);

    let target = LocalResourceReservations::open(dir.path(), inst.custodian)
        .unwrap()
        .with_admission(registry_for(&[&trust]));
    let Some(ResourcePoolRecord::V1(stored)) = target.inspect_pool_record(inst.ctx, POOL).unwrap()
    else {
        panic!("the institutional pool must still be a V1 record after commit and restart");
    };
    assert_eq!(
        stored.admission,
        institutional_pool(&inst, &trust, AdmissionGradeV0::Rehearsal).admission
    );
    assert!(matches!(
        target.reconcile(inst.ctx, OP).unwrap(),
        ReservationReconciliation::Committed { .. }
    ));
    // Replay of the same valid admission allocates nothing.
    assert_eq!(
        reserve_admitted(&target, &evidence, &request),
        Err(ReservationError::Authorization(AdmissionRefusal::Replayed))
    );
    // A fresh operation through the direct path is still refused after restart.
    let request2 = inst.request([0x0b; 32], 1);
    let (log, fourth) = honest_history(&inst);
    // Signed for revision 0 so it passes the caller-resolved preflight; the pool refuses it on
    // its admission requirement, which is checked before the revision.
    let evidence2 = inst.reservation_evidence(&log, &[inst.m(0), inst.m(1), &fourth], &request2);
    assert!(matches!(
        target.reserve_checkpointed(
            &evidence2,
            &inst.frontier(2, None),
            &rules(),
            NOW,
            &request2
        ),
        Err(ReservationError::AdmissionDenied(_))
    ));
    assert_eq!(available(&target, inst.ctx), 6);
}

/// The closure gate hands the target a gated resolver, but the pool cannot verify that; it is
/// refused like any caller-resolved admission. Only the installed verifier satisfies the pool.
#[test]
fn closure_gate_cannot_satisfy_an_institutional_policy_pool() {
    let inst = Institution::new(DOMAIN);
    let (evidence, request) = valid_admission(&inst);
    let trust = inst.trust(2, None);
    let (_dir, target) = policy_target(&inst, &trust, AdmissionGradeV0::Rehearsal);
    let reached = std::cell::Cell::new(false);
    let result = reserve_via_gate(&target, &trust, &evidence, &request, &reached);
    assert!(reached.get());
    assert!(
        matches!(
            result,
            Err(InstitutionalAdmissionError::Target(
                ReservationError::AdmissionDenied(_)
            ))
        ),
        "{result:?}"
    );
    untouched(&target, &inst, OP);
}

#[test]
fn missing_or_unknown_admission_policy_fails_closed() {
    let inst = Institution::new(DOMAIN);
    let (evidence, request) = valid_admission(&inst);
    let trust = inst.trust(2, None);

    // Restarted without the verifier installed.
    let (dir, target) = policy_target(&inst, &trust, AdmissionGradeV0::Rehearsal);
    drop(target);
    let bare = LocalResourceReservations::open(dir.path(), inst.custodian).unwrap();
    assert!(matches!(
        reserve_admitted(&bare, &evidence, &request),
        Err(ReservationError::AdmissionUnavailable(_))
    ));
    untouched(&bare, &inst, OP);
    drop(bare);

    // Only another institution's verifier installed: its policy id differs.
    let other = Institution::new(FOREIGN_DOMAIN);
    let other_trust = anchor(
        FOREIGN_DOMAIN,
        other.ctx,
        2,
        None,
        RecognitionRequirement::RehearsalPermitted,
    );
    assert_ne!(
        InstitutionalAdmissionVerifierV0::policy_ref(&other_trust),
        InstitutionalAdmissionVerifierV0::policy_ref(&trust)
    );
    let wrong = LocalResourceReservations::open(dir.path(), inst.custodian)
        .unwrap()
        .with_admission(registry_for(&[&other_trust]));
    assert!(matches!(
        reserve_admitted(&wrong, &evidence, &request),
        Err(ReservationError::AdmissionUnavailable(_))
    ));
    untouched(&wrong, &inst, OP);

    // A pool that names an unknown version of the right policy.
    let dir2 = tempfile::tempdir().unwrap();
    let future = LocalResourceReservations::open(dir2.path(), inst.custodian)
        .unwrap()
        .with_admission(registry_for(&[&trust]));
    let mut pool = institutional_pool(&inst, &trust, AdmissionGradeV0::Rehearsal);
    pool.admission = AdmissionRequirementV1::Policy {
        policy: AdmissionPolicyRefV0 {
            version: 1,
            ..InstitutionalAdmissionVerifierV0::policy_ref(&trust)
        },
        min_grade: AdmissionGradeV0::Rehearsal,
    };
    future.initialize_pool_v1(pool).unwrap();
    assert!(matches!(
        reserve_admitted(&future, &evidence, &request),
        Err(ReservationError::AdmissionUnavailable(_))
    ));
    untouched(&future, &inst, OP);

    // A supplement of the wrong type is refused by the verifier, not ignored.
    let (_dir3, target) = policy_target(&inst, &trust, AdmissionGradeV0::Rehearsal);
    assert!(matches!(
        target.reserve_admitted(&evidence.authorization, &evidence.recognitions, &request),
        Err(ReservationError::AdmissionRefused(_))
    ));
    untouched(&target, &inst, OP);
}

#[test]
fn admission_for_another_operation_pool_revision_or_context_is_refused() {
    let inst = Institution::new(DOMAIN);
    let (evidence, request) = valid_admission(&inst);
    let trust = inst.trust(2, None);
    let (_dir, target) = policy_target(&inst, &trust, AdmissionGradeV0::Rehearsal);

    let mut other_op = request.clone();
    other_op.operation_id = [0x0c; 32];
    let mut other_revision = request.clone();
    other_revision.expected_pool_revision = 1;
    let mut other_quantity = request.clone();
    other_quantity.quantity = 5;
    for (name, req) in [
        ("operation", &other_op),
        ("revision", &other_revision),
        ("parameters", &other_quantity),
    ] {
        let result = reserve_admitted(&target, &evidence, req);
        assert!(
            matches!(result, Err(ReservationError::InvalidInput(_))),
            "{name}: {result:?}"
        );
        assert_eq!(
            target.inspect_consumed(inst.ctx, req.operation_id).unwrap(),
            None
        );
    }
    untouched(&target, &inst, OP);

    // Another pool under the same requirement: the authorization names POOL, not this one.
    let mut second = institutional_pool(&inst, &trust, AdmissionGradeV0::Rehearsal);
    second.pool.pool_id = [0x7f; 32];
    target.initialize_pool_v1(second).unwrap();
    let mut other_pool = request.clone();
    other_pool.pool_id = [0x7f; 32];
    assert!(matches!(
        reserve_admitted(&target, &evidence, &other_pool),
        Err(ReservationError::InvalidInput(_))
    ));
    untouched(&target, &inst, OP);

    // Another institution's valid authorization, presented to this custodian's pool.
    let foreign = Institution::new(FOREIGN_DOMAIN);
    assert_ne!(foreign.ctx, inst.ctx);
    let (foreign_evidence, _) = valid_admission(&foreign);
    let result = reserve_admitted(&target, &foreign_evidence, &request);
    // Refused by the verifier's first check, before recognition touches the foreign evidence.
    assert!(
        matches!(&result, Err(ReservationError::AdmissionRefused(reason))
            if reason.contains("not for the trusted authority context")),
        "{result:?}"
    );
    untouched(&target, &inst, OP);
}

/// A faulty installed verifier that preflights against its own fixed target and/or clock instead of
/// the ones the store passes. Used to show the store re-asserts those bindings itself.
struct FaultyVerifier {
    inner: InstitutionalAdmissionVerifierV0,
    target: Option<[u8; 32]>,
    now: Option<u64>,
}

impl icn_authz_store::CustodianAdmissionVerifierV0 for FaultyVerifier {
    fn policy(&self) -> AdmissionPolicyRefV0 {
        icn_authz_store::CustodianAdmissionVerifierV0::policy(&self.inner)
    }

    fn preflight(
        &self,
        evidence: &CheckpointedEvidenceV0,
        supplement: &dyn std::any::Any,
        target: [u8; 32],
        now: u64,
    ) -> Result<icn_authz_store::VerifiedAdmissionV0, icn_authz_store::AdmissionRefusalV0> {
        icn_authz_store::CustodianAdmissionVerifierV0::preflight(
            &self.inner,
            evidence,
            supplement,
            self.target.unwrap_or(target),
            self.now.unwrap_or(now),
        )
    }
}

/// An authorization for one custodian cannot commit at another custodian's store, even when that
/// store's installed verifier wrongly preflights it against the original target: the store checks
/// the preflighted target against its own.
#[test]
fn store_refuses_a_preflight_for_another_target_even_from_its_own_verifier() {
    let inst = Institution::new(DOMAIN);
    let (evidence, request) = valid_admission(&inst);
    let trust = inst.trust(2, None);

    let elsewhere = [0x5e; 32];
    assert_ne!(elsewhere, inst.custodian);
    let mut registry = AdmissionRegistryV0::new();
    registry
        .install(std::sync::Arc::new(FaultyVerifier {
            inner: InstitutionalAdmissionVerifierV0::new(trust.clone()),
            target: Some(inst.custodian),
            now: None,
        }))
        .unwrap();
    registry.install_clock(clock_at(NOW)).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = LocalResourceReservations::open(dir.path(), elsewhere)
        .unwrap()
        .with_admission(registry);
    let mut pool = institutional_pool(&inst, &trust, AdmissionGradeV0::Rehearsal);
    pool.pool.custodian = elsewhere;
    store.initialize_pool_v1(pool).unwrap();

    let result = reserve_admitted(&store, &evidence, &request);
    assert!(
        matches!(result, Err(ReservationError::InvalidInput(_))),
        "{result:?}"
    );
    untouched(&store, &inst, OP);
}

/// An expired authorization cannot commit even when the installed verifier wrongly evaluates it at
/// an earlier time: the store checks expiry against the custodian clock's reading itself.
#[test]
fn store_refuses_an_expired_authorization_even_from_its_own_verifier() {
    let inst = Institution::new(DOMAIN);
    let (evidence, request) = valid_admission(&inst);
    let trust = inst.trust(2, None);
    let not_after = evidence.authorization.authorization.body.not_after;

    let mut registry = AdmissionRegistryV0::new();
    registry
        .install(std::sync::Arc::new(FaultyVerifier {
            inner: InstitutionalAdmissionVerifierV0::new(trust.clone()),
            target: None,
            now: Some(NOW),
        }))
        .unwrap();
    // The custodian clock reads one second past expiry; the faulty verifier ignores it.
    registry.install_clock(clock_at(not_after + 1)).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = LocalResourceReservations::open(dir.path(), inst.custodian)
        .unwrap()
        .with_admission(registry);
    store
        .initialize_pool_v1(institutional_pool(
            &inst,
            &trust,
            AdmissionGradeV0::Rehearsal,
        ))
        .unwrap();

    assert!(NOW <= not_after);
    let supplement = InstitutionalSupplementV0 {
        recognitions: evidence.recognitions.clone(),
    };
    let result = store.reserve_admitted(&evidence.authorization, &supplement, &request);
    assert!(
        matches!(result, Err(ReservationError::InvalidInput(_))),
        "{result:?}"
    );
    untouched(&store, &inst, OP);
}

/// The admitted path is not a way into a pool that declares no custodian policy: a V0 pool and an
/// explicit generic V1 pool both refuse it as `AdmissionDenied`, even with a verifier installed
/// and otherwise valid evidence, and nothing is consumed.
#[test]
fn admitted_path_is_refused_by_a_pool_without_a_custodian_policy() {
    let inst = Institution::new(DOMAIN);
    let (evidence, request) = valid_admission(&inst);
    let trust = inst.trust(2, None);

    let dir0 = tempfile::tempdir().unwrap();
    let v0 = LocalResourceReservations::open(dir0.path(), inst.custodian)
        .unwrap()
        .with_admission(registry_for(&[&trust]));
    v0.initialize_pool(inst.pool(10)).unwrap();

    let dir1 = tempfile::tempdir().unwrap();
    let generic_v1 = LocalResourceReservations::open(dir1.path(), inst.custodian)
        .unwrap()
        .with_admission(registry_for(&[&trust]));
    generic_v1
        .initialize_pool_v1(ResourcePoolV1 {
            pool: inst.pool(10),
            admission: AdmissionRequirementV1::CallerResolved,
        })
        .unwrap();

    for (name, target) in [("v0", &v0), ("generic v1", &generic_v1)] {
        let result = reserve_admitted(target, &evidence, &request);
        assert!(
            matches!(result, Err(ReservationError::AdmissionDenied(_))),
            "{name}: {result:?}"
        );
        untouched(target, &inst, OP);
    }
}

#[test]
fn stale_frontier_is_refused_by_the_installed_verifier() {
    let inst = Institution::new(DOMAIN);
    let (evidence, request) = valid_admission(&inst);
    // The custodian's anchor has advanced past the presented checkpoint (tip seq 2).
    let advanced = inst.trust(3, None);
    let (_dir, target) = policy_target(&inst, &advanced, AdmissionGradeV0::Rehearsal);
    let result = reserve_admitted(&target, &evidence, &request);
    assert!(
        matches!(result, Err(ReservationError::AdmissionRefused(_))),
        "{result:?}"
    );
    untouched(&target, &inst, OP);
    // The caller has no way to supply its own, weaker frontier for this pool.
    assert!(matches!(
        target.reserve_checkpointed(
            &evidence.authorization,
            &inst.frontier(0, None),
            &rules(),
            NOW,
            &request
        ),
        Err(ReservationError::AdmissionDenied(_))
    ));
    untouched(&target, &inst, OP);
}

#[test]
fn recognized_but_ineligible_subject_is_refused_at_the_target() {
    let inst = Institution::new(DOMAIN);
    let (log, fourth) = honest_history(&inst);
    let outsider = GenASubject::incept(DOMAIN, 0x07);
    let request = inst.request(OP, 4);
    let evidence = InstitutionalEvidenceV0 {
        authorization: inst.reservation_evidence(
            &log,
            &[inst.m(0), inst.m(1), &outsider],
            &request,
        ),
        recognitions: members_and(&inst, &[&fourth, &outsider]),
    };
    let (_dir, target) = policy_target(&inst, &inst.trust(2, None), AdmissionGradeV0::Rehearsal);
    let result = reserve_admitted(&target, &evidence, &request);
    assert!(
        matches!(result, Err(ReservationError::AdmissionRefused(_))),
        "{result:?}"
    );
    untouched(&target, &inst, OP);
}

#[test]
fn rehearsal_recognition_never_satisfies_a_production_pool() {
    let inst = Institution::new(DOMAIN);
    let (evidence, request) = valid_admission(&inst);

    // Rehearsal-permitting anchor: verification succeeds at Rehearsal grade, and the pool's
    // Production minimum refuses it at commit.
    let (_dir, target) = policy_target(&inst, &inst.trust(2, None), AdmissionGradeV0::Production);
    let result = reserve_admitted(&target, &evidence, &request);
    assert!(
        matches!(result, Err(ReservationError::AdmissionDenied(_))),
        "{result:?}"
    );
    untouched(&target, &inst, OP);

    // Production-grade anchor: every v1 (rehearsal) recognition is refused by the verifier.
    let production = anchor(
        DOMAIN,
        inst.ctx,
        2,
        None,
        RecognitionRequirement::ProductionGrade,
    );
    let (_dir2, target) = policy_target(&inst, &production, AdmissionGradeV0::Production);
    let result = reserve_admitted(&target, &evidence, &request);
    assert!(
        matches!(result, Err(ReservationError::AdmissionRefused(_))),
        "{result:?}"
    );
    untouched(&target, &inst, OP);
}

// ---------------------------------------------------------------------------------------------
// Part 8 F1: custodian-controlled admission time (unix seconds, from the installed clock)
// ---------------------------------------------------------------------------------------------

/// A custodian store over `dir`, its clock fixed at `now` (or none). Initializes the policy pool
/// only when `init` is set, so the same directory can be reopened as after a restart.
fn custodian_store(
    dir: &std::path::Path,
    inst: &Institution,
    anchor: &InstitutionalTrustAnchorV0,
    now: Option<u64>,
    init: bool,
) -> LocalResourceReservations {
    let store = LocalResourceReservations::open(dir, inst.custodian)
        .unwrap()
        .with_admission(registry_at(&[anchor], now));
    if init {
        store
            .initialize_pool_v1(institutional_pool(
                inst,
                anchor,
                AdmissionGradeV0::Rehearsal,
            ))
            .unwrap();
    }
    store
}

/// The expiry boundary is judged at the custodian clock: from the earliest time through
/// `not_after` inclusive it admits, and `reserved_at` is exactly the clock's reading; one second
/// past `not_after` it refuses and consumes nothing.
#[test]
fn custodian_clock_decides_the_expiry_boundary_and_stamps_reserved_at() {
    let inst = Institution::new(DOMAIN);
    let (evidence, request) = valid_admission(&inst);
    let trust = inst.trust(2, None);
    assert_eq!(
        evidence.authorization.authorization.body.not_after,
        NOT_AFTER
    );

    for at in [0, NOW, NOT_AFTER - 1, NOT_AFTER] {
        let dir = tempfile::tempdir().unwrap();
        let store = custodian_store(dir.path(), &inst, &trust, Some(at), true);
        let record = reserve_admitted(&store, &evidence, &request)
            .unwrap_or_else(|e| panic!("clock {at}: {e:?}"));
        assert_eq!(record.reserved_at, at, "reserved_at is the clock's reading");
        assert_eq!(
            store.inspect_reservation(inst.ctx, OP).unwrap(),
            Some(record)
        );
    }

    for at in [NOT_AFTER + 1, NOT_AFTER + 86_400, u64::MAX] {
        let dir = tempfile::tempdir().unwrap();
        let store = custodian_store(dir.path(), &inst, &trust, Some(at), true);
        let result = reserve_admitted(&store, &evidence, &request);
        assert!(
            matches!(result, Err(ReservationError::AdmissionRefused(_))),
            "clock {at}: {result:?}"
        );
        untouched(&store, &inst, OP);
    }
}

/// With no clock installed the custodian path refuses valid evidence as unavailable and consumes
/// nothing, so the same evidence still admits once a clock is installed.
#[test]
fn no_custodian_clock_refuses_valid_evidence_without_consuming_it() {
    let inst = Institution::new(DOMAIN);
    let (evidence, request) = valid_admission(&inst);
    let trust = inst.trust(2, None);
    let dir = tempfile::tempdir().unwrap();

    let unclocked = custodian_store(dir.path(), &inst, &trust, None, true);
    let result = reserve_admitted(&unclocked, &evidence, &request);
    assert!(
        matches!(result, Err(ReservationError::AdmissionUnavailable(_))),
        "{result:?}"
    );
    untouched(&unclocked, &inst, OP);
    drop(unclocked);

    let clocked = custodian_store(dir.path(), &inst, &trust, Some(NOW), false);
    assert_eq!(
        reserve_admitted(&clocked, &evidence, &request)
            .unwrap()
            .reserved_at,
        NOW
    );
}

/// Admission time is not persisted and survives no restart as a decision: each reopened store
/// judges by its own installed clock. An expired-clock refusal stays a refusal after restart,
/// a clockless restart refuses, and a correct clock then admits exactly once.
#[test]
fn restart_keeps_custodian_clock_refusals_and_records() {
    let inst = Institution::new(DOMAIN);
    let (evidence, request) = valid_admission(&inst);
    let trust = inst.trust(2, None);
    let dir = tempfile::tempdir().unwrap();

    let expired = custodian_store(dir.path(), &inst, &trust, Some(NOT_AFTER + 1), true);
    assert!(matches!(
        reserve_admitted(&expired, &evidence, &request),
        Err(ReservationError::AdmissionRefused(_))
    ));
    drop(expired);

    let reopened = custodian_store(dir.path(), &inst, &trust, Some(NOT_AFTER + 1), false);
    assert!(matches!(
        reserve_admitted(&reopened, &evidence, &request),
        Err(ReservationError::AdmissionRefused(_))
    ));
    untouched(&reopened, &inst, OP);
    drop(reopened);

    let clockless = custodian_store(dir.path(), &inst, &trust, None, false);
    assert!(matches!(
        reserve_admitted(&clockless, &evidence, &request),
        Err(ReservationError::AdmissionUnavailable(_))
    ));
    untouched(&clockless, &inst, OP);
    drop(clockless);

    let valid = custodian_store(dir.path(), &inst, &trust, Some(NOW), false);
    let record = reserve_admitted(&valid, &evidence, &request).unwrap();
    assert_eq!(record.reserved_at, NOW);
    drop(valid);

    // After a restart the committed record is unchanged and the operation is not admitted again,
    // whatever the new clock reads.
    for at in [NOW, NOT_AFTER + 1] {
        let after = custodian_store(dir.path(), &inst, &trust, Some(at), false);
        let replay = reserve_admitted(&after, &evidence, &request);
        if at <= NOT_AFTER {
            // In range: the verifier admits, and the store refuses the already-consumed operation.
            assert!(
                matches!(replay, Err(ReservationError::Authorization(_))),
                "{replay:?}"
            );
        } else {
            // Past expiry: the verifier refuses before the store is consulted.
            assert!(
                matches!(replay, Err(ReservationError::AdmissionRefused(_))),
                "{replay:?}"
            );
        }
        assert_eq!(
            after.inspect_reservation(inst.ctx, OP).unwrap(),
            Some(record.clone())
        );
        assert_eq!(available(&after, inst.ctx), 10 - 4);
    }
}

/// Nothing the caller controls on the custodian path (evidence, supplement, request) moves the
/// admission time: with the clock past expiry every combination is refused, and with the clock
/// in range the same valid inputs admit.
#[test]
fn caller_inputs_cannot_move_the_custodian_admission_time() {
    let inst = Institution::new(DOMAIN);
    let (evidence, request) = valid_admission(&inst);
    let trust = inst.trust(2, None);
    let supplement = InstitutionalSupplementV0 {
        recognitions: evidence.recognitions.clone(),
    };
    let dir = tempfile::tempdir().unwrap();
    let expired = custodian_store(dir.path(), &inst, &trust, Some(NOT_AFTER + 1), true);
    let supplements: [&dyn std::any::Any; 4] = [&supplement, &NOW, &(NOW, NOT_AFTER), &()];
    for s in supplements {
        let result = expired.reserve_admitted(&evidence.authorization, s, &request);
        assert!(
            matches!(result, Err(ReservationError::AdmissionRefused(_))),
            "{result:?}"
        );
    }
    untouched(&expired, &inst, OP);
    drop(expired);

    let in_range = custodian_store(dir.path(), &inst, &trust, Some(NOW), false);
    assert!(in_range
        .reserve_admitted(&evidence.authorization, &supplement, &request)
        .is_ok());
}

/// A clock that advances one second on every read, starting at `start`, and counts its reads.
struct SteppingClock {
    start: u64,
    reads: std::sync::atomic::AtomicU64,
}

impl CustodianClockV0 for SteppingClock {
    fn now(&self) -> u64 {
        self.start + self.reads.fetch_add(1, std::sync::atomic::Ordering::SeqCst)
    }
}

/// One reading per call, on the success path too: with a clock that reads `NOT_AFTER` first and
/// `NOT_AFTER + 1` on any later read, the reservation admits, `reserved_at` is the first reading,
/// and the clock was read exactly once. A second reading for the verifier, the store's expiry
/// re-check or `reserved_at` would see an expired or different time.
#[test]
fn one_custodian_clock_reading_serves_verifier_expiry_check_and_reserved_at() {
    let inst = Institution::new(DOMAIN);
    let (evidence, request) = valid_admission(&inst);
    let trust = inst.trust(2, None);
    let clock = std::sync::Arc::new(SteppingClock {
        start: NOT_AFTER,
        reads: std::sync::atomic::AtomicU64::new(0),
    });
    let mut registry = registry_at(&[&trust], None);
    registry.install_clock(clock.clone()).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let store = LocalResourceReservations::open(dir.path(), inst.custodian)
        .unwrap()
        .with_admission(registry);
    store
        .initialize_pool_v1(institutional_pool(
            &inst,
            &trust,
            AdmissionGradeV0::Rehearsal,
        ))
        .unwrap();

    let record = reserve_admitted(&store, &evidence, &request).unwrap();
    assert_eq!(record.reserved_at, NOT_AFTER);
    assert_eq!(clock.reads.load(std::sync::atomic::Ordering::SeqCst), 1);
}
